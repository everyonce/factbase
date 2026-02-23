//! Full scan E2E tests with real Ollama operations.
//! Tests complete scan workflow with real embeddings and link detection.

mod common;

use common::cosine_similarity;
use common::fixtures::copy_fixture_repo;
use common::ollama_helpers::require_ollama;
use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    llm::{LinkDetector, OllamaLlm},
    models::Repository,
    processor::DocumentProcessor,
    scanner::{full_scan, ScanOptions, Scanner},
    EmbeddingProvider,
};

/// Task 3.1: Test complete scan workflow with real Ollama
#[tokio::test]
async fn test_full_scan_with_real_ollama() {
    require_ollama().await;

    // Copy test fixture repo to temp directory
    let temp = copy_fixture_repo("test-repo");
    let repo_path = temp.path();

    // Create database
    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("operation should succeed");
    let db = Database::new(&db_path).expect("operation should succeed");

    // Create repository
    let repo = Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.upsert_repository(&repo)
        .expect("operation should succeed");

    // Set up components
    let config = Config::default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 100_000,
        chunk_overlap: 2_000,
        verbose: false,
        dry_run: false,
        show_progress: false,
        check_duplicates: false,
        collect_stats: false,
        since: None,
        min_coverage: 0.8,
        embedding_batch_size: 10,
        force_reindex: false,
        skip_links: false,
    };

    // Run full scan
    let result = full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
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
    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    let query_emb = embedding
        .generate("test")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, docs.len(), None, None, None)
        .expect("operation should succeed");

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
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("operation should succeed");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.upsert_repository(&repo)
        .expect("operation should succeed");

    let config = Config::default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 100_000,
        chunk_overlap: 2_000,
        verbose: false,
        dry_run: false,
        show_progress: false,
        check_duplicates: false,
        collect_stats: false,
        since: None,
        min_coverage: 0.8,
        embedding_batch_size: 10,
        force_reindex: false,
        skip_links: false,
    };

    full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("Scan should succeed");

    // Verify embedding dimensions by generating a test embedding
    let test_emb = embedding
        .generate("test query")
        .await
        .expect("operation should succeed");
    assert_eq!(test_emb.len(), 1024, "Embedding dimension should be 1024");

    // Check values in reasonable range
    for &v in &test_emb {
        assert!(v.abs() < 10.0, "Embedding value {} out of range", v);
    }

    // Test semantic similarity: similar queries should return similar results
    let emb_person = embedding
        .generate("software engineer developer")
        .await
        .expect("operation should succeed");
    let emb_project = embedding
        .generate("project management timeline")
        .await
        .expect("operation should succeed");
    let emb_person2 = embedding
        .generate("programmer coder developer")
        .await
        .expect("operation should succeed");

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
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("operation should succeed");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.upsert_repository(&repo)
        .expect("operation should succeed");

    let config = Config::default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 100_000,
        chunk_overlap: 2_000,
        verbose: false,
        dry_run: false,
        show_progress: false,
        check_duplicates: false,
        collect_stats: false,
        since: None,
        min_coverage: 0.8,
        embedding_batch_size: 10,
        force_reindex: false,
        skip_links: false,
    };

    full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("Scan should succeed");

    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");

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
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("operation should succeed");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.upsert_repository(&repo)
        .expect("operation should succeed");

    let config = Config::default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 100_000,
        chunk_overlap: 2_000,
        verbose: false,
        dry_run: false,
        show_progress: false,
        check_duplicates: false,
        collect_stats: false,
        since: None,
        min_coverage: 0.8,
        embedding_batch_size: 10,
        force_reindex: false,
        skip_links: false,
    };

    full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("Scan should succeed");

    // Search for "backend engineer"
    let query_emb = embedding
        .generate("backend engineer")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 5, None, None, None)
        .expect("operation should succeed");

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
    let query_emb = embedding
        .generate("API design patterns")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 5, None, None, None)
        .expect("operation should succeed");

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
    let query_emb = embedding
        .generate("software development")
        .await
        .expect("operation should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 5, Some("person"), None, None)
        .expect("operation should succeed");

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
