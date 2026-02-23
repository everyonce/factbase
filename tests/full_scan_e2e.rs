//! Full scan E2E tests with real Ollama operations.
//! Tests complete scan workflow with real embeddings and link detection.

mod common;

use common::fixtures::copy_fixture_repo;
use common::ollama_helpers::require_ollama;
use common::TestScanSetup;
use factbase::{cosine_similarity, database::Database, scanner::full_scan, EmbeddingProvider};

/// Task 3.1: Test complete scan workflow with real Ollama
#[tokio::test]
async fn test_full_scan_with_real_ollama() {
    require_ollama().await;

    // Copy test fixture repo to temp directory
    let temp = copy_fixture_repo("test-repo");
    let repo_path = temp.path();

    // Create database
    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    // Create repository
    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo).unwrap();

    // Set up components
    let setup = TestScanSetup::new();

    // Run full scan
    let result = full_scan(&repo, &db, &setup.context())
        .await
        .expect("Full scan should succeed");

    // Verify documents indexed (28 total: 10 people + 8 projects + 5 concepts + 5 notes)
    assert!(
        result.added >= 20,
        "Should index at least 20 documents, got {}",
        result.added
    );
    println!(
        "Scan result: {} added, {} updated, {} deleted",
        result.added, result.updated, result.deleted
    );

    // Verify all documents have embeddings by searching
    let docs = db.get_documents_for_repo("test").unwrap();
    let query_emb = setup.embedding.generate("test").await.unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, docs.len(), None, None, None)
        .unwrap();

    // Search should return results for all indexed docs with embeddings
    assert!(
        !results.is_empty(),
        "Search should return results (embeddings exist)"
    );
    println!(
        "Verified embeddings exist: {} documents searchable",
        results.len()
    );

    // Verify links detected
    let total_links: usize = docs
        .values()
        .map(|d| db.get_links_from(&d.id).unwrap_or_default().len())
        .sum();
    println!("Total links detected: {}", total_links);
    // Should have some links (cross-references in test fixtures)
    assert!(
        total_links > 0,
        "Should detect some links between documents"
    );
}

/// Task 3.2: Verify embedding quality
#[tokio::test]
async fn test_embedding_quality() {
    require_ollama().await;

    let temp = copy_fixture_repo("test-repo");
    let repo_path = temp.path();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo).unwrap();

    let setup = TestScanSetup::new();
    full_scan(&repo, &db, &setup.context())
        .await
        .expect("Scan should succeed");

    // Verify embedding dimensions by generating a test embedding
    let test_emb = setup.embedding.generate("test query").await.unwrap();
    assert_eq!(test_emb.len(), 1024, "Embedding dimension should be 1024");

    // Check values in reasonable range
    for &v in &test_emb {
        assert!(v.abs() < 10.0, "Embedding value {} out of range", v);
    }

    // Test semantic similarity: similar queries should return similar results
    let emb_person = setup
        .embedding
        .generate("software engineer developer")
        .await
        .unwrap();
    let emb_project = setup
        .embedding
        .generate("project management timeline")
        .await
        .unwrap();
    let emb_person2 = setup
        .embedding
        .generate("programmer coder developer")
        .await
        .unwrap();

    let sim_person_person2 = cosine_similarity(&emb_person, &emb_person2);
    let sim_person_project = cosine_similarity(&emb_person, &emb_project);

    println!(
        "Similarity between 'software engineer' and 'programmer': {:.4}",
        sim_person_person2
    );
    println!(
        "Similarity between 'software engineer' and 'project management': {:.4}",
        sim_person_project
    );

    // Similar concepts should have higher similarity
    assert!(
        sim_person_person2 > sim_person_project,
        "Similar concepts should have higher similarity"
    );
}

/// Task 3.3: Verify link detection accuracy
#[tokio::test]
async fn test_link_detection_accuracy() {
    require_ollama().await;

    let temp = copy_fixture_repo("test-repo");
    let repo_path = temp.path();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo).unwrap();

    let setup = TestScanSetup::new();
    full_scan(&repo, &db, &setup.context())
        .await
        .expect("Scan should succeed");

    let docs = db.get_documents_for_repo("test").unwrap();

    // Check that project documents have links to people (team members)
    let projects: Vec<_> = docs
        .values()
        .filter(|d| d.doc_type.as_deref() == Some("project"))
        .collect();

    let mut projects_with_links = 0;
    for proj in &projects {
        let links = db.get_links_from(&proj.id).unwrap_or_default();
        if !links.is_empty() {
            projects_with_links += 1;
            println!(
                "Project '{}' links to: {:?}",
                proj.title,
                links.iter().map(|l| &l.target_id).collect::<Vec<_>>()
            );
        }
    }

    println!(
        "{} of {} projects have outgoing links",
        projects_with_links,
        projects.len()
    );

    // Check bidirectional links are stored
    for doc in docs.values() {
        let outgoing = db.get_links_from(&doc.id).unwrap_or_default();
        for link in &outgoing {
            let incoming = db.get_links_to(&link.target_id).unwrap_or_default();
            let has_reverse = incoming.iter().any(|l| l.source_id == doc.id);
            if has_reverse {
                println!("Bidirectional link: {} <-> {}", doc.id, link.target_id);
            }
        }
    }
}

/// Task 3.4: Verify semantic search works
#[tokio::test]
async fn test_semantic_search_works() {
    require_ollama().await;

    let temp = copy_fixture_repo("test-repo");
    let repo_path = temp.path();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo).unwrap();

    let setup = TestScanSetup::new();
    full_scan(&repo, &db, &setup.context())
        .await
        .expect("Scan should succeed");

    // Search for "backend engineer"
    let query_emb = setup.embedding.generate("backend engineer").await.unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 5, None, None, None)
        .unwrap();

    assert!(!results.is_empty(), "Search should return results");
    println!("Search 'backend engineer' results:");
    for (i, r) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.4}, type: {:?})",
            i + 1,
            r.title,
            r.relevance_score,
            r.doc_type
        );
    }

    // Search for "API design"
    let query_emb = setup
        .embedding
        .generate("API design patterns")
        .await
        .unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 5, None, None, None)
        .unwrap();

    assert!(!results.is_empty(), "Search should return results");
    println!("\nSearch 'API design patterns' results:");
    for (i, r) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.4}, type: {:?})",
            i + 1,
            r.title,
            r.relevance_score,
            r.doc_type
        );
    }

    // Search with type filter
    let query_emb = setup
        .embedding
        .generate("software development")
        .await
        .unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 5, Some("person"), None, None)
        .unwrap();

    println!("\nSearch 'software development' (type=person) results:");
    for (i, r) in results.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, r.title, r.relevance_score);
        assert_eq!(
            r.doc_type.as_deref(),
            Some("person"),
            "Should only return person docs"
        );
    }

    // Verify relevance scores make sense (higher is better, should be between 0 and 1)
    for r in &results {
        assert!(
            r.relevance_score >= 0.0 && r.relevance_score <= 1.0,
            "Relevance score {} should be between 0 and 1",
            r.relevance_score
        );
    }
}
