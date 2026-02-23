//! Concurrent operations stress tests.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use common::ollama_helpers::require_ollama;
use common::run_scan;
use factbase::{config::Config, database::Database, embedding::OllamaEmbedding, mcp::McpServer};
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::oneshot;

/// Test 7.1: Concurrent file changes and MCP requests
#[tokio::test]
async fn test_concurrent_file_changes_and_mcp() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("docs")).unwrap();

    // Create initial documents
    for i in 0..5 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nInitial content for document {}.", i, i),
        )
        .unwrap();
    }

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.clone());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Initial scan
    run_scan(&repo, &db, &config).await.unwrap();

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
        None,
    );
    let base_url = Arc::new(format!("http://127.0.0.1:{}", port));

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        server.start(shutdown_rx).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap(),
    );

    let errors = Arc::new(AtomicUsize::new(0));
    let mcp_success = Arc::new(AtomicUsize::new(0));
    let file_ops = Arc::new(AtomicUsize::new(0));

    // Spawn concurrent tasks
    let mut handles = vec![];

    // Task 1: File modifications
    let repo_path_clone = repo_path.clone();
    let file_ops_clone = file_ops.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..10 {
            let path = repo_path_clone.join(format!("docs/doc{}.md", i % 5));
            let content = format!("# Document {}\nUpdated content iteration {}.", i % 5, i);
            if fs::write(&path, content).is_ok() {
                file_ops_clone.fetch_add(1, Ordering::SeqCst);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }));

    // Task 2: MCP search requests
    let client_clone = client.clone();
    let base_url_clone = base_url.clone();
    let mcp_success_clone = mcp_success.clone();
    let errors_clone = errors.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..15 {
            let resp = client_clone
                .post(format!("{}/mcp", base_url_clone))
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "method": "tools/call",
                    "params": {"name": "list_entities", "arguments": {}}
                }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    mcp_success_clone.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    errors_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }));

    // Task 3: MCP write operations
    let client_clone = client.clone();
    let base_url_clone = base_url.clone();
    let mcp_success_clone = mcp_success.clone();
    let errors_clone = errors.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..5 {
            let resp = client_clone
                .post(format!("{}/mcp", base_url_clone))
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 100 + i,
                    "method": "tools/call",
                    "params": {"name": "create_document", "arguments": {
                        "repo": "test",
                        "path": format!("docs/new{}.md", i),
                        "title": format!("New Doc {}", i),
                        "content": format!("Content for new document {}.", i)
                    }}
                }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    mcp_success_clone.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    errors_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }));

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Final scan to process all changes
    let result = run_scan(&repo, &db, &config).await.unwrap();

    let total_errors = errors.load(Ordering::SeqCst);
    let total_mcp = mcp_success.load(Ordering::SeqCst);
    let total_files = file_ops.load(Ordering::SeqCst);

    println!("Concurrent test results:");
    println!("  File operations: {}", total_files);
    println!("  MCP successes: {}", total_mcp);
    println!("  Errors: {}", total_errors);
    println!(
        "  Final scan: {} added, {} updated",
        result.added, result.updated
    );

    assert!(
        total_errors < 3,
        "Should have minimal errors (got {})",
        total_errors
    );
    assert!(total_mcp >= 15, "Most MCP requests should succeed");
    assert!(total_files >= 8, "Most file operations should succeed");

    shutdown_tx.send(()).ok();
}

/// Test 7.2: Scan during active MCP requests
#[tokio::test]
async fn test_scan_during_active_mcp_requests() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("docs")).unwrap();

    // Create documents
    for i in 0..10 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nContent for document {}.", i, i),
        )
        .unwrap();
    }

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.clone());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Initial scan
    run_scan(&repo, &db, &config).await.unwrap();

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
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
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();

    // Start MCP requests in background
    let client_clone = client.clone();
    let base_url_clone = base_url.clone();
    let mcp_handle = tokio::spawn(async move {
        let mut success = 0;
        for i in 0..20 {
            let resp = client_clone
                .post(format!("{}/mcp", base_url_clone))
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "method": "tools/call",
                    "params": {"name": "list_entities", "arguments": {}}
                }))
                .send()
                .await;

            if resp.is_ok() {
                success += 1;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        success
    });

    // Modify files and trigger rescan while MCP requests are running
    tokio::time::sleep(Duration::from_millis(500)).await;

    for i in 0..5 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nModified content for document {}.", i, i),
        )
        .unwrap();
    }

    // Run scan while MCP requests are active
    let scan_result = run_scan(&repo, &db, &config).await.unwrap();

    // Wait for MCP requests to complete
    let mcp_success = mcp_handle.await.unwrap();

    println!("Scan during MCP results:");
    println!("  MCP successes: {}/20", mcp_success);
    println!("  Scan updated: {}", scan_result.updated);

    assert!(
        mcp_success >= 18,
        "Most MCP requests should succeed during scan"
    );
    assert_eq!(scan_result.updated, 5, "Scan should update 5 documents");

    shutdown_tx.send(()).ok();
}

/// Test 7.3: MCP writes during scan
#[tokio::test]
async fn test_mcp_writes_during_scan() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("docs")).unwrap();

    // Create initial documents
    for i in 0..5 {
        fs::write(
            repo_path.join(format!("docs/initial{}.md", i)),
            format!("# Initial {}\nInitial content {}.", i, i),
        )
        .unwrap();
    }

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("test", repo_path.clone());
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
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
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();

    // Start scan in background
    let repo_clone = repo.clone();
    let db_clone = db.clone();
    let config_clone = config.clone();
    let scan_handle =
        tokio::spawn(async move { run_scan(&repo_clone, &db_clone, &config_clone).await });

    // Make MCP create_document calls while scan is running
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut created_ids = vec![];
    for i in 0..3 {
        let resp = client
            .post(format!("{}/mcp", base_url))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": i,
                "method": "tools/call",
                "params": {"name": "create_document", "arguments": {
                    "repo": "test",
                    "path": format!("docs/during_scan{}.md", i),
                    "title": format!("During Scan {}", i),
                    "content": format!("Created during scan {}.", i)
                }}
            }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();

        if let Some(id) = resp["result"]["id"].as_str() {
            created_ids.push(id.to_string());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Wait for scan to complete
    let scan_result = scan_handle.await.unwrap().unwrap();

    // Run another scan to pick up MCP-created documents
    let final_result = run_scan(&repo, &db, &config).await.unwrap();

    println!("MCP writes during scan results:");
    println!("  First scan: {} added", scan_result.added);
    println!("  MCP created: {} documents", created_ids.len());
    println!(
        "  Final scan: {} added, {} unchanged",
        final_result.added, final_result.unchanged
    );

    // Verify all documents exist
    let docs = db.get_documents_for_repo("test").unwrap();
    assert!(
        docs.len() >= 8,
        "Should have at least 8 documents (5 initial + 3 MCP created)"
    );

    // Verify MCP-created files exist
    for i in 0..3 {
        let path = repo_path.join(format!("docs/during_scan{}.md", i));
        assert!(path.exists(), "MCP-created file {} should exist", i);
    }

    shutdown_tx.send(()).ok();
}
