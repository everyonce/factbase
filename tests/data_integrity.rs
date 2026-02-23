//! Data integrity and consistency tests.
//! Verifies data remains consistent across operations.

mod common;

use chrono::Utc;
use common::ollama_helpers::require_ollama;
use common::run_scan;
use factbase::{
    config::Config, database::Database, embedding::OllamaEmbedding, models::Repository,
    EmbeddingProvider,
};
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

/// Test 12.1: Document ID stability
#[tokio::test]
async fn test_document_id_stability() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    // Create documents
    fs::write(repo_path.join("doc1.md"), "# Document 1\nOriginal content.")
        .expect("operation should succeed");
    fs::write(repo_path.join("doc2.md"), "# Document 2\nOriginal content.")
        .expect("operation should succeed");

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    // Initial scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Record IDs
    let docs_before = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let ids_before: HashSet<String> = docs_before.keys().cloned().collect();

    // Modify documents
    fs::write(repo_path.join("doc1.md"), "# Document 1\nModified content.")
        .expect("operation should succeed");
    fs::write(repo_path.join("doc2.md"), "# Document 2\nModified content.")
        .expect("operation should succeed");

    // Rescan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Verify IDs unchanged
    let docs_after = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let ids_after: HashSet<String> = docs_after.keys().cloned().collect();

    assert_eq!(
        ids_before, ids_after,
        "Document IDs should remain stable after modification"
    );

    // Verify content updated
    for (id, doc) in &docs_after {
        assert!(
            doc.content.contains("Modified"),
            "Document {} content should be updated",
            id
        );
    }
}

/// Test 12.2: Embedding-document consistency
#[tokio::test]
async fn test_embedding_document_consistency() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    fs::write(
        repo_path.join("doc.md"),
        "# Test Document\nOriginal content about software engineering.",
    )
    .expect("operation should succeed");

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Initial scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Verify embedding exists
    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let doc_id = docs.keys().next().expect("operation should succeed");

    let query_emb = embedding
        .generate("software engineering")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("operation should succeed");
    assert!(
        results.iter().any(|r| &r.id == doc_id),
        "Document should be found via embedding search"
    );

    // Modify document with different content
    fs::write(
        repo_path.join("doc.md"),
        "# Test Document\nNew content about machine learning and AI.",
    )
    .expect("operation should succeed");

    // Rescan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Verify embedding updated - new content should be more relevant
    let ml_query = embedding
        .generate("machine learning AI")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&ml_query, 10, None, None, None)
        .expect("operation should succeed");
    assert!(
        results.iter().any(|r| &r.id == doc_id),
        "Document should be found with updated embedding"
    );
}

/// Test 12.3: Link validity
#[tokio::test]
async fn test_link_validity() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("people")).expect("operation should succeed");
    fs::create_dir_all(repo_path.join("projects")).expect("operation should succeed");

    // Create documents with cross-references
    fs::write(
        repo_path.join("people/alice.md"),
        "# Alice\nAlice works on the API Project.",
    )
    .expect("operation should succeed");
    fs::write(
        repo_path.join("projects/api.md"),
        "# API Project\nLed by Alice.",
    )
    .expect("operation should succeed");

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    // Scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Get all document IDs
    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let doc_ids: HashSet<String> = docs.keys().cloned().collect();

    // Verify all links point to existing documents
    for id in docs.keys() {
        let links = db.get_links_from(id).expect("operation should succeed");
        for link in &links {
            assert!(
                doc_ids.contains(&link.target_id),
                "Link from {} to {} should point to existing document",
                id,
                link.target_id
            );
        }
    }

    // Delete a document
    fs::remove_file(repo_path.join("people/alice.md")).expect("operation should succeed");

    // Rescan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Verify links to deleted document are removed
    let remaining_docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let active_docs: Vec<_> = remaining_docs.values().filter(|d| !d.is_deleted).collect();

    for doc in &active_docs {
        let links = db
            .get_links_from(&doc.id)
            .expect("operation should succeed");
        for link in &links {
            // Links should only point to non-deleted documents
            if let Some(target) = remaining_docs.get(&link.target_id) {
                assert!(
                    !target.is_deleted,
                    "Link should not point to deleted document"
                );
            }
        }
    }
}

/// Test 12.4: No orphaned records
#[tokio::test]
async fn test_no_orphaned_records() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    // Create documents
    for i in 0..5 {
        fs::write(
            repo_path.join(format!("doc{}.md", i)),
            format!("# Document {}\nContent for document {}.", i, i),
        )
        .expect("operation should succeed");
    }

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    // Initial scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Delete some documents
    fs::remove_file(repo_path.join("doc1.md")).expect("operation should succeed");
    fs::remove_file(repo_path.join("doc3.md")).expect("operation should succeed");

    // Rescan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Get all documents (including deleted)
    let all_docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let active_doc_ids: HashSet<String> = all_docs
        .iter()
        .filter(|(_, d)| !d.is_deleted)
        .map(|(id, _)| id.clone())
        .collect();

    // Verify embeddings only exist for active documents
    let zero_emb = vec![0.0; config.embedding.dimension];
    let search_results = db
        .search_semantic_with_query(&zero_emb, 100, None, None, None)
        .expect("operation should succeed");

    for result in &search_results {
        assert!(
            active_doc_ids.contains(&result.id),
            "Embedding for {} should belong to active document",
            result.id
        );
    }

    // Verify count matches
    assert_eq!(
        active_doc_ids.len(),
        3,
        "Should have 3 active documents (5 - 2 deleted)"
    );
}

/// Test document hash consistency
#[tokio::test]
async fn test_document_hash_consistency() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let content = "# Test Document\nConsistent content.";
    fs::write(repo_path.join("doc.md"), content).expect("operation should succeed");

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    // First scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    let docs_first = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let hash_first = docs_first
        .values()
        .next()
        .expect("operation should succeed")
        .file_hash
        .clone();

    // Second scan without changes
    let result = run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");
    assert_eq!(result.unchanged, 1, "Document should be unchanged");

    let docs_second = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let hash_second = docs_second
        .values()
        .next()
        .expect("operation should succeed")
        .file_hash
        .clone();

    assert_eq!(
        hash_first, hash_second,
        "Hash should be consistent for unchanged content"
    );

    // Modify content
    fs::write(
        repo_path.join("doc.md"),
        "# Test Document\nModified content.",
    )
    .expect("operation should succeed");

    // Third scan
    let result = run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");
    assert_eq!(result.updated, 1, "Document should be updated");

    let docs_third = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let hash_third = docs_third
        .values()
        .next()
        .expect("operation should succeed")
        .file_hash
        .clone();

    assert_ne!(
        hash_first, hash_third,
        "Hash should change for modified content"
    );
}
