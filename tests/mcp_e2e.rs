//! MCP server E2E tests with real semantic search.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use chrono::Utc;
use common::ollama_helpers::require_ollama;
use common::TestScanSetup;
use factbase::{
    config::Config, database::Database, embedding::OllamaEmbedding, mcp::McpServer,
    models::Repository, scanner::full_scan,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::oneshot;

/// Helper to set up a test repo with real embeddings
async fn setup_indexed_repo() -> (TempDir, Database, OllamaEmbedding, Repository, Config) {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(repo_path.join("people")).unwrap();
    fs::create_dir_all(repo_path.join("projects")).unwrap();

    // Create test documents
    fs::write(
        repo_path.join("people/alice.md"),
        "# Alice Chen\nAlice is a senior backend engineer specializing in Rust and distributed systems.",
    )
    .unwrap();
    fs::write(
        repo_path.join("people/bob.md"),
        "# Bob Martinez\nBob is a frontend developer with expertise in React and TypeScript.",
    )
    .unwrap();
    fs::write(
        repo_path.join("projects/api.md"),
        "# API Gateway\nA high-performance API gateway built with Rust. Team: Alice Chen leads the backend.",
    )
    .unwrap();

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "test".into(),
        name: "Test Repo".into(),
        path: repo_path.clone(),
        perspective: Some(factbase::models::Perspective {
            type_name: "knowledge-base".into(),
            organization: Some("Test Org".into()),
            focus: Some("testing".into()),
            allowed_types: None,
            review: None,
            format: None,
            link_match_mode: None,
        }),
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).unwrap();

    let setup = TestScanSetup::new();
    full_scan(&repo, &db, &setup.context()).await.unwrap();

    (temp_dir, db, setup.embedding, repo, setup.config)
}

/// Test 6.1: MCP search with real embeddings
#[tokio::test]
#[ignore]
async fn test_mcp_search_with_real_embeddings() {
    require_ollama().await;

    let (_temp, db, embedding, _repo, config) = setup_indexed_repo().await;

    // Start MCP server
    let port = common::random_port();
    let server = McpServer::new(
        db,
        embedding,
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
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
        .unwrap();

    // Test semantic search for "backend engineer"
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "backend engineer Rust"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "Should find results for backend engineer"
    );

    // Alice should be top result (she's a backend engineer)
    let top_result = &results[0];
    assert!(
        top_result["title"].as_str().unwrap().contains("Alice"),
        "Alice should be top result for backend engineer query"
    );

    // Verify relevance score is reasonable
    let score = top_result["relevance_score"].as_f64().unwrap();
    assert!(
        score > 0.0 && score <= 1.0,
        "Relevance score should be between 0 and 1"
    );

    // Test search for "frontend React"
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "frontend React developer"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|r| r["title"].as_str().unwrap().contains("Bob")),
        "Bob should be found for frontend React query"
    );

    // Test search with type filter
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "engineer", "type": "person"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    for r in results {
        assert_eq!(r["type"], "person", "All results should be person type");
    }

    shutdown_tx.send(()).ok();
}

/// Test 6.2: All 8 MCP tools end-to-end
#[tokio::test]
#[ignore]
async fn test_all_8_mcp_tools() {
    require_ollama().await;

    let (_temp, db, embedding, repo, config) = setup_indexed_repo().await;

    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
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
        .unwrap();

    // 1. search_knowledge
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "engineer"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        resp["result"]["results"].is_array(),
        "search_knowledge should return results array"
    );

    // 2. get_entity
    let docs = db.get_documents_for_repo("test").unwrap();
    let first_id = docs.keys().next().unwrap();
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {"name": "get_entity", "arguments": {"id": first_id}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        resp["result"]["id"].as_str().unwrap(),
        first_id,
        "get_entity should return correct document"
    );

    // 3. list_entities
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": {"name": "list_entities", "arguments": {}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let entities = resp["result"]["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 3, "list_entities should return 3 documents");

    // 4. get_perspective
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 4,
            "method": "tools/call",
            "params": {"name": "get_perspective", "arguments": {}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        resp["result"]["id"], "test",
        "get_perspective should return repo info"
    );

    // 5. list_repositories (removed — now returns helpful error)
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 5,
            "method": "tools/call",
            "params": {"name": "list_repositories", "arguments": {}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        resp["error"].is_object(),
        "list_repositories should return error (removed tool)"
    );

    // 6. create_document
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 6,
            "method": "tools/call",
            "params": {"name": "create_document", "arguments": {
                "repo": "test",
                "path": "people/carol.md",
                "title": "Carol Davis",
                "content": "Carol is a DevOps engineer."
            }}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        resp["result"]["id"].is_string(),
        "create_document should return new document ID"
    );
    let new_id = resp["result"]["id"].as_str().unwrap();

    // Verify file created
    let file_path = repo.path.join("people/carol.md");
    assert!(file_path.exists(), "New document file should exist");
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("Carol Davis"), "File should contain title");

    // 7. update_document
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 7,
            "method": "tools/call",
            "params": {"name": "update_document", "arguments": {
                "id": new_id,
                "content": "Carol is a senior DevOps engineer specializing in Kubernetes."
            }}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        resp["result"]["id"], new_id,
        "update_document should return updated document ID"
    );

    // Verify file updated
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("Kubernetes"),
        "File should contain updated content"
    );

    // 8. delete_document
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 8,
            "method": "tools/call",
            "params": {"name": "delete_document", "arguments": {"id": new_id}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        resp["result"]["id"], new_id,
        "delete_document should return deleted document ID"
    );

    // Verify file deleted
    assert!(
        !file_path.exists(),
        "Deleted document file should not exist"
    );

    shutdown_tx.send(()).ok();
}

/// Test 6.3: Concurrent MCP requests with real operations
#[tokio::test]
#[ignore]
async fn test_mcp_concurrent_requests_real() {
    require_ollama().await;

    let (_temp, db, embedding, _repo, config) = setup_indexed_repo().await;

    let port = common::random_port();
    let server = McpServer::new(
        db,
        embedding,
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
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap(),
    );

    // Send 20 concurrent search requests
    let start = Instant::now();
    let mut handles = vec![];

    for i in 0..20 {
        let client = client.clone();
        let url = format!("{}/mcp", base_url);
        let query = match i % 4 {
            0 => "backend engineer",
            1 => "frontend developer",
            2 => "API gateway",
            _ => "distributed systems",
        };

        handles.push(tokio::spawn(async move {
            let req_start = Instant::now();
            let resp = client
                .post(&url)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "method": "tools/call",
                    "params": {"name": "search_knowledge", "arguments": {"query": query}}
                }))
                .send()
                .await;
            let duration = req_start.elapsed();
            (i, resp.is_ok(), duration)
        }));
    }

    let mut success_count = 0;
    let mut total_duration = Duration::ZERO;

    for handle in handles {
        let (i, ok, duration) = handle.await.unwrap();
        if ok {
            success_count += 1;
            total_duration += duration;
        } else {
            eprintln!("Request {} failed", i);
        }
    }

    let total_time = start.elapsed();
    let avg_duration = total_duration / success_count as u32;

    println!("Concurrent test results:");
    println!("  Total requests: 20");
    println!("  Successful: {}", success_count);
    println!("  Total time: {:?}", total_time);
    println!("  Avg response time: {:?}", avg_duration);

    assert_eq!(
        success_count, 20,
        "All 20 concurrent requests should succeed"
    );
    assert!(
        total_time < Duration::from_secs(60),
        "All requests should complete within 60 seconds"
    );

    shutdown_tx.send(()).ok();
}

/// Test 6.4: MCP write operations trigger proper updates
#[tokio::test]
#[ignore]
async fn test_mcp_write_operations_update_index() {
    require_ollama().await;

    let (_temp, db, embedding, repo, config) = setup_indexed_repo().await;

    let port = common::random_port();
    let server = McpServer::new(
        db.clone(),
        embedding.clone(),
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
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
        .unwrap();

    // Create a new document via MCP
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {"name": "create_document", "arguments": {
                "repo": "test",
                "path": "people/dave.md",
                "title": "Dave Wilson",
                "content": "Dave is a machine learning engineer working on NLP models."
            }}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let new_id = resp["result"]["id"].as_str().unwrap().to_string();

    // Run scan to generate embedding for new document
    let scan_setup = TestScanSetup::new();
    full_scan(&repo, &db, &scan_setup.context()).await.unwrap();

    // Search for the new document
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "machine learning NLP"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|r| r["title"].as_str().unwrap().contains("Dave")),
        "Dave should be searchable after scan"
    );

    // Update the document via MCP
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": {"name": "update_document", "arguments": {
                "id": new_id,
                "content": "Dave is a computer vision engineer working on image recognition."
            }}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(resp["error"].is_null(), "Update should succeed");

    // Rescan to update embedding
    full_scan(&repo, &db, &scan_setup.context()).await.unwrap();

    // Search for updated content
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 4,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "computer vision image recognition"}}
        }))
        .send().await.unwrap().json::<Value>().await.unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|r| r["title"].as_str().unwrap().contains("Dave")),
        "Dave should be found with updated content"
    );

    // Delete the document via MCP
    client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 5,
            "method": "tools/call",
            "params": {"name": "delete_document", "arguments": {"id": new_id}}
        }))
        .send()
        .await
        .unwrap();

    // Rescan to process deletion
    full_scan(&repo, &db, &scan_setup.context()).await.unwrap();

    // Verify deleted document not in search results
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0", "id": 6,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "computer vision"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let results = resp["result"]["results"].as_array().unwrap();
    assert!(
        !results
            .iter()
            .any(|r| r["title"].as_str().unwrap().contains("Dave")),
        "Deleted document should not appear in search"
    );

    shutdown_tx.send(()).ok();
}

/// Test tools/list endpoint returns all 8 tools
#[tokio::test]
#[ignore]
async fn test_tools_list_endpoint() {
    let (_temp, db, embedding, _repo, config) = setup_indexed_repo().await;

    let port = common::random_port();
    let server = McpServer::new(
        db,
        embedding,
        "127.0.0.1",
        port,
        config.rate_limit.clone(),
        &config.embedding.base_url,
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
        .unwrap();

    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools.len(),
        3,
        "Should have 3 MCP tools: search, workflow, factbase"
    );

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    assert!(tool_names.contains(&"search"));
    assert!(tool_names.contains(&"workflow"));
    assert!(tool_names.contains(&"factbase"));

    shutdown_tx.send(()).ok();
}
