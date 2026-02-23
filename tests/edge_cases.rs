//! Edge case and boundary tests.
//! Tests unusual but valid scenarios.

mod common;

use common::ollama_helpers::require_ollama;
use common::TestContext;
use factbase::EmbeddingProvider;
use std::fs;

/// Test 11.1: Empty repository
#[tokio::test]
async fn test_empty_repository() {
    require_ollama().await;

    let ctx = TestContext::new("empty");

    // Scan should succeed with no documents
    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 0, "No documents should be added");
    assert_eq!(result.total, 0, "Total should be 0");

    // Search should return empty
    let query_emb = vec![0.0; ctx.config.embedding.dimension];
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert!(results.is_empty(), "Search should return empty");
}

/// Test 11.2: Single document repository
#[tokio::test]
async fn test_single_document_repository() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "single",
        &[(
            "only.md",
            "# Only Document\nThis is the only document in the repo.",
        )],
    );

    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 1, "One document should be added");

    // Verify searchable
    let embedding = ctx.embedding();
    let query_emb = embedding.generate("only document").await.unwrap();
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert_eq!(results.len(), 1, "Should find the single document");
}

/// Test 11.3: Deeply nested directories
#[tokio::test]
async fn test_deeply_nested_directories() {
    require_ollama().await;

    let ctx = TestContext::new("nested");

    // Create 10-level deep nesting
    let mut nested_path = ctx.repo_path.clone();
    for i in 0..10 {
        nested_path = nested_path.join(format!("level{}", i));
    }
    fs::create_dir_all(&nested_path).unwrap();

    // Add documents at various levels
    fs::write(
        ctx.repo_path.join("root.md"),
        "# Root\nRoot level document.",
    )
    .unwrap();
    fs::write(
        ctx.repo_path.join("level0/mid.md"),
        "# Mid\nMid level document.",
    )
    .unwrap();
    fs::write(nested_path.join("deep.md"), "# Deep\nDeep level document.").unwrap();

    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 3, "All 3 documents should be found");

    // Verify types derived from folder names
    let docs = ctx.db.get_documents_for_repo("nested").unwrap();
    let deep_doc = docs.values().find(|d| d.title == "Deep").unwrap();
    assert_eq!(
        deep_doc.doc_type.as_deref(),
        Some("level9"),
        "Type should be derived from parent folder"
    );
}

/// Test 11.4: Unicode in filenames and content
#[tokio::test]
async fn test_unicode_filenames_and_content() {
    require_ollama().await;

    let ctx = TestContext::new("unicode");

    // Unicode filenames
    fs::write(
        ctx.repo_path.join("日本語.md"),
        "# 日本語ドキュメント\n日本語のコンテンツです。",
    )
    .unwrap();
    fs::write(
        ctx.repo_path.join("émilie.md"),
        "# Émilie\nContenu en français avec des accents.",
    )
    .unwrap();
    fs::write(
        ctx.repo_path.join("emoji🎉.md"),
        "# Emoji Document 🎉\nContent with emoji 🚀 and symbols ™️.",
    )
    .unwrap();

    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 3, "All unicode documents should be indexed");

    // Verify searchable
    let docs = ctx.db.get_documents_for_repo("unicode").unwrap();
    assert!(
        docs.values().any(|d| d.title.contains("日本語")),
        "Japanese document should be indexed"
    );
    assert!(
        docs.values().any(|d| d.title.contains("Émilie")),
        "French document should be indexed"
    );
}

/// Test 11.5: Special characters in content
#[tokio::test]
async fn test_special_characters_in_content() {
    require_ollama().await;

    let ctx = TestContext::new("special");

    // Document with code blocks
    fs::write(
        ctx.repo_path.join("code.md"),
        r#"# Code Document

```rust
fn main() {
    println!("Hello, world!");
}
```

```sql
SELECT * FROM users WHERE id = 1;
```
"#,
    )
    .unwrap();

    // Document with HTML comments
    fs::write(
        ctx.repo_path.join("comments.md"),
        r#"# Comments Document

<!-- This is a comment -->
Regular content here.
<!-- Another comment -->
"#,
    )
    .unwrap();

    // Document with tables
    fs::write(
        ctx.repo_path.join("table.md"),
        r#"# Table Document

| Column 1 | Column 2 |
|----------|----------|
| Value 1  | Value 2  |
| Value 3  | Value 4  |
"#,
    )
    .unwrap();

    // Document with special markdown
    fs::write(
        ctx.repo_path.join("special.md"),
        r#"# Special Characters

- Bullet with `inline code`
- **Bold** and *italic*
- [Link](https://example.com)
- > Blockquote
- --- horizontal rule above
"#,
    )
    .unwrap();

    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 4, "All special documents should be indexed");

    // Verify content preserved
    let docs = ctx.db.get_documents_for_repo("special").unwrap();
    let code_doc = docs.values().find(|d| d.title == "Code Document").unwrap();
    assert!(
        code_doc.content.contains("println!"),
        "Code content should be preserved"
    );
}

/// Test 11.6: Very long file paths
#[tokio::test]
async fn test_long_file_paths() {
    require_ollama().await;

    let ctx = TestContext::new("longpath");

    // Create path with long folder names (but within OS limits)
    let long_name = "a".repeat(50);
    let mut nested_path = ctx.repo_path.clone();
    for _ in 0..4 {
        nested_path = nested_path.join(&long_name);
    }
    fs::create_dir_all(&nested_path).unwrap();

    fs::write(
        nested_path.join("document.md"),
        "# Long Path Document\nDocument in deeply nested long-named folders.",
    )
    .unwrap();

    let result = ctx.scan().await.unwrap();
    assert_eq!(result.added, 1, "Document with long path should be indexed");

    // Verify file path stored correctly
    let docs = ctx.db.get_documents_for_repo("longpath").unwrap();
    let doc = docs.values().next().unwrap();
    assert!(
        doc.file_path.contains(&long_name),
        "Long path should be preserved"
    );
}
