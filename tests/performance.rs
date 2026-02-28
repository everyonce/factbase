//! Performance and stress tests.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use common::ollama_helpers::require_ollama;
use factbase::{
    config::Config, database::Database, embedding::OllamaEmbedding, mcp::McpServer,
    models::Document, processor::DocumentProcessor, scanner::Scanner, watcher::FileWatcher,
    EmbeddingProvider,
};
use reqwest::Client;
use serde_json::json;
use std::fs;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::oneshot;

// === Task 14.1: 1000 document repository ===

#[tokio::test]
#[ignore]
async fn test_scan_1000_documents() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("large-repo");
    fs::create_dir_all(repo_path.join("docs")).unwrap();

    // Generate 1000 test files
    println!("Generating 1000 test files...");
    let gen_start = Instant::now();
    for i in 0..1000 {
        let content = format!(
            "# Document {}\n\nThis is test document number {}. It contains content about topic {} for testing embedding generation and search functionality.\n\nKeywords: test, document, performance, benchmark",
            i, i, i % 50
        );
        fs::write(repo_path.join(format!("docs/doc{:04}.md", i)), content).unwrap();
    }
    println!("Generated 1000 files in {:?}", gen_start.elapsed());

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("large", repo_path.clone());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    // Time the scan
    println!("Starting scan of 1000 documents...");
    let scan_start = Instant::now();
    let files = scanner.find_markdown_files(&repo_path);
    assert_eq!(files.len(), 1000);

    let mut indexed = 0;
    for file in &files {
        let content = fs::read_to_string(file).unwrap();
        let rel_path = file.strip_prefix(&repo_path).unwrap();
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, file);
        let doc_type = processor.derive_type(rel_path, &repo_path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "hash".into(),
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

        indexed += 1;
        if indexed % 100 == 0 {
            println!("Indexed {}/1000 documents...", indexed);
        }
    }

    let scan_elapsed = scan_start.elapsed();
    let per_doc_ms = scan_elapsed.as_millis() as f64 / 1000.0;
    println!(
        "Scanned 1000 documents in {:?} ({:.1}ms/doc)",
        scan_elapsed, per_doc_ms
    );

    // Time search queries
    println!("\nTesting search performance...");
    let queries = ["document test", "topic performance", "benchmark keywords"];
    for query in queries {
        let search_start = Instant::now();
        let query_emb = embedding.generate(query).await.unwrap();
        let emb_time = search_start.elapsed();

        let db_start = Instant::now();
        let results = db
            .search_semantic_with_query(&query_emb, 10, None, None, None)
            .unwrap();
        let db_time = db_start.elapsed();

        println!(
            "Query '{}': embedding={:?}, db_search={:?}, results={}",
            query,
            emb_time,
            db_time,
            results.len()
        );

        // DB search should be fast (<100ms)
        assert!(
            db_time.as_millis() < 100,
            "DB search too slow: {:?}",
            db_time
        );
    }
}

// === Task 14.2: Concurrent MCP requests ===

#[tokio::test]
#[ignore]
async fn test_concurrent_mcp_requests() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    // Create some test documents
    for i in 0..20 {
        fs::write(
            repo_path.join(format!("doc{}.md", i)),
            format!("# Document {}\nContent for document {}.", i, i),
        )
        .unwrap();
    }

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.clone());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    // Index documents
    for file in scanner.find_markdown_files(&repo_path) {
        let content = fs::read_to_string(&file).unwrap();
        let rel_path = file.strip_prefix(&repo_path).unwrap();
        let id = processor
            .extract_id(&content)
            .unwrap_or_else(|| processor.generate_id());
        let title = processor.extract_title(&content, &file);
        let doc_type = processor.derive_type(rel_path, &repo_path);

        let doc = Document {
            id: id.clone(),
            repo_id: repo.id.clone(),
            file_path: rel_path.to_string_lossy().to_string(),
            file_hash: "hash".into(),
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

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db,
        embedding,
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
        None,
    );
    let base_url = format!("http://127.0.0.1:{}", port);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        server.start(shutdown_rx).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send concurrent requests
    println!("Sending 50 concurrent list_entities requests...");
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..50 {
        let client = client.clone();
        let url = format!("{}/mcp", base_url);
        handles.push(tokio::spawn(async move {
            let req_start = Instant::now();
            let resp = client
                .post(&url)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "method": "tools/call",
                    "params": {"name": "list_entities", "arguments": {}}
                }))
                .send()
                .await;
            (i, req_start.elapsed(), resp.is_ok())
        }));
    }

    let mut latencies = Vec::new();
    let mut failures = 0;
    for handle in handles {
        let (i, latency, success) = handle.await.unwrap();
        if success {
            latencies.push(latency);
        } else {
            failures += 1;
            println!("Request {} failed", i);
        }
    }

    let total_time = start.elapsed();
    latencies.sort();

    let avg_ms =
        latencies.iter().map(|d| d.as_millis()).sum::<u128>() as f64 / latencies.len() as f64;
    let p50 = latencies[latencies.len() / 2];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    println!("50 concurrent requests completed in {:?}", total_time);
    println!("Failures: {}", failures);
    println!("Avg latency: {:.1}ms", avg_ms);
    println!("P50 latency: {:?}", p50);
    println!("P99 latency: {:?}", p99);

    assert_eq!(failures, 0, "No requests should fail");
    assert!(
        p99.as_millis() < 5000,
        "P99 latency should be under 5s: {:?}",
        p99
    );

    shutdown_tx.send(()).ok();
}

// === Task 14.3: Rapid file changes ===

#[tokio::test]
#[ignore]
async fn test_rapid_file_changes() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();

    let mut watcher = FileWatcher::new(500, &["*.swp".into()]).unwrap();
    watcher.watch_directory(&watch_path).unwrap();

    // Small delay to ensure watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make 50 rapid file changes
    println!("Making 50 rapid file changes...");
    let start = Instant::now();
    for i in 0..50 {
        let file_path = watch_path.join(format!("file{}.md", i));
        fs::write(&file_path, format!("# File {}\nContent", i)).unwrap();
    }
    println!("Created 50 files in {:?}", start.elapsed());

    // Wait for debounce window + buffer
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Should receive batched events (not 50 individual events)
    let mut event_count = 0;
    let mut total_paths = 0;
    while let Some(paths) = watcher.try_recv() {
        event_count += 1;
        total_paths += paths.len();
    }

    println!(
        "Received {} event batches with {} total paths",
        event_count, total_paths
    );

    // Debouncing should batch events
    assert!(
        event_count <= 5,
        "Should batch events, got {} batches",
        event_count
    );
    // Should eventually see all files
    assert!(
        total_paths >= 40,
        "Should see most files, got {}",
        total_paths
    );
}

// === Task 14.4: Memory stability (simplified) ===

#[tokio::test]
#[ignore]
async fn test_memory_stability_basic() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", temp_dir.path().to_path_buf());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Perform repeated operations
    println!("Performing 100 embedding generations...");
    for i in 0..100 {
        let text = format!("Test content number {} for memory stability testing", i);
        let _ = embedding.generate(&text).await.unwrap();
        if i % 25 == 0 {
            println!("Completed {}/100 embeddings", i);
        }
    }

    // Perform repeated DB operations
    println!("Performing 100 document upserts...");
    for i in 0..100 {
        let doc = Document {
            id: format!("doc{:03}", i),
            repo_id: "test".into(),
            file_path: format!("doc{}.md", i),
            file_hash: "hash".into(),
            title: format!("Document {}", i),
            doc_type: Some("test".into()),
            content: format!("Content for document {}", i),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
    }

    // Verify all documents accessible
    let docs = db.get_documents_for_repo("test").unwrap();
    assert_eq!(docs.len(), 100);

    println!("Memory stability test completed successfully");
}
