//! Export/import workflow integration tests.

use super::common::ollama_helpers::require_ollama;
use super::common::run_scan;
use chrono::Utc;
use factbase::{
    config::Config, database::Database, embedding::OllamaEmbedding, models::Repository,
    EmbeddingProvider,
};
use std::fs;
use tempfile::TempDir;

/// Test 13.3: export -> import workflow
#[tokio::test]
#[ignore]
async fn test_export_import_workflow() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("source");
    let export_path = temp_dir.path().join("export");
    let import_repo_path = temp_dir.path().join("imported");

    fs::create_dir_all(repo_path.join("docs")).expect("operation should succeed");
    fs::create_dir_all(&export_path).expect("operation should succeed");
    fs::create_dir_all(&import_repo_path).expect("operation should succeed");

    // Create source documents
    fs::write(
        repo_path.join("docs/doc1.md"),
        "# Document 1\nFirst document content.",
    )
    .expect("operation should succeed");
    fs::write(
        repo_path.join("docs/doc2.md"),
        "# Document 2\nSecond document content.",
    )
    .expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "source".into(),
        name: "Source".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    // Scan source
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Simulate export - copy files
    let docs = db
        .get_documents_for_repo("source")
        .expect("operation should succeed");
    for doc in docs.values() {
        let src = repo_path.join(&doc.file_path);
        let dst_dir = export_path.join(
            std::path::Path::new(&doc.file_path)
                .parent()
                .unwrap_or(std::path::Path::new("")),
        );
        fs::create_dir_all(&dst_dir).expect("operation should succeed");
        let dst = export_path.join(&doc.file_path);
        fs::copy(&src, &dst).expect("operation should succeed");
    }

    // Verify export
    assert!(
        export_path.join("docs/doc1.md").exists(),
        "Exported doc1 should exist"
    );
    assert!(
        export_path.join("docs/doc2.md").exists(),
        "Exported doc2 should exist"
    );

    // Simulate import - copy to new repo
    let mut md_files = Vec::new();
    let mut stack = vec![export_path.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .expect("read_dir should succeed")
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                md_files.push(path);
            }
        }
    }
    for path in &md_files {
        let rel = path
            .strip_prefix(&export_path)
            .expect("operation should succeed");
        let dst = import_repo_path.join(rel);
        fs::create_dir_all(dst.parent().expect("operation should succeed"))
            .expect("operation should succeed");
        fs::copy(path, &dst).expect("operation should succeed");
    }

    // Add imported repo
    let import_repo = Repository {
        id: "imported".into(),
        name: "Imported".into(),
        path: import_repo_path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&import_repo)
        .expect("operation should succeed");

    // Scan imported
    let result = run_scan(&import_repo, &db, &config)
        .await
        .expect("operation should succeed");
    assert_eq!(result.added, 2, "Should import 2 documents");

    // Create embedding provider for search
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Verify search works on imported
    let query_emb = embedding
        .generate("document content")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, Some("imported"), None, None)
        .expect("operation should succeed");
    assert_eq!(results.len(), 2, "Should find both imported documents");
}

/// Test 13.3b: export --format yaml (YAML serialization test)
#[test]
fn test_export_yaml_format() {
    // Test YAML serialization logic without database
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct ExportDoc {
        id: String,
        title: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        doc_type: Option<String>,
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        links_to: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        linked_from: Vec<String>,
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct ExportWrapper {
        documents: Vec<ExportDoc>,
    }

    // Create test documents with multi-line content
    let docs = vec![
        ExportDoc {
            id: "abc123".into(),
            title: "Alice".into(),
            doc_type: Some("person".into()),
            content: "<!-- factbase:abc123 -->\n# Alice\nAlice is a software engineer.\nShe works on backend systems.".into(),
            links_to: vec!["def456".into()],
            linked_from: vec![],
        },
        ExportDoc {
            id: "def456".into(),
            title: "Project Notes".into(),
            doc_type: None,
            content: "<!-- factbase:def456 -->\n# Project Notes\nThis is a knowledge base about the team.".into(),
            links_to: vec![],
            linked_from: vec!["abc123".into()],
        },
    ];

    let wrapper = ExportWrapper { documents: docs };

    // Serialize to YAML
    let yaml_content =
        serde_yaml_ng::to_string(&wrapper).expect("YAML serialization should succeed");

    // Verify YAML is valid by parsing it back
    let parsed: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&yaml_content).expect("YAML should be valid");

    // Verify structure
    let documents = parsed
        .get("documents")
        .expect("should have documents key")
        .as_sequence()
        .expect("documents should be a sequence");
    assert_eq!(documents.len(), 2, "Should have 2 documents");

    // Verify document fields
    for doc in documents {
        assert!(doc.get("id").is_some(), "Document should have id");
        assert!(doc.get("title").is_some(), "Document should have title");
        assert!(doc.get("content").is_some(), "Document should have content");
    }

    // Verify multi-line content is preserved
    let alice_doc = documents
        .iter()
        .find(|d| {
            d.get("title")
                .and_then(|t| t.as_str())
                .map(|t| t == "Alice")
                .unwrap_or(false)
        })
        .expect("Should find Alice document");
    let alice_content = alice_doc
        .get("content")
        .and_then(|c| c.as_str())
        .expect("Alice should have content");
    assert!(
        alice_content.contains("software engineer"),
        "Content should be preserved"
    );
    assert!(
        alice_content.contains("backend systems"),
        "Multi-line content should be preserved"
    );

    // Verify type field is present for person doc
    assert!(
        alice_doc.get("type").is_some(),
        "Alice should have type field"
    );
    assert_eq!(
        alice_doc.get("type").and_then(|t| t.as_str()),
        Some("person"),
        "Alice type should be 'person'"
    );

    // Verify links are serialized
    let alice_links = alice_doc.get("links_to");
    assert!(alice_links.is_some(), "Alice should have links_to");
    let links_array = alice_links
        .unwrap()
        .as_sequence()
        .expect("links_to should be array");
    assert_eq!(links_array.len(), 1, "Alice should have 1 outgoing link");

    // Verify empty links are omitted (skip_serializing_if)
    let project_doc = documents
        .iter()
        .find(|d| {
            d.get("title")
                .and_then(|t| t.as_str())
                .map(|t| t == "Project Notes")
                .unwrap_or(false)
        })
        .expect("Should find Project Notes document");
    assert!(
        project_doc.get("links_to").is_none(),
        "Empty links_to should be omitted"
    );

    // Verify type is omitted when None
    assert!(
        project_doc.get("type").is_none(),
        "None type should be omitted"
    );

    // Verify roundtrip - parse back to struct
    let roundtrip: ExportWrapper =
        serde_yaml_ng::from_str(&yaml_content).expect("Roundtrip should succeed");
    assert_eq!(roundtrip.documents.len(), 2);
    assert_eq!(roundtrip.documents[0].id, "abc123");
    assert_eq!(roundtrip.documents[0].title, "Alice");
    assert_eq!(roundtrip.documents[0].doc_type, Some("person".into()));
}
