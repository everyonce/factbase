//! Integration tests for MCP server HTTP endpoints.
//! These tests REQUIRE Ollama to be running - they will fail if unavailable.

mod common;

use common::ollama_helpers::require_ollama;
use common::TestServer;
use serde_json::json;

// --- Tests ---

#[tokio::test]
async fn test_health_endpoint() {
    let server = TestServer::start().await;
    let resp = server.health().await.expect("operation should succeed");
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_search_knowledge() {
    require_ollama().await;

    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("search_knowledge", json!({"query": "software engineer"}))
        .await
        .expect("operation should succeed");

    assert_eq!(resp["jsonrpc"], "2.0");
    // May return error if embeddings not generated, or results if they are
    if resp["error"].is_null() {
        assert!(resp["result"]["results"].is_array());
    }
}

#[tokio::test]
async fn test_get_entity_valid() {
    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("get_entity", json!({"id": "doc1"}))
        .await
        .expect("operation should succeed");

    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["result"].is_object());
    assert_eq!(resp["result"]["id"], "doc1");
    assert_eq!(resp["result"]["title"], "Alice Smith");
    assert_eq!(resp["result"]["type"], "person");
}

#[tokio::test]
async fn test_get_entity_not_found() {
    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("get_entity", json!({"id": "nonexistent"}))
        .await
        .expect("operation should succeed");

    assert!(resp["error"].is_object());
    assert!(resp["error"]["message"]
        .as_str()
        .expect("operation should succeed")
        .contains("not found"));
}

#[tokio::test]
async fn test_list_entities() {
    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("list_entities", json!({}))
        .await
        .expect("operation should succeed");

    assert!(resp["result"]["entities"].is_array());
    let entities = resp["result"]["entities"]
        .as_array()
        .expect("operation should succeed");
    assert_eq!(entities.len(), 3);
}

#[tokio::test]
async fn test_list_entities_with_type_filter() {
    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("list_entities", json!({"type": "person"}))
        .await
        .expect("operation should succeed");

    let entities = resp["result"]["entities"]
        .as_array()
        .expect("operation should succeed");
    assert_eq!(entities.len(), 2);
    for e in entities {
        assert_eq!(e["type"], "person");
    }
}

#[tokio::test]
async fn test_get_perspective() {
    let server = TestServer::start_with_data().await;
    let resp = server
        .call_tool("get_perspective", json!({}))
        .await
        .expect("operation should succeed");

    assert_eq!(resp["result"]["id"], "test-repo");
    assert_eq!(resp["result"]["name"], "Test Repo");
    assert!(resp["result"]["perspective"].is_object());
}

#[tokio::test]
async fn test_concurrent_requests() {
    let server = TestServer::start_with_data().await;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let client = server.client.clone();
            let url = format!("{}/mcp", server.base_url);
            tokio::spawn(async move {
                let request = json!({
                    "jsonrpc": "2.0",
                    "id": i,
                    "method": "tools/call",
                    "params": {"name": "list_entities", "arguments": {}}
                });
                client.post(&url).json(&request).send().await
            })
        })
        .collect();

    let mut success = 0;
    for h in handles {
        if h.await.expect("operation should succeed").is_ok() {
            success += 1;
        }
    }
    assert_eq!(success, 10);
}

#[tokio::test]
async fn test_unknown_tool() {
    let server = TestServer::start().await;
    let resp = server
        .call_tool("unknown_tool", json!({}))
        .await
        .expect("operation should succeed");

    assert!(resp["error"].is_object());
    assert!(resp["error"]["message"]
        .as_str()
        .expect("operation should succeed")
        .contains("Unknown tool"));
}
