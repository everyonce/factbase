//! End-to-end integration tests for serve command.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use chrono::Utc;
use common::ollama_helpers::require_ollama;
use common::TestServer;
use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    mcp::McpServer,
    models::{Document, Perspective, Repository},
    watcher::FileWatcher,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::oneshot;

// --- Tests ---

#[tokio::test]
#[ignore]
async fn test_serve_starts_both_components() {
    let server = TestServer::start_with_data().await;

    // Verify MCP server is accepting connections
    let resp = server.health().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify we can query entities
    let resp = server.call_tool("list_entities", json!({})).await.unwrap();
    assert!(resp["result"]["entities"].is_array());
}

#[tokio::test]
#[ignore]
async fn test_initial_document_accessible() {
    let server = TestServer::start_with_data().await;

    // Get a document
    let resp = server
        .call_tool("get_entity", json!({"id": "doc1"}))
        .await
        .unwrap();
    assert_eq!(resp["result"]["id"], "doc1");
    assert_eq!(resp["result"]["title"], "Alice Smith");
}

#[tokio::test]
#[ignore]
async fn test_get_perspective_returns_repo_info() {
    let server = TestServer::start_with_data().await;

    let resp = server
        .call_tool("get_perspective", json!({}))
        .await
        .unwrap();
    assert_eq!(resp["result"]["id"], "test-repo");
    assert_eq!(resp["result"]["name"], "Test Repo");
    assert!(resp["result"]["perspective"].is_object());
}

#[tokio::test]
#[ignore]
async fn test_mcp_client_workflow() {
    // Simulates an AI agent using MCP
    let server = TestServer::start_with_data().await;

    // Step 1: Get perspective to understand the knowledge base
    let perspective = server
        .call_tool("get_perspective", json!({}))
        .await
        .unwrap();
    assert!(perspective["result"]["id"].is_string());

    // Step 2: List available entities
    let list = server.call_tool("list_entities", json!({})).await.unwrap();
    let entities = list["result"]["entities"].as_array().unwrap();
    assert!(!entities.is_empty());

    // Step 3: Get details of first entity
    let first_id = entities[0]["id"].as_str().unwrap();
    let entity = server
        .call_tool("get_entity", json!({"id": first_id}))
        .await
        .unwrap();
    assert!(entity["result"]["content"].is_string());
}

#[tokio::test]
#[ignore]
async fn test_graceful_shutdown() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

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
    let handle = tokio::spawn(async move { server.start(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify server is running
    let client = Client::new();
    let resp = client.get(format!("{}/health", base_url)).send().await;
    assert!(resp.is_ok());

    // Send shutdown signal
    shutdown_tx.send(()).ok();

    // Wait for server to stop
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "Server should shut down within 5 seconds");
}

#[tokio::test]
#[ignore]
async fn test_watcher_detects_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();

    let mut watcher = FileWatcher::new(100, &["*.swp".into()]).unwrap();
    watcher.watch_directory(&watch_path).unwrap();

    // Small delay to ensure watcher is ready
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create a new markdown file
    let file_path = watch_path.join("test.md");
    fs::write(&file_path, "# Test\nContent").unwrap();

    // Wait for debounce + some buffer for filesystem events
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Check for events with retries (filesystem events can be delayed)
    let mut found = false;
    for _ in 0..5 {
        if let Some(paths) = watcher.try_recv() {
            if paths.iter().any(|p| p.ends_with("test.md")) {
                found = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(found, "Should detect new file within retries");
}

// === E2E tests requiring Ollama ===

/// Test complete new user workflow: init -> repo add -> scan -> search
#[tokio::test]
#[ignore]
async fn test_new_user_workflow() {
    require_ollama().await;

    use factbase::{processor::DocumentProcessor, scanner::Scanner, EmbeddingProvider};

    // Start with fresh temp directory (simulating new user)
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("my-notes");
    fs::create_dir_all(&repo_path).unwrap();

    // Create some markdown files
    fs::create_dir_all(repo_path.join("people")).unwrap();
    fs::write(
        repo_path.join("people/alice.md"),
        "# Alice\nAlice is a software engineer.",
    )
    .unwrap();
    fs::write(
        repo_path.join("notes.md"),
        "# Meeting Notes\nDiscussed project with Alice.",
    )
    .unwrap();

    // Initialize database (simulating `factbase init`)
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    // Add repository (simulating `factbase repo add`)
    let repo = common::test_repo("notes", repo_path.clone());
    db.add_repository(&repo).unwrap();

    // Verify repo added
    let repos = db.list_repositories().unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].id, "notes");

    // Scan repository (simulating `factbase scan`)
    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    let files = scanner.find_markdown_files(&repo_path);
    assert_eq!(files.len(), 2);

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
            indexed_at: Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    // Verify documents indexed
    let docs = db.get_documents_for_repo("notes").unwrap();
    assert_eq!(docs.len(), 2);

    // Search (simulating `factbase search`)
    let query_emb = embedding.generate("software engineer").await.unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert!(!results.is_empty());
    // Alice should be in results since she's a software engineer
    assert!(results.iter().any(|r| r.title.contains("Alice")));
}

/// Test AI agent workflow via MCP
#[tokio::test]
#[ignore]
async fn test_agent_workflow_via_mcp() {
    require_ollama().await;

    use factbase::EmbeddingProvider;

    // Setup test environment with indexed documents
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("kb");
    fs::create_dir_all(repo_path.join("projects")).unwrap();
    fs::create_dir_all(repo_path.join("people")).unwrap();

    fs::write(
        repo_path.join("projects/api.md"),
        "# API Project\nBuilding REST API with authentication.",
    )
    .unwrap();
    fs::write(
        repo_path.join("people/bob.md"),
        "# Bob\nBob leads the API Project team.",
    )
    .unwrap();

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = Repository {
        id: "kb".into(),
        name: "Knowledge Base".into(),
        path: repo_path.clone(),
        perspective: Some(Perspective {
            type_name: "team".into(),
            organization: Some("Acme Corp".into()),
            focus: Some("projects and people".into()),
            allowed_types: None,
            review: None,
            format: None,
            link_match_mode: None,
            ..Default::default()
        }),
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Index documents
    use factbase::{processor::DocumentProcessor, scanner::Scanner};
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

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
            indexed_at: Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
        let emb = embedding.generate(&content).await.unwrap();
        db.upsert_embedding(&id, &emb).unwrap();
    }

    // Start MCP server
    let port = common::random_port();
    let server = factbase::mcp::McpServer::new(
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
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    // Agent workflow:
    // 1. Get perspective to understand the knowledge base
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "get_perspective", "arguments": {}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(resp["result"]["name"], "Knowledge Base");

    // 2. Search for relevant information
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "search_knowledge", "arguments": {"query": "API authentication"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let results = resp["result"]["results"].as_array().unwrap();
    assert!(!results.is_empty());

    // 3. Get entity details
    let first_id = results[0]["id"].as_str().unwrap();
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "get_entity", "arguments": {"id": first_id}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(resp["result"]["content"].is_string());

    // 4. List entities by type
    let resp = client
        .post(format!("{}/mcp", base_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "list_entities", "arguments": {"type": "person"}}
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let people = resp["result"]["entities"].as_array().unwrap();
    assert!(people
        .iter()
        .any(|p| p["title"].as_str().unwrap().contains("Bob")));

    // Cleanup
    shutdown_tx.send(()).ok();
}

/// Test system stability under repeated operations
#[tokio::test]
#[ignore]
async fn test_repeated_operations_stability() {
    require_ollama().await;

    let server = TestServer::start_with_data().await;

    // Perform repeated operations
    for i in 0..10 {
        // Health check
        let resp = server.health().await.unwrap();
        assert_eq!(resp.status(), 200, "Health check failed at iteration {}", i);

        // List entities
        let resp = server.call_tool("list_entities", json!({})).await.unwrap();
        assert!(
            resp["result"]["entities"].is_array(),
            "List entities failed at iteration {}",
            i
        );

        // Get perspective
        let resp = server
            .call_tool("get_perspective", json!({}))
            .await
            .unwrap();
        assert!(
            resp["result"]["id"].is_string(),
            "Get perspective failed at iteration {}",
            i
        );

        // Small delay between iterations
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
