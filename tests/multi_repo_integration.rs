//! Integration tests for multi-repository workflows.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use chrono::Utc;
use common::compute_hash;
use common::ollama_helpers::require_ollama;
use common::{create_test_db, create_test_repo, TestContext};
use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    models::{Document, Repository},
    processor::DocumentProcessor,
    scanner::Scanner,
    EmbeddingProvider,
};

// === Task 12.1: Add multiple repositories ===

#[tokio::test]
async fn test_add_multiple_repositories() {
    let (db, _temp) = create_test_db();

    let (repo1, _repo1_dir) = create_test_repo(
        "repo1",
        "First Repo",
        &[("doc1.md", "# Doc 1\nRepo 1 content")],
    );
    let (repo2, _repo2_dir) = create_test_repo(
        "repo2",
        "Second Repo",
        &[("doc2.md", "# Doc 2\nRepo 2 content")],
    );

    db.add_repository(&repo1).unwrap();
    db.add_repository(&repo2).unwrap();

    let repos = db.list_repositories().unwrap();
    assert_eq!(repos.len(), 2);
    assert!(repos.iter().any(|r| r.id == "repo1"));
    assert!(repos.iter().any(|r| r.id == "repo2"));

    let stats = db.list_repositories_with_stats().unwrap();
    assert_eq!(stats.len(), 2);
}

#[tokio::test]
async fn test_add_repository_duplicate_id_fails() {
    let (db, _temp) = create_test_db();

    let (repo, _repo_dir) = create_test_repo("myrepo", "My Repo", &[]);
    db.add_repository(&repo).unwrap();

    let (repo2, _repo2_dir) = create_test_repo("myrepo", "Another Repo", &[]);
    let result = db.add_repository(&repo2);
    assert!(result.is_err());
}

// === Task 12.2: Scan specific repository ===

#[tokio::test]
async fn test_scan_specific_repository() {
    require_ollama().await;

    let ctx1 = TestContext::with_files("repo1", &[("alpha.md", "# Alpha\nFirst repo doc")]);
    let ctx2 = TestContext::with_files("repo2", &[("beta.md", "# Beta\nSecond repo doc")]);

    // Scan only repo1
    ctx1.scan().await.unwrap();

    // Verify repo1 has docs
    let repo1_docs = ctx1.db.get_documents_for_repo("repo1").unwrap();
    assert_eq!(repo1_docs.len(), 1);

    // Scan repo2
    ctx2.scan().await.unwrap();

    let repo2_docs = ctx2.db.get_documents_for_repo("repo2").unwrap();
    assert_eq!(repo2_docs.len(), 1);
}

// === Task 12.3: Search across repos ===

#[tokio::test]
async fn test_search_across_repos() {
    require_ollama().await;

    let (db, _temp) = create_test_db();
    let config = Config::default();

    let (repo1, _repo1_dir) = create_test_repo(
        "repo1",
        "Repo 1",
        &[(
            "rust.md",
            "# Rust Programming\nRust is a systems programming language.",
        )],
    );
    let (repo2, _repo2_dir) = create_test_repo(
        "repo2",
        "Repo 2",
        &[(
            "python.md",
            "# Python Programming\nPython is a scripting language.",
        )],
    );

    db.add_repository(&repo1).unwrap();
    db.add_repository(&repo2).unwrap();

    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    // Index both repos
    for repo in [&repo1, &repo2] {
        for file in scanner.find_markdown_files(&repo.path) {
            let content = std::fs::read_to_string(&file).unwrap();
            let rel_path = file.strip_prefix(&repo.path).unwrap();
            let id = processor
                .extract_id(&content)
                .unwrap_or_else(|| processor.generate_id());
            let title = processor.extract_title(&content, &file);
            let doc_type = processor.derive_type(rel_path, &repo.path);

            let doc = Document {
                id: id.clone(),
                repo_id: repo.id.clone(),
                file_path: rel_path.to_string_lossy().to_string(),
                file_hash: "hash".to_string(),
                title,
                doc_type: Some(doc_type),
                content: content.clone(),
                file_modified_at: None,
                indexed_at: Utc::now(),
                is_deleted: false,
            };
            db.upsert_document(&doc).unwrap();
            let emb = embedding.generate(&content).await.unwrap();
            db.upsert_embedding(&id, &emb).unwrap();
        }
    }

    // Search without repo filter - should find both
    let query_emb = embedding.generate("programming language").await.unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert_eq!(results.len(), 2);

    // Search with repo filter
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, Some("repo1"), None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("Rust"));

    let results = db
        .search_semantic_with_query(&query_emb, 10, None, Some("repo2"), None)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("Python"));
}

// === Task 12.4: Remove repository ===

#[tokio::test]
async fn test_remove_repository() {
    require_ollama().await;

    let ctx = TestContext::with_files("todelete", &[("test.md", "# Test\nTest content")]);
    ctx.scan().await.unwrap();

    // Verify repo exists with docs
    let repos = ctx.db.list_repositories().unwrap();
    assert_eq!(repos.len(), 1);
    let docs = ctx.db.get_documents_for_repo("todelete").unwrap();
    assert_eq!(docs.len(), 1);

    // Remove repo
    let deleted = ctx.db.remove_repository("todelete").unwrap();
    assert_eq!(deleted, 1);

    // Verify repo not in list
    let repos = ctx.db.list_repositories().unwrap();
    assert!(repos.is_empty());

    // Verify documents deleted
    let docs = ctx.db.get_documents_for_repo("todelete").unwrap();
    assert!(docs.is_empty());

    // Verify search excludes removed repo
    let query_emb = ctx.embedding().generate("test").await.unwrap();
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert!(results.is_empty());
}

// === Task 12.5: File watcher multiple repos (simplified - no actual watcher) ===

#[tokio::test]
async fn test_repo_isolation_after_file_change() {
    require_ollama().await;

    let (db, _temp) = create_test_db();
    let config = Config::default();

    let (repo1, repo1_dir) = create_test_repo("repo1", "Repo 1", &[("a.md", "# A\nOriginal")]);
    let (repo2, _repo2_dir) = create_test_repo("repo2", "Repo 2", &[("b.md", "# B\nOriginal")]);

    db.add_repository(&repo1).unwrap();
    db.add_repository(&repo2).unwrap();

    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    // Helper to scan a repo
    async fn scan_repo(
        db: &Database,
        repo: &Repository,
        scanner: &Scanner,
        processor: &DocumentProcessor,
        embedding: &OllamaEmbedding,
    ) {
        for file in scanner.find_markdown_files(&repo.path) {
            let content = std::fs::read_to_string(&file).unwrap();
            let rel_path = file.strip_prefix(&repo.path).unwrap();
            let id = processor
                .extract_id(&content)
                .unwrap_or_else(|| processor.generate_id());
            let title = processor.extract_title(&content, &file);
            let doc_type = processor.derive_type(rel_path, &repo.path);

            let doc = Document {
                id: id.clone(),
                repo_id: repo.id.clone(),
                file_path: rel_path.to_string_lossy().to_string(),
                file_hash: compute_hash(&content),
                title,
                doc_type: Some(doc_type),
                content: content.clone(),
                file_modified_at: None,
                indexed_at: Utc::now(),
                is_deleted: false,
            };
            db.upsert_document(&doc).unwrap();
            let emb = embedding.generate(&content).await.unwrap();
            db.upsert_embedding(&id, &emb).unwrap();
        }
    }

    // Initial scan of both
    scan_repo(&db, &repo1, &scanner, &processor, &embedding).await;
    scan_repo(&db, &repo2, &scanner, &processor, &embedding).await;

    let repo1_docs_before = db.get_documents_for_repo("repo1").unwrap();
    let repo2_docs_before = db.get_documents_for_repo("repo2").unwrap();
    let repo2_hash_before: String = repo2_docs_before.values().next().unwrap().file_hash.clone();
    let repo1_hash_before: String = repo1_docs_before.values().next().unwrap().file_hash.clone();

    // Modify file in repo1
    std::fs::write(repo1_dir.path().join("a.md"), "# A\nModified content").unwrap();

    // Rescan only repo1
    scan_repo(&db, &repo1, &scanner, &processor, &embedding).await;

    // Verify repo2 unchanged
    let repo2_docs_after = db.get_documents_for_repo("repo2").unwrap();
    assert_eq!(repo2_docs_before.len(), repo2_docs_after.len());
    assert_eq!(
        repo2_hash_before,
        repo2_docs_after.values().next().unwrap().file_hash
    );

    // Verify repo1 updated
    let repo1_docs_after = db.get_documents_for_repo("repo1").unwrap();
    assert_eq!(repo1_docs_before.len(), repo1_docs_after.len());
    assert_ne!(
        repo1_hash_before,
        repo1_docs_after.values().next().unwrap().file_hash
    );
}

// === Task 30: Check embedding status ===

#[tokio::test]
async fn test_check_embedding_status() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "check_test",
        &[
            ("doc1.md", "# Document One\nFirst document content."),
            ("doc2.md", "# Document Two\nSecond document content."),
        ],
    );

    // Before scan: no embeddings
    let status = ctx
        .db
        .check_embedding_status("check_test")
        .expect("check should succeed");
    assert_eq!(status.with_embeddings.len(), 0);
    assert_eq!(status.without_embeddings.len(), 0); // No docs indexed yet
    assert_eq!(status.orphaned.len(), 0);

    // After scan: all docs have embeddings
    ctx.scan().await.expect("scan should succeed");

    let status = ctx
        .db
        .check_embedding_status("check_test")
        .expect("check should succeed");
    assert_eq!(status.with_embeddings.len(), 2);
    assert_eq!(status.without_embeddings.len(), 0);
    assert_eq!(status.orphaned.len(), 0);
}

#[tokio::test]
async fn test_get_embedding_dimension() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "dim_test",
        &[(
            "doc.md",
            "# Test Doc\nContent for embedding dimension test.",
        )],
    );

    // Before scan: no dimension
    let dim = ctx
        .db
        .get_embedding_dimension()
        .expect("get dimension should succeed");
    assert!(dim.is_none());

    // After scan: dimension should be 1024 (qwen3-embedding)
    ctx.scan().await.expect("scan should succeed");

    let dim = ctx
        .db
        .get_embedding_dimension()
        .expect("get dimension should succeed");
    assert_eq!(dim, Some(1024));
}
