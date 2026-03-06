//! Integration tests requiring live Ollama instance.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use common::create_test_db;
use common::ollama_helpers::require_ollama;
use factbase::{
    config::Config,
    cosine_similarity,
    database::Database,
    embedding::OllamaEmbedding,
    llm::{LinkDetector, OllamaLlm},
    models::Document,
    processor::DocumentProcessor,
    scanner::Scanner,
    EmbeddingProvider,
};
use tempfile::TempDir;

#[tokio::test]
#[ignore]
async fn test_ollama_availability_check() {
    require_ollama().await;
    // If we get here, Ollama is available
    println!("Ollama is available and responding");
}

#[tokio::test]
#[ignore]
async fn test_embedding_generation() {
    require_ollama().await;

    let config = Config::default();
    let provider = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let embedding = provider.generate("Hello, world!").await;
    assert!(embedding.is_ok(), "Embedding generation failed");

    let vec = embedding.expect("embedding should succeed");
    assert_eq!(vec.len(), 1024, "Expected 1024 dimensions");

    for &v in &vec {
        assert!(v.abs() < 10.0, "Embedding value out of range: {}", v);
    }
}

#[tokio::test]
#[ignore]
async fn test_embedding_various_lengths() {
    require_ollama().await;

    let config = Config::default();
    let provider = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let texts = [
        "Hi",
        "A medium length sentence about testing.",
        "A longer paragraph with multiple sentences testing embedding generation.",
    ];

    for text in texts {
        let result = provider.generate(text).await;
        assert!(result.is_ok(), "Failed for text length {}", text.len());
        assert_eq!(result.expect("embedding should succeed").len(), 1024);
    }
}

#[tokio::test]
#[ignore]
async fn test_embedding_similarity() {
    require_ollama().await;

    let config = Config::default();
    let provider = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let similar1 = provider
        .generate("The cat sat on the mat")
        .await
        .expect("embedding should succeed");
    let similar2 = provider
        .generate("A cat was sitting on a mat")
        .await
        .expect("embedding should succeed");
    let different = provider
        .generate("Quantum physics explains particle behavior")
        .await
        .expect("embedding should succeed");

    let sim_score = cosine_similarity(&similar1, &similar2);
    let diff_score = cosine_similarity(&similar1, &different);

    println!("Similar texts score: {}", sim_score);
    println!("Different texts score: {}", diff_score);

    assert!(
        sim_score > diff_score,
        "Similar texts should have higher similarity"
    );
}

#[tokio::test]
#[ignore]
async fn test_llm_link_detection() {
    require_ollama().await;

    let config = Config::default();
    let (db, _temp) = create_test_db();

    let doc1 = Document {
        id: "abc123".to_string(),
        repo_id: "test".to_string(),
        file_path: "people/alice.md".to_string(),
        file_hash: "hash1".to_string(),
        title: "Alice Smith".to_string(),
        doc_type: Some("person".to_string()),
        content: "# Alice Smith\nA software engineer.".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };
    let doc2 = Document {
        id: "def456".to_string(),
        repo_id: "test".to_string(),
        file_path: "projects/widget.md".to_string(),
        file_hash: "hash2".to_string(),
        title: "Widget Project".to_string(),
        doc_type: Some("project".to_string()),
        content: "# Widget Project\nA cool project.".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };
    db.upsert_document(&doc1)
        .expect("upsert doc1 should succeed");
    db.upsert_document(&doc2)
        .expect("upsert doc2 should succeed");

    let known_entities = vec![
        ("abc123".to_string(), "Alice Smith".to_string()),
        ("def456".to_string(), "Widget Project".to_string()),
    ];

    let detector = LinkDetector::new();

    let content = "# Meeting Notes\nDiscussed the Widget Project with Alice Smith.";
    let links = detector
        .detect_links(content, "source1", &known_entities);

    println!("Detected links: {:?}", links);

    assert!(!links.is_empty(), "Should detect entity mentions");
}

#[tokio::test]
#[ignore]
async fn test_full_scan_with_embeddings() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    std::fs::create_dir(repo_path.join("people")).expect("create people dir");
    std::fs::write(
        repo_path.join("people/john.md"),
        "# John Doe\nA test person.",
    )
    .expect("write john.md");
    std::fs::write(
        repo_path.join("notes.md"),
        "# Notes\nMet with John Doe today.",
    )
    .expect("write notes.md");

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let files = scanner.find_markdown_files(&repo.path);

    assert_eq!(files.len(), 2, "Should find 2 markdown files");

    for file in &files {
        let content = std::fs::read_to_string(file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");

        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");

        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    let query_emb = embedding
        .generate("person named John")
        .await
        .expect("query embedding should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    assert!(!results.is_empty(), "Search should return results");
    println!(
        "Search results: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[ignore]
async fn test_search_finds_relevant_documents() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    // Create diverse documents
    std::fs::create_dir(repo_path.join("people")).expect("create people dir");
    std::fs::create_dir(repo_path.join("projects")).expect("create projects dir");
    std::fs::write(
        repo_path.join("people/alice.md"),
        "# Alice Johnson\nAlice is a software engineer specializing in Rust.",
    )
    .expect("write alice.md");
    std::fs::write(
        repo_path.join("people/bob.md"),
        "# Bob Smith\nBob is a data scientist working on machine learning.",
    )
    .expect("write bob.md");
    std::fs::write(
        repo_path.join("projects/api.md"),
        "# REST API Project\nBuilding a REST API using Rust and Axum framework.",
    )
    .expect("write api.md");

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    for file in scanner.find_markdown_files(&repo.path) {
        let content = std::fs::read_to_string(&file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, &file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");
        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    // Search for Rust-related content
    let query_emb = embedding
        .generate("Rust programming")
        .await
        .expect("query embedding should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    assert!(!results.is_empty());
    // Alice and API project mention Rust, should rank higher
    let titles: Vec<_> = results.iter().map(|r| r.title.as_str()).collect();
    println!("Search 'Rust programming': {:?}", titles);
}

#[tokio::test]
#[ignore]
async fn test_search_with_type_filter() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    std::fs::create_dir(repo_path.join("people")).expect("create people dir");
    std::fs::create_dir(repo_path.join("projects")).expect("create projects dir");
    std::fs::write(
        repo_path.join("people/dev.md"),
        "# Developer\nA software developer.",
    )
    .expect("write people/dev.md");
    std::fs::write(
        repo_path.join("projects/dev.md"),
        "# Dev Tools\nDevelopment tools project.",
    )
    .expect("write projects/dev.md");

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    for file in scanner.find_markdown_files(&repo.path) {
        let content = std::fs::read_to_string(&file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, &file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");
        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    let query_emb = embedding
        .generate("developer")
        .await
        .expect("query embedding should succeed");

    // Filter by person type
    let results = db
        .search_semantic_with_query(&query_emb, 10, Some("person"), None, None)
        .expect("search with person filter should succeed");
    assert!(results
        .iter()
        .all(|r| r.doc_type.as_deref() == Some("person")));
    println!(
        "Person filter results: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );

    // Filter by project type
    let results = db
        .search_semantic_with_query(&query_emb, 10, Some("project"), None, None)
        .expect("search with project filter should succeed");
    assert!(results
        .iter()
        .all(|r| r.doc_type.as_deref() == Some("project")));
    println!(
        "Project filter results: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[ignore]
async fn test_search_no_results() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    std::fs::write(repo_path.join("test.md"), "# Test\nSimple test document.")
        .expect("write test.md");

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("test", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    for file in scanner.find_markdown_files(&repo.path) {
        let content = std::fs::read_to_string(&file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, &file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");
        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    // Search with non-existent type filter
    let query_emb = embedding
        .generate("test")
        .await
        .expect("query embedding should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, Some("nonexistent_type"), None, None)
        .expect("search should succeed");

    // Should return empty, not error
    assert!(
        results.is_empty(),
        "Should return empty for non-matching type filter"
    );
    println!("No results test passed");
}

// === Performance tests ===

#[tokio::test]
#[ignore]
async fn test_scan_100_documents() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    // Generate 100 test files
    std::fs::create_dir(repo_path.join("docs")).expect("create docs dir");
    for i in 0..100 {
        let content = format!("# Document {}\n\nThis is test document number {}. It contains some content for testing embedding generation and search functionality.", i, i);
        std::fs::write(repo_path.join(format!("docs/doc{:03}.md", i)), content)
            .expect("write doc file");
    }

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("perf", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    let start = std::time::Instant::now();
    let files = scanner.find_markdown_files(&repo.path);
    assert_eq!(files.len(), 100);

    for file in &files {
        let content = std::fs::read_to_string(file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");
        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    let elapsed = start.elapsed();
    let per_doc = elapsed.as_millis() as f64 / 100.0;
    println!(
        "Scanned 100 documents in {:?} ({:.1}ms/doc)",
        elapsed, per_doc
    );

    // Should complete in reasonable time (120s = 1200ms/doc max)
    assert!(elapsed.as_secs() < 120, "Scan took too long: {:?}", elapsed);
}

#[tokio::test]
#[ignore]
async fn test_search_latency() {
    require_ollama().await;

    let temp = TempDir::new().expect("create temp dir");
    let repo_path = temp.path();

    // Create 50 documents for search
    std::fs::create_dir(repo_path.join("docs")).expect("create docs dir");
    for i in 0..50 {
        let content = format!(
            "# Topic {}\n\nContent about topic {} with various keywords.",
            i, i
        );
        std::fs::write(repo_path.join(format!("docs/doc{:02}.md", i)), content)
            .expect("write doc file");
    }

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).expect("create .factbase dir");
    let db = Database::new(&db_path).expect("create database");

    let repo = common::test_repo("perf", repo_path.to_path_buf());
    db.upsert_repository(&repo)
        .expect("upsert repository should succeed");

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    // Index all documents
    for file in scanner.find_markdown_files(&repo.path) {
        let content = std::fs::read_to_string(&file).expect("read file");
        let rel_path = file
            .strip_prefix(&repo.path)
            .expect("strip prefix should succeed");
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, &file);
        let doc_type = processor.derive_type(rel_path, &repo.path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc)
            .expect("upsert document should succeed");
        let emb = embedding
            .generate(&content)
            .await
            .expect("embedding should succeed");
        db.upsert_embedding(&id, &emb)
            .expect("upsert embedding should succeed");
    }

    // Measure search latency (embedding generation + vector search)
    let queries = ["topic keywords", "content search", "document test"];
    for query in queries {
        let start = std::time::Instant::now();
        let query_emb = embedding
            .generate(query)
            .await
            .expect("query embedding should succeed");
        let emb_time = start.elapsed();

        let search_start = std::time::Instant::now();
        let results = db
            .search_semantic_with_query(&query_emb, 10, None, None, None)
            .expect("search should succeed");
        let search_time = search_start.elapsed();

        let total = start.elapsed();
        println!(
            "Query '{}': embedding={:?}, search={:?}, total={:?}, results={}",
            query,
            emb_time,
            search_time,
            total,
            results.len()
        );

        // Vector search should be fast (<100ms)
        assert!(
            search_time.as_millis() < 100,
            "Search too slow: {:?}",
            search_time
        );
        // Total including embedding should be <1000ms
        assert!(
            total.as_millis() < 1000,
            "Total query too slow: {:?}",
            total
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_link_detection_scale() {
    require_ollama().await;

    let config = Config::default();

    // Create 30 known entities
    let known_entities: Vec<(String, String)> = (0..30)
        .map(|i| (format!("id{:02}", i), format!("Entity Number {}", i)))
        .collect();

    let detector = LinkDetector::new();

    let content = "# Test Document\n\nThis document mentions Entity Number 5 and Entity Number 15. It also references Entity Number 25.";

    let start = std::time::Instant::now();
    let links = detector
        .detect_links(content, "source", &known_entities);
    let elapsed = start.elapsed();

    println!("Link detection with 30 entities: {:?}", elapsed);
    println!("Detected {} links: {:?}", links.len(), links);

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 30,
        "Link detection too slow: {:?}",
        elapsed
    );
}

#[tokio::test]
#[ignore]
async fn test_merge_planning() {
    use factbase::organize::{plan_merge, FactDestination};

    require_ollama().await;

    let config = Config::default();
    let (db, _temp) = create_test_db();

    // Create a test repository
    let repo = common::test_repo("test", std::path::PathBuf::from("/tmp/test"));
    db.upsert_repository(&repo).expect("upsert repo");

    // Create two similar documents about the same person
    let doc1 = Document {
        id: "doc1".to_string(),
        repo_id: "test".to_string(),
        title: "John Smith".to_string(),
        doc_type: Some("person".to_string()),
        file_path: "people/john.md".to_string(),
        content: "# John Smith\n\n- Software engineer at Acme Corp\n- Lives in Austin".to_string(),
        file_hash: "hash1".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    let doc2 = Document {
        id: "doc2".to_string(),
        repo_id: "test".to_string(),
        title: "John Smith Profile".to_string(),
        doc_type: Some("person".to_string()),
        file_path: "people/john-profile.md".to_string(),
        content: "# John Smith Profile\n\n- Works at Acme Corp\n- Has PhD in Computer Science"
            .to_string(),
        file_hash: "hash2".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    db.upsert_document(&doc1).expect("upsert doc1");
    db.upsert_document(&doc2).expect("upsert doc2");

    // Create LLM provider
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);

    // Plan the merge
    let start = std::time::Instant::now();
    let plan = plan_merge("doc1", &["doc2"], &db, &llm).await;
    let elapsed = start.elapsed();

    println!("Merge planning took: {:?}", elapsed);
    assert!(plan.is_ok(), "Merge planning failed: {:?}", plan.err());

    let plan = plan.expect("plan should succeed");

    // Verify the plan
    assert_eq!(plan.keep_id, "doc1");
    assert_eq!(plan.merge_ids, vec!["doc2"]);
    assert!(plan.is_valid(), "Plan should be balanced");

    // Check fact counts
    let total_facts = plan.ledger.source_facts.len();
    println!("Total facts: {}", total_facts);
    assert!(total_facts >= 4, "Should have at least 4 facts");

    // Check destination counts
    let counts = plan.ledger.destination_counts();
    println!("Destination counts: {:?}", counts);

    // All facts should be assigned
    assert!(plan.ledger.unaccounted_facts().is_empty());

    // Should have some facts going to document
    let doc_count = counts.get(&FactDestination::Document).unwrap_or(&0);
    assert!(*doc_count > 0, "Should have facts assigned to document");

    // Combined content should include merged facts
    println!("Combined content:\n{}", plan.combined_content);
    assert!(plan.combined_content.contains("John Smith"));

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 60,
        "Merge planning too slow: {:?}",
        elapsed
    );
}

#[tokio::test]
#[ignore]
async fn test_split_detection() {
    use factbase::organize::detect_split_candidates;

    require_ollama().await;

    let config = Config::default();
    let (db, _temp) = create_test_db();

    // Create a test repository
    let repo = common::test_repo("test", std::path::PathBuf::from("/tmp/test"));
    db.upsert_repository(&repo).expect("upsert repo");

    // Create a document with distinct sections that should be split
    let multi_topic_doc = Document {
        id: "multi".to_string(),
        repo_id: "test".to_string(),
        title: "Mixed Topics".to_string(),
        doc_type: Some("note".to_string()),
        file_path: "notes/mixed.md".to_string(),
        content: r#"# Mixed Topics

## Software Engineering

Software engineering is the systematic application of engineering approaches
to the development of software. It involves requirements analysis, design,
implementation, testing, and maintenance of software systems.

## Cooking Recipes

Here are some delicious recipes for home cooking. Start with fresh ingredients
and follow the steps carefully. Baking requires precise measurements while
savory dishes allow more flexibility in seasoning.

## Quantum Physics

Quantum mechanics describes nature at the smallest scales of energy levels
of atoms and subatomic particles. The wave function provides probability
amplitudes for different quantum states.
"#
        .to_string(),
        file_hash: "hash_multi".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    // Create a document with related sections that should NOT be split
    let single_topic_doc = Document {
        id: "single".to_string(),
        repo_id: "test".to_string(),
        title: "Software Development".to_string(),
        doc_type: Some("note".to_string()),
        file_path: "notes/software.md".to_string(),
        content: r#"# Software Development

## Frontend Development

Frontend development focuses on the user interface and user experience.
Technologies include HTML, CSS, JavaScript, and frameworks like React.

## Backend Development

Backend development handles server-side logic, databases, and APIs.
Common languages include Python, Java, Node.js, and Go.

## DevOps Practices

DevOps combines development and operations for continuous integration
and deployment. Tools include Docker, Kubernetes, and CI/CD pipelines.
"#
        .to_string(),
        file_hash: "hash_single".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    db.upsert_document(&multi_topic_doc).expect("upsert multi");
    db.upsert_document(&single_topic_doc)
        .expect("upsert single");

    // Create embedding provider
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Detect split candidates with threshold 0.5
    let start = std::time::Instant::now();
    let candidates = detect_split_candidates(
        &db,
        &embedding,
        0.5,
        Some("test"),
        &factbase::ProgressReporter::Silent,
    )
    .await;
    let elapsed = start.elapsed();

    println!("Split detection took: {:?}", elapsed);
    assert!(
        candidates.is_ok(),
        "Split detection failed: {:?}",
        candidates.err()
    );

    let candidates = candidates.expect("candidates should succeed");
    println!("Found {} split candidates", candidates.len());

    for candidate in &candidates {
        println!(
            "  {} ({}): avg_sim={:.3}, min_sim={:.3}, sections={}",
            candidate.doc_title,
            candidate.doc_id,
            candidate.avg_similarity,
            candidate.min_similarity,
            candidate.sections.len()
        );
        println!("    Rationale: {}", candidate.rationale);
    }

    // The multi-topic document should be a split candidate (distinct topics)
    // The single-topic document may or may not be, depending on embedding similarity
    let multi_candidate = candidates.iter().find(|c| c.doc_id == "multi");
    assert!(
        multi_candidate.is_some(),
        "Multi-topic document should be a split candidate"
    );

    let multi = multi_candidate.expect("multi should exist");
    assert_eq!(multi.sections.len(), 3, "Should have 3 sections");
    assert!(
        multi.avg_similarity < 0.5,
        "Distinct topics should have low similarity"
    );

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 60,
        "Split detection too slow: {:?}",
        elapsed
    );
}

#[tokio::test]
#[ignore]
async fn test_split_planning() {
    use factbase::organize::{extract_sections, plan_split, SplitSection};

    require_ollama().await;

    let config = Config::default();
    let (db, _temp) = create_test_db();

    // Create a test repository
    let repo = common::test_repo("test", std::path::PathBuf::from("/tmp/test"));
    db.upsert_repository(&repo).expect("upsert repo");

    // Create a document with distinct sections to split
    let doc = Document {
        id: "split_me".to_string(),
        repo_id: "test".to_string(),
        title: "Person Profile".to_string(),
        doc_type: Some("person".to_string()),
        file_path: "people/person.md".to_string(),
        content: r#"<!-- factbase:split_me -->
# Person Profile

## Career

- CTO at Acme Corp @t[2020..2022]
- VP Engineering at BigCo @t[2022..]
- Founded startup in 2019

## Education

- PhD in Computer Science from MIT
- BS in Mathematics from Stanford
- Attended coding bootcamp in 2015
"#
        .to_string(),
        file_hash: "hash_split".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    db.upsert_document(&doc).expect("upsert doc");

    // Extract sections from the document
    let sections = extract_sections(&doc.content);
    let valid_sections: Vec<SplitSection> = sections
        .into_iter()
        .filter(|s| s.content.len() >= 50)
        .collect();

    assert!(
        valid_sections.len() >= 2,
        "Should have at least 2 valid sections"
    );

    // Create LLM provider
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);

    // Plan the split
    let start = std::time::Instant::now();
    let plan = plan_split("split_me", &valid_sections, &db, &llm).await;
    let elapsed = start.elapsed();

    println!("Split planning took: {:?}", elapsed);
    assert!(plan.is_ok(), "Split planning failed: {:?}", plan.err());

    let plan = plan.expect("plan should succeed");
    println!("Split plan:");
    println!("  Source: {}", plan.source_id);
    println!("  New documents: {}", plan.document_count());
    println!("  Orphans: {}", plan.orphan_count());
    println!("  Ledger balanced: {}", plan.is_valid());

    for doc in &plan.new_documents {
        println!("  - {} (from section: {})", doc.title, doc.section_title);
    }

    // Verify the plan
    assert!(plan.is_valid(), "Plan ledger should be balanced");
    assert!(
        plan.document_count() >= 2,
        "Should create at least 2 documents"
    );

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 120,
        "Split planning too slow: {:?}",
        elapsed
    );
}

#[tokio::test]
#[ignore]
async fn test_split_execution() {
    use factbase::organize::{execute_split, extract_sections, plan_split, SplitSection};
    use std::fs;

    require_ollama().await;

    let config = Config::default();
    let temp = TempDir::new().expect("create temp dir");
    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).expect("create database");

    // Create a test repository
    let repo_path = temp.path().join("repo");
    fs::create_dir_all(&repo_path).expect("create repo dir");

    let repo = common::test_repo("test", repo_path.clone());
    db.upsert_repository(&repo).expect("upsert repo");

    // Create a document file with distinct sections
    let doc_content = r#"<!-- factbase:split_me -->
# Person Profile

## Career

- CTO at Acme Corp @t[2020..2022]
- VP Engineering at BigCo @t[2022..]
- Founded startup in 2019

## Education

- PhD in Computer Science from MIT
- BS in Mathematics from Stanford
- Attended coding bootcamp in 2015
"#;
    let doc_path = repo_path.join("person.md");
    fs::write(&doc_path, doc_content).expect("write doc");

    // Create document in database
    let doc = Document {
        id: "split_me".to_string(),
        repo_id: "test".to_string(),
        title: "Person Profile".to_string(),
        doc_type: Some("person".to_string()),
        file_path: "person.md".to_string(),
        content: doc_content.to_string(),
        file_hash: "hash_split".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };
    db.upsert_document(&doc).expect("upsert doc");

    // Extract sections from the document
    let sections = extract_sections(&doc.content);
    let valid_sections: Vec<SplitSection> = sections
        .into_iter()
        .filter(|s| s.content.len() >= 50)
        .collect();

    assert!(
        valid_sections.len() >= 2,
        "Should have at least 2 valid sections"
    );

    // Create LLM provider and plan the split
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let plan = plan_split("split_me", &valid_sections, &db, &llm)
        .await
        .expect("plan should succeed");

    println!("Split plan created:");
    println!("  New documents: {}", plan.document_count());
    println!("  Orphans: {}", plan.orphan_count());

    // Execute the split
    let result = execute_split(&plan, &db, &repo_path);
    assert!(result.is_ok(), "Split execution failed: {:?}", result.err());

    let result = result.expect("result should succeed");
    println!("Split execution result:");
    println!("  Source: {}", result.source_id);
    println!("  New doc IDs: {:?}", result.new_doc_ids);
    println!("  Facts distributed: {}", result.fact_count);
    println!("  Orphans: {}", result.orphan_count);

    // Verify source file was deleted
    assert!(!doc_path.exists(), "Source file should be deleted");

    // Verify new files were created
    assert!(
        !result.new_doc_ids.is_empty(),
        "Should have created new documents"
    );

    // Count markdown files in repo (excluding _orphans.md)
    let md_files: Vec<_> = fs::read_dir(&repo_path)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "md") && e.file_name() != "_orphans.md"
        })
        .collect();

    assert!(
        md_files.len() >= 2,
        "Should have at least 2 new markdown files, found {}",
        md_files.len()
    );

    // Verify new files have factbase headers
    for entry in &md_files {
        let content = fs::read_to_string(entry.path()).expect("read file");
        assert!(
            content.starts_with("<!-- factbase:"),
            "New file should have factbase header: {}",
            entry.path().display()
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_misplaced_detection() {
    use factbase::organize::detect_misplaced;

    require_ollama().await;

    let config = Config::default();
    let (db, _temp) = create_test_db();

    // Create a test repository
    let repo = common::test_repo("test", std::path::PathBuf::from("/tmp/test"));
    db.upsert_repository(&repo).expect("upsert repo");

    // Create embedding provider
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Create documents of type "person" (about people)
    let person_docs = vec![
        (
            "person1",
            "John Smith",
            "John Smith is a software engineer with 10 years of experience in backend development.",
        ),
        (
            "person2",
            "Jane Doe",
            "Jane Doe is a product manager who has led multiple successful product launches.",
        ),
        (
            "person3",
            "Bob Wilson",
            "Bob Wilson is a data scientist specializing in machine learning and AI.",
        ),
    ];

    // Create documents of type "project" (about projects)
    let project_docs = vec![
        (
            "project1",
            "Project Alpha",
            "Project Alpha is a web application for managing customer relationships.",
        ),
        (
            "project2",
            "Project Beta",
            "Project Beta is a mobile app for tracking fitness and health metrics.",
        ),
        (
            "project3",
            "Project Gamma",
            "Project Gamma is an API platform for integrating third-party services.",
        ),
    ];

    // Create a misplaced document: person content in project folder
    let misplaced_doc = Document {
        id: "misplaced".to_string(),
        repo_id: "test".to_string(),
        title: "Alice Johnson".to_string(),
        doc_type: Some("project".to_string()), // Wrong type!
        file_path: "projects/alice.md".to_string(),
        content: "Alice Johnson is a senior architect with expertise in distributed systems and cloud infrastructure.".to_string(),
        file_hash: "hash_misplaced".to_string(),
        file_modified_at: None,
        indexed_at: chrono::Utc::now(),
        is_deleted: false,
    };

    // Insert all documents and generate embeddings
    for (id, title, content) in &person_docs {
        let doc = Document {
            id: id.to_string(),
            repo_id: "test".to_string(),
            title: title.to_string(),
            doc_type: Some("person".to_string()),
            file_path: format!("people/{}.md", id),
            content: content.to_string(),
            file_hash: format!("hash_{}", id),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).expect("upsert person doc");
        let emb = embedding
            .generate(content)
            .await
            .expect("generate embedding");
        db.upsert_embedding(id, &emb).expect("upsert embedding");
    }

    for (id, title, content) in &project_docs {
        let doc = Document {
            id: id.to_string(),
            repo_id: "test".to_string(),
            title: title.to_string(),
            doc_type: Some("project".to_string()),
            file_path: format!("projects/{}.md", id),
            content: content.to_string(),
            file_hash: format!("hash_{}", id),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).expect("upsert project doc");
        let emb = embedding
            .generate(content)
            .await
            .expect("generate embedding");
        db.upsert_embedding(id, &emb).expect("upsert embedding");
    }

    // Insert misplaced document
    db.upsert_document(&misplaced_doc)
        .expect("upsert misplaced doc");
    let misplaced_emb = embedding
        .generate(&misplaced_doc.content)
        .await
        .expect("generate embedding");
    db.upsert_embedding("misplaced", &misplaced_emb)
        .expect("upsert embedding");

    // Detect misplaced documents
    let start = std::time::Instant::now();
    let candidates = detect_misplaced(&db, Some("test"), &factbase::ProgressReporter::Silent);
    let elapsed = start.elapsed();

    println!("Misplaced detection took: {:?}", elapsed);
    assert!(
        candidates.is_ok(),
        "Misplaced detection failed: {:?}",
        candidates.err()
    );

    let candidates = candidates.expect("candidates should succeed");
    println!("Found {} misplaced candidates", candidates.len());

    for candidate in &candidates {
        println!(
            "  {} ({}): current='{}', suggested='{}', confidence={:.3}",
            candidate.doc_title,
            candidate.doc_id,
            candidate.current_type,
            candidate.suggested_type,
            candidate.confidence
        );
        println!("    Rationale: {}", candidate.rationale);
    }

    // The misplaced document should be detected
    let misplaced_candidate = candidates.iter().find(|c| c.doc_id == "misplaced");
    assert!(
        misplaced_candidate.is_some(),
        "Misplaced document should be detected"
    );

    let candidate = misplaced_candidate.expect("misplaced should exist");
    assert_eq!(candidate.current_type, "project");
    assert_eq!(candidate.suggested_type, "person");
    assert!(candidate.confidence > 0.0, "Confidence should be positive");

    // Should complete quickly (no LLM calls, just embedding comparisons)
    assert!(
        elapsed.as_secs() < 10,
        "Misplaced detection too slow: {:?}",
        elapsed
    );
}
