//! File watcher E2E tests with real Ollama rescans.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use common::ollama_helpers::require_ollama;
use common::TestContext;
use factbase::{mcp::McpServer, watcher::FileWatcher, EmbeddingProvider};
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::time::Duration;
use tokio::sync::oneshot;

/// Test 5.1: File watcher triggers real scan with Ollama
#[tokio::test]
async fn test_watcher_triggers_real_scan() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "test",
        &[("people/alice.md", "# Alice\nAlice is a software engineer.")],
    );

    let embedding = ctx.embedding();

    // Initial scan
    let result = ctx.scan().await.expect("initial scan");
    assert_eq!(result.added, 1, "Should add 1 document");

    // Verify embedding exists via search
    let query_emb = embedding
        .generate("software engineer")
        .await
        .expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert!(!results.is_empty(), "Should find Alice via semantic search");

    // Start file watcher
    let mut watcher =
        FileWatcher::new(200, &ctx.config.watcher.ignore_patterns).expect("create watcher");
    watcher.watch_directory(&ctx.repo_path).expect("watch dir");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Modify the document
    fs::write(
        ctx.repo_path.join("people/alice.md"),
        "# Alice\nAlice is a backend developer specializing in Rust.",
    )
    .expect("write file");

    // Wait for debounce
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check for watcher event
    let mut event_received = false;
    for _ in 0..10 {
        if watcher.try_recv().is_some() {
            event_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(event_received, "Watcher should detect file modification");

    // Run rescan
    let result = ctx.scan().await.expect("rescan");
    assert_eq!(result.updated, 1, "Should update 1 document");

    // Verify embedding updated
    let query_emb = embedding
        .generate("Rust backend developer")
        .await
        .expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert!(!results.is_empty(), "Should find updated content");
    assert!(
        results[0].title.contains("Alice"),
        "Alice should be top result for Rust query"
    );
}

/// Test 5.2: New file detection and indexing with real embeddings
#[tokio::test]
async fn test_new_file_detection_and_indexing() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "test",
        &[(
            "projects/alpha.md",
            "# Alpha Project\nA web application project.",
        )],
    );

    let embedding = ctx.embedding();

    // Initial scan
    ctx.scan().await.expect("initial scan");

    // Start watcher
    let mut watcher =
        FileWatcher::new(200, &ctx.config.watcher.ignore_patterns).expect("create watcher");
    watcher.watch_directory(&ctx.repo_path).expect("watch dir");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Create new file
    fs::write(
        ctx.repo_path.join("projects/beta.md"),
        "# Beta Project\nA machine learning pipeline for data analysis.",
    )
    .expect("write file");

    // Wait for watcher event
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut event_received = false;
    for _ in 0..10 {
        if watcher.try_recv().is_some() {
            event_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(event_received, "Watcher should detect new file");

    // Rescan
    let result = ctx.scan().await.expect("rescan");
    assert_eq!(result.added, 1, "Should add new document");

    // Verify new file has factbase ID injected
    let content = fs::read_to_string(ctx.repo_path.join("projects/beta.md")).expect("read file");
    assert!(
        content.contains("<!-- factbase:"),
        "New file should have factbase ID"
    );

    // Verify embedding generated and searchable
    let query_emb = embedding
        .generate("machine learning data")
        .await
        .expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert!(
        results.iter().any(|r| r.title.contains("Beta")),
        "Beta project should be searchable"
    );
}

/// Test 5.3: File deletion handling with real database updates
#[tokio::test]
async fn test_file_deletion_handling() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "test",
        &[
            ("notes/keep.md", "# Keep This\nThis document stays."),
            (
                "notes/delete.md",
                "# Delete This\nThis document will be removed.",
            ),
        ],
    );

    let embedding = ctx.embedding();

    // Initial scan
    let result = ctx.scan().await.expect("initial scan");
    assert_eq!(result.added, 2, "Should add 2 documents");

    // Verify both searchable
    let query_emb = embedding.generate("document").await.expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert_eq!(results.len(), 2, "Both documents should be searchable");

    // Start watcher
    let mut watcher =
        FileWatcher::new(200, &ctx.config.watcher.ignore_patterns).expect("create watcher");
    watcher.watch_directory(&ctx.repo_path).expect("watch dir");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Delete one file
    fs::remove_file(ctx.repo_path.join("notes/delete.md")).expect("delete file");

    // Wait for watcher
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Rescan
    let result = ctx.scan().await.expect("rescan");
    assert_eq!(result.deleted, 1, "Should mark 1 document deleted");

    // Verify deleted doc not in search results
    let query_emb = embedding.generate("removed").await.expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert!(
        !results.iter().any(|r| r.title.contains("Delete")),
        "Deleted document should not appear in search"
    );

    // Verify kept doc still searchable
    let query_emb = embedding.generate("stays").await.expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert!(
        results.iter().any(|r| r.title.contains("Keep")),
        "Kept document should still be searchable"
    );
}

/// Test 5.4: Rapid changes with debouncing and real operations
#[tokio::test]
async fn test_rapid_changes_with_debouncing() {
    require_ollama().await;

    let ctx = TestContext::new("test");
    let embedding = ctx.embedding();

    // Start watcher with 500ms debounce
    let mut watcher =
        FileWatcher::new(500, &ctx.config.watcher.ignore_patterns).expect("create watcher");
    watcher.watch_directory(&ctx.repo_path).expect("watch dir");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Make 5 rapid file creations
    for i in 0..5 {
        fs::write(
            ctx.repo_path.join(format!("doc{}.md", i)),
            format!("# Document {}\nContent for document {}.", i, i),
        )
        .expect("write file");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Wait for debounce window to pass
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Count event batches
    let mut batches = 0;
    for _ in 0..10 {
        if watcher.try_recv().is_some() {
            batches += 1;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Debouncing should batch events
    assert!(
        batches > 0 && batches < 5,
        "Expected batching (1-4 batches for 5 files), got {}",
        batches
    );

    // Run scan to process all changes
    let result = ctx.scan().await.expect("scan");
    assert_eq!(result.added, 5, "All 5 documents should be added");

    // Verify all documents searchable
    let query_emb = embedding.generate("document content").await.expect("embed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search");
    assert_eq!(results.len(), 5, "All 5 documents should be searchable");
}

/// Test watcher with MCP server integration
#[tokio::test]
async fn test_watcher_with_mcp_server() {
    require_ollama().await;

    let ctx = TestContext::with_files(
        "test",
        &[("people/bob.md", "# Bob\nBob is a project manager.")],
    );

    let embedding = ctx.embedding();

    // Initial scan
    ctx.scan().await.expect("initial scan");

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        ctx.db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        ctx.config.rate_limit.clone(),
        &ctx.config.embedding.base_url,
        None,
    );
    let base_url = format!("http://127.0.0.1:{}", port);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        server.start(shutdown_rx).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build client");

    // Verify initial document via MCP
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "project manager"}}
        }))
        .send()
        .await
        .expect("send request")
        .json::<Value>()
        .await
        .expect("parse json");
    let results = resp["result"]["results"].as_array().expect("results array");
    assert!(
        results
            .iter()
            .any(|r| r["title"].as_str().unwrap_or("").contains("Bob")),
        "Bob should be found via MCP search"
    );

    // Start watcher
    let mut watcher =
        FileWatcher::new(200, &ctx.config.watcher.ignore_patterns).expect("create watcher");
    watcher.watch_directory(&ctx.repo_path).expect("watch dir");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Add new document
    fs::write(
        ctx.repo_path.join("people/carol.md"),
        "# Carol\nCarol is a data scientist working on ML models.",
    )
    .expect("write file");

    // Wait for watcher event
    tokio::time::sleep(Duration::from_millis(500)).await;
    for _ in 0..10 {
        if watcher.try_recv().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Rescan
    ctx.scan().await.expect("rescan");

    // Verify new document via MCP
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "data scientist ML"}}
        }))
        .send()
        .await
        .expect("send request")
        .json::<Value>()
        .await
        .expect("parse json");
    let results = resp["result"]["results"].as_array().expect("results array");
    assert!(
        results
            .iter()
            .any(|r| r["title"].as_str().unwrap_or("").contains("Carol")),
        "Carol should be found via MCP search after rescan"
    );

    // Cleanup
    shutdown_tx.send(()).ok();
}
