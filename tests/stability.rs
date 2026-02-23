//! Long-running stability tests.
//! These tests verify the system remains stable over extended operation.
//! Note: Full 10-minute tests are marked #[ignore] - run with --ignored flag.

mod common;

use chrono::Utc;
use common::ollama_helpers::require_ollama;
use common::{run_scan, TestContext};
use factbase::{
    config::Config, database::Database, embedding::OllamaEmbedding, mcp::McpServer,
    models::Repository,
};
use reqwest::Client;
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::oneshot;

/// Test 9.1: Short stability test (2 minutes)
/// Tests system stability with periodic operations
#[tokio::test]
async fn test_stability_short() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("docs")).expect("operation should succeed");

    // Create initial documents
    for i in 0..10 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nInitial content for document {}.", i, i),
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
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Initial scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
    );
    let base_url = Arc::new(format!("http://127.0.0.1:{}", port));

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        server.start(shutdown_rx).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("operation should succeed"),
    );

    let errors = Arc::new(AtomicUsize::new(0));
    let mcp_requests = Arc::new(AtomicUsize::new(0));
    let file_changes = Arc::new(AtomicUsize::new(0));
    let scans = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    let duration = Duration::from_secs(120); // 2 minutes

    // Run periodic operations
    let mut iteration = 0;
    while start.elapsed() < duration {
        iteration += 1;

        // MCP request every iteration
        {
            let resp = client
                .post(format!("{}/mcp", base_url))
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": iteration,
                    "method": "tools/call",
                    "params": {"name": "list_entities", "arguments": {}}
                }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    mcp_requests.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    errors.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        // File change every 15 seconds
        if iteration % 3 == 0 {
            let doc_num = iteration % 10;
            fs::write(
                repo_path.join(format!("docs/doc{}.md", doc_num)),
                format!(
                    "# Document {}\nUpdated at iteration {}.",
                    doc_num, iteration
                ),
            )
            .expect("operation should succeed");
            file_changes.fetch_add(1, Ordering::SeqCst);
        }

        // Scan every 30 seconds
        if iteration % 6 == 0 {
            if run_scan(&repo, &db, &config).await.is_ok() {
                scans.fetch_add(1, Ordering::SeqCst);
            } else {
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let total_errors = errors.load(Ordering::SeqCst);
    let total_mcp = mcp_requests.load(Ordering::SeqCst);
    let total_files = file_changes.load(Ordering::SeqCst);
    let total_scans = scans.load(Ordering::SeqCst);

    println!("Stability test results (2 minutes):");
    println!("  MCP requests: {}", total_mcp);
    println!("  File changes: {}", total_files);
    println!("  Scans: {}", total_scans);
    println!("  Errors: {}", total_errors);

    // Verify database integrity
    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    assert_eq!(docs.len(), 10, "All documents should still exist");

    assert!(total_errors < 3, "Should have minimal errors");
    assert!(total_mcp >= 20, "Should complete many MCP requests");
    assert!(total_scans >= 3, "Should complete multiple scans");

    shutdown_tx.send(()).ok();
}

/// Test 9.2: Long stability test (10 minutes)
/// Run with: cargo test test_stability_long --ignored
#[tokio::test]
#[ignore]
async fn test_stability_long() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("docs")).expect("operation should succeed");

    // Create initial documents
    for i in 0..20 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nInitial content for document {}.", i, i),
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
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Initial scan
    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
    );
    let base_url = Arc::new(format!("http://127.0.0.1:{}", port));

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        server.start(shutdown_rx).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("operation should succeed"),
    );

    let errors = Arc::new(AtomicUsize::new(0));
    let mcp_requests = Arc::new(AtomicUsize::new(0));
    let file_changes = Arc::new(AtomicUsize::new(0));
    let scans = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    let duration = Duration::from_secs(600); // 10 minutes

    println!("Starting 10-minute stability test...");

    let mut iteration = 0;
    while start.elapsed() < duration {
        iteration += 1;

        // MCP request every 10 seconds
        let resp = client
            .post(format!("{}/mcp", base_url))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": iteration,
                "method": "tools/call",
                "params": {"name": "list_entities", "arguments": {}}
            }))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                mcp_requests.fetch_add(1, Ordering::SeqCst);
            }
            _ => {
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        // File change every 30 seconds
        if iteration % 3 == 0 {
            let doc_num = iteration % 20;
            fs::write(
                repo_path.join(format!("docs/doc{}.md", doc_num)),
                format!(
                    "# Document {}\nUpdated at iteration {}.",
                    doc_num, iteration
                ),
            )
            .expect("operation should succeed");
            file_changes.fetch_add(1, Ordering::SeqCst);
        }

        // Scan every 60 seconds
        if iteration % 6 == 0 {
            if run_scan(&repo, &db, &config).await.is_ok() {
                scans.fetch_add(1, Ordering::SeqCst);
            } else {
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        // Progress update every minute
        if iteration % 6 == 0 {
            let elapsed = start.elapsed().as_secs();
            println!(
                "  Progress: {}m {}s - MCP: {}, Files: {}, Scans: {}, Errors: {}",
                elapsed / 60,
                elapsed % 60,
                mcp_requests.load(Ordering::SeqCst),
                file_changes.load(Ordering::SeqCst),
                scans.load(Ordering::SeqCst),
                errors.load(Ordering::SeqCst)
            );
        }

        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    let total_errors = errors.load(Ordering::SeqCst);
    let total_mcp = mcp_requests.load(Ordering::SeqCst);
    let total_files = file_changes.load(Ordering::SeqCst);
    let total_scans = scans.load(Ordering::SeqCst);

    println!("\nStability test results (10 minutes):");
    println!("  MCP requests: {}", total_mcp);
    println!("  File changes: {}", total_files);
    println!("  Scans: {}", total_scans);
    println!("  Errors: {}", total_errors);

    // Verify database integrity
    let docs = db
        .get_documents_for_repo("test")
        .expect("operation should succeed");
    assert_eq!(docs.len(), 20, "All documents should still exist");

    assert!(
        total_errors < 5,
        "Should have minimal errors over 10 minutes"
    );
    assert!(total_mcp >= 50, "Should complete many MCP requests");
    assert!(total_scans >= 8, "Should complete multiple scans");

    shutdown_tx.send(()).ok();
}

/// Test 9.3: Database integrity after operations
#[tokio::test]
async fn test_database_integrity_after_operations() {
    require_ollama().await;

    let ctx = TestContext::new("test");
    let repo_path = ctx.repo_path.clone();
    fs::create_dir_all(repo_path.join("docs")).expect("operation should succeed");

    // Create documents
    for i in 0..5 {
        fs::write(
            repo_path.join(format!("docs/doc{}.md", i)),
            format!("# Document {}\nContent for document {}.", i, i),
        )
        .expect("operation should succeed");
    }

    // Initial scan
    ctx.scan().await.expect("operation should succeed");

    // Perform multiple operations
    for i in 0..10 {
        // Modify a document
        let doc_num = i % 5;
        fs::write(
            repo_path.join(format!("docs/doc{}.md", doc_num)),
            format!("# Document {}\nIteration {} content.", doc_num, i),
        )
        .expect("operation should succeed");

        // Rescan
        ctx.scan().await.expect("operation should succeed");
    }

    // Add new document
    fs::write(
        repo_path.join("docs/new.md"),
        "# New Document\nNew content.",
    )
    .expect("operation should succeed");
    ctx.scan().await.expect("operation should succeed");

    // Delete a document
    fs::remove_file(repo_path.join("docs/doc0.md")).expect("operation should succeed");
    ctx.scan().await.expect("operation should succeed");

    // Verify database integrity
    let docs = ctx
        .db
        .get_documents_for_repo("test")
        .expect("operation should succeed");

    // Should have 5 docs (4 original + 1 new, 1 deleted)
    let active_docs: Vec<_> = docs.values().filter(|d| !d.is_deleted).collect();
    assert_eq!(active_docs.len(), 5, "Should have 5 active documents");

    // Verify each active document has an embedding
    for doc in active_docs {
        let results = ctx
            .db
            .search_semantic_with_query(
                &vec![0.0; ctx.config.embedding.dimension],
                100,
                None,
                None,
                None,
            )
            .expect("operation should succeed");
        assert!(
            results.iter().any(|r| r.id == doc.id),
            "Document {} should have embedding",
            doc.id
        );
    }

    // Verify no orphaned embeddings
    let all_doc_ids: std::collections::HashSet<_> = docs.keys().collect();
    let search_results = ctx
        .db
        .search_semantic_with_query(
            &vec![0.0; ctx.config.embedding.dimension],
            100,
            None,
            None,
            None,
        )
        .expect("operation should succeed");

    for result in &search_results {
        assert!(
            all_doc_ids.contains(&result.id),
            "Embedding {} should belong to existing document",
            result.id
        );
    }
}
