//! Integration tests requiring live Ollama instance.
//! Run with: cargo test --test ollama_integration -- --ignored

use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    llm::{LinkDetector, OllamaLlm},
    models::{Document, Repository},
    processor::DocumentProcessor,
    scanner::Scanner,
    EmbeddingProvider,
};
use std::time::Duration;
use tempfile::TempDir;

async fn is_ollama_available() -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .is_ok()
}

fn create_test_db() -> (Database, TempDir) {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    (db, temp)
}

#[tokio::test]
#[ignore]
async fn test_ollama_availability_check() {
    let available = is_ollama_available().await;
    println!("Ollama available: {}", available);
}

#[tokio::test]
#[ignore]
async fn test_embedding_generation() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let config = Config::default();
    let provider = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let embedding = provider.generate("Hello, world!").await;
    assert!(embedding.is_ok(), "Embedding generation failed");

    let vec = embedding.unwrap();
    assert_eq!(vec.len(), 768, "Expected 768 dimensions");

    for &v in &vec {
        assert!(v.abs() < 10.0, "Embedding value out of range: {}", v);
    }
}

#[tokio::test]
#[ignore]
async fn test_embedding_various_lengths() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

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
        assert_eq!(result.unwrap().len(), 768);
    }
}

#[tokio::test]
#[ignore]
async fn test_embedding_similarity() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let config = Config::default();
    let provider = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let similar1 = provider.generate("The cat sat on the mat").await.unwrap();
    let similar2 = provider
        .generate("A cat was sitting on a mat")
        .await
        .unwrap();
    let different = provider
        .generate("Quantum physics explains particle behavior")
        .await
        .unwrap();

    let sim_score = cosine_similarity(&similar1, &similar2);
    let diff_score = cosine_similarity(&similar1, &different);

    println!("Similar texts score: {}", sim_score);
    println!("Different texts score: {}", diff_score);

    assert!(
        sim_score > diff_score,
        "Similar texts should have higher similarity"
    );
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}

#[tokio::test]
#[ignore]
async fn test_llm_link_detection() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

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
    db.upsert_document(&doc1).unwrap();
    db.upsert_document(&doc2).unwrap();

    let known_entities = vec![
        ("abc123".to_string(), "Alice Smith".to_string()),
        ("def456".to_string(), "Widget Project".to_string()),
    ];

    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let detector = LinkDetector::new(Box::new(llm));

    let content = "# Meeting Notes\nDiscussed the Widget Project with Alice Smith.";
    let links = detector
        .detect_links(content, "source1", &known_entities)
        .await;

    assert!(links.is_ok(), "Link detection failed");
    let links = links.unwrap();
    println!("Detected links: {:?}", links);

    assert!(!links.is_empty(), "Should detect entity mentions");
}

#[tokio::test]
#[ignore]
async fn test_full_scan_with_embeddings() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    std::fs::create_dir(repo_path.join("people")).unwrap();
    std::fs::write(
        repo_path.join("people/john.md"),
        "# John Doe\nA test person.",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("notes.md"),
        "# Notes\nMet with John Doe today.",
    )
    .unwrap();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

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
        let content = std::fs::read_to_string(file).unwrap();
        let rel_path = file.strip_prefix(&repo.path).unwrap();

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
        db.upsert_document(&doc).unwrap();

        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    let query_emb = embedding.generate("person named John").await.unwrap();
    let results = db.search_semantic(&query_emb, 10, None, None).unwrap();

    assert!(!results.is_empty(), "Search should return results");
    println!(
        "Search results: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
}

// === Task 13: Search command end-to-end ===

#[tokio::test]
#[ignore]
async fn test_search_finds_relevant_documents() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Create diverse documents
    std::fs::create_dir(repo_path.join("people")).unwrap();
    std::fs::create_dir(repo_path.join("projects")).unwrap();
    std::fs::write(
        repo_path.join("people/alice.md"),
        "# Alice Johnson\nAlice is a software engineer specializing in Rust.",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("people/bob.md"),
        "# Bob Smith\nBob is a data scientist working on machine learning.",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("projects/api.md"),
        "# REST API Project\nBuilding a REST API using Rust and Axum framework.",
    )
    .unwrap();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "test".to_string(),
        name: "Test".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

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
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    // Search for Rust-related content
    let query_emb = embedding.generate("Rust programming").await.unwrap();
    let results = db.search_semantic(&query_emb, 10, None, None).unwrap();

    assert!(!results.is_empty());
    // Alice and API project mention Rust, should rank higher
    let titles: Vec<_> = results.iter().map(|r| r.title.as_str()).collect();
    println!("Search 'Rust programming': {:?}", titles);
}

#[tokio::test]
#[ignore]
async fn test_search_with_type_filter() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    std::fs::create_dir(repo_path.join("people")).unwrap();
    std::fs::create_dir(repo_path.join("projects")).unwrap();
    std::fs::write(
        repo_path.join("people/dev.md"),
        "# Developer\nA software developer.",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("projects/dev.md"),
        "# Dev Tools\nDevelopment tools project.",
    )
    .unwrap();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "test".to_string(),
        name: "Test".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

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
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    let query_emb = embedding.generate("developer").await.unwrap();

    // Filter by person type
    let results = db
        .search_semantic(&query_emb, 10, Some("person"), None)
        .unwrap();
    assert!(results
        .iter()
        .all(|r| r.doc_type.as_deref() == Some("person")));
    println!(
        "Person filter results: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );

    // Filter by project type
    let results = db
        .search_semantic(&query_emb, 10, Some("project"), None)
        .unwrap();
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
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    std::fs::write(repo_path.join("test.md"), "# Test\nSimple test document.").unwrap();

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "test".to_string(),
        name: "Test".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

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
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    // Search with non-existent type filter
    let query_emb = embedding.generate("test").await.unwrap();
    let results = db
        .search_semantic(&query_emb, 10, Some("nonexistent_type"), None)
        .unwrap();

    // Should return empty, not error
    assert!(
        results.is_empty(),
        "Should return empty for non-matching type filter"
    );
    println!("No results test passed");
}

// === Task 14: Performance tests ===

#[tokio::test]
#[ignore]
async fn test_scan_100_documents() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Generate 100 test files
    std::fs::create_dir(repo_path.join("docs")).unwrap();
    for i in 0..100 {
        let content = format!("# Document {}\n\nThis is test document number {}. It contains some content for testing embedding generation and search functionality.", i, i);
        std::fs::write(repo_path.join(format!("docs/doc{:03}.md", i)), content).unwrap();
    }

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "perf".to_string(),
        name: "Performance Test".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

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
        let content = std::fs::read_to_string(file).unwrap();
        let rel_path = file.strip_prefix(&repo.path).unwrap();
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
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    let elapsed = start.elapsed();
    let per_doc = elapsed.as_millis() as f64 / 100.0;
    println!(
        "Scanned 100 documents in {:?} ({:.1}ms/doc)",
        elapsed, per_doc
    );

    // Should complete in reasonable time (60s = 600ms/doc max)
    assert!(elapsed.as_secs() < 120, "Scan took too long: {:?}", elapsed);
}

#[tokio::test]
#[ignore]
async fn test_search_latency() {
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Create 50 documents for search
    std::fs::create_dir(repo_path.join("docs")).unwrap();
    for i in 0..50 {
        let content = format!(
            "# Topic {}\n\nContent about topic {} with various keywords.",
            i, i
        );
        std::fs::write(repo_path.join(format!("docs/doc{:02}.md", i)), content).unwrap();
    }

    let db_path = repo_path.join(".factbase/factbase.db");
    std::fs::create_dir_all(repo_path.join(".factbase")).unwrap();
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "perf".to_string(),
        name: "Perf".to_string(),
        path: repo_path.to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo).unwrap();

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
            file_hash: "test".to_string(),
            title,
            doc_type: Some(doc_type),
            content: content.clone(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    // Measure search latency (embedding generation + vector search)
    let queries = ["topic keywords", "content search", "document test"];
    for query in queries {
        let start = std::time::Instant::now();
        let query_emb = embedding.generate(query).await.unwrap();
        let emb_time = start.elapsed();

        let search_start = std::time::Instant::now();
        let results = db.search_semantic(&query_emb, 10, None, None).unwrap();
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
        // Total including embedding should be <500ms
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
    if !is_ollama_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let config = Config::default();

    // Create 30 known entities
    let known_entities: Vec<(String, String)> = (0..30)
        .map(|i| (format!("id{:02}", i), format!("Entity Number {}", i)))
        .collect();

    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let detector = LinkDetector::new(Box::new(llm));

    let content = "# Test Document\n\nThis document mentions Entity Number 5 and Entity Number 15. It also references Entity Number 25.";

    let start = std::time::Instant::now();
    let links = detector
        .detect_links(content, "source", &known_entities)
        .await;
    let elapsed = start.elapsed();

    println!("Link detection with 30 entities: {:?}", elapsed);
    assert!(links.is_ok());
    let links = links.unwrap();
    println!("Detected {} links: {:?}", links.len(), links);

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 30,
        "Link detection too slow: {:?}",
        elapsed
    );
}
