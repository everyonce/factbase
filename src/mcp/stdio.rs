use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::llm::LlmProvider;
use crate::mcp::protocol::initialize_result;
use crate::mcp::tools::{handle_tool_call, tools_list, McpRequest, McpResponse};
use futures::FutureExt;
use serde_json::Value;
use std::io::{self, BufRead, Write};
use tracing::{debug, error};

/// Runs the MCP stdio transport loop on stdin/stdout.
///
/// All logging goes to stderr via tracing (never to stdout).
pub async fn run_stdio<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    llm: Option<&dyn LlmProvider>,
) -> anyhow::Result<()> {
    run_stdio_io(db, embedding, llm, io::stdin().lock(), io::stdout().lock()).await
}

/// Writes a progress notification for a long-running tool call.
fn write_progress(
    out: &mut impl Write,
    token: &Value,
    progress: u64,
    total: u64,
    message: Option<&str>,
) -> anyhow::Result<()> {
    let mut params = serde_json::json!({
        "progressToken": token,
        "progress": progress,
        "total": total,
    });
    if let Some(msg) = message {
        params["message"] = Value::String(msg.to_string());
    }
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": params,
    });
    let json = serde_json::to_string(&notification)?;
    writeln!(out, "{json}")?;
    out.flush()?;
    Ok(())
}

/// Runs the MCP stdio transport loop on arbitrary reader/writer.
///
/// Reads newline-delimited JSON-RPC messages, dispatches them,
/// and writes single-line JSON-RPC responses. Exits on EOF.
async fn run_stdio_io<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    llm: Option<&dyn LlmProvider>,
    reader: impl BufRead,
    mut writer: impl Write,
) -> anyhow::Result<()> {
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("stdin read error: {}", e);
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = McpResponse::error(-32700, format!("Parse error: {e}"));
                write_response(&mut writer, &resp)?;
                continue;
            }
        };

        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        debug!(method, "stdio request");

        let response = match method {
            "initialize" => {
                let id = id.clone().unwrap_or(Value::Null);
                Some(McpResponse::success(id, initialize_result()))
            }
            "notifications/initialized" => None,
            "tools/list" => {
                let id = id.clone().unwrap_or(Value::Null);
                Some(McpResponse::success(id, tools_list()))
            }
            "tools/call" => match serde_json::from_value::<McpRequest>(msg.clone()) {
                Ok(request) => {
                    // Extract progressToken from params._meta.progressToken
                    let progress_token = msg
                        .pointer("/params/arguments/_meta/progressToken")
                        .or_else(|| msg.pointer("/params/_meta/progressToken"))
                        .cloned();

                    let (tx, progress) = if progress_token.is_some() {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
                        (Some(rx), Some(tx))
                    } else {
                        (None, None)
                    };

                    let tool_fut = std::panic::AssertUnwindSafe(
                        handle_tool_call(db, embedding, llm, request, progress),
                    )
                    .catch_unwind();
                    tokio::pin!(tool_fut);

                    if let Some(mut rx) = tx {
                        // Safety: progress_token is guaranteed Some when tx channel exists
                        let token = progress_token
                            .expect("progress_token is Some when progress channel is created");
                        loop {
                            tokio::select! {
                                biased;
                                Some(msg) = rx.recv() => {
                                    let p = msg.get("progress").and_then(Value::as_u64).unwrap_or(0);
                                    let t = msg.get("total").and_then(Value::as_u64).unwrap_or(0);
                                    let m = msg.get("message").and_then(|v| v.as_str());
                                    let _ = write_progress(&mut writer, &token, p, t, m);
                                }
                                result = &mut tool_fut => {
                                    // Drain remaining progress
                                    while let Ok(msg) = rx.try_recv() {
                                        let p = msg.get("progress").and_then(Value::as_u64).unwrap_or(0);
                                        let t = msg.get("total").and_then(Value::as_u64).unwrap_or(0);
                                        let m = msg.get("message").and_then(|v| v.as_str());
                                        let _ = write_progress(&mut writer, &token, p, t, m);
                                    }
                                    break match result {
                                        Ok(Ok(resp)) => resp,
                                        Ok(Err(e)) => {
                                            error!("tool call failed: {e}");
                                            Some(McpResponse::error(-32603, format!("Internal error: {e}")))
                                        }
                                        Err(_) => {
                                            error!("tool call panicked");
                                            Some(McpResponse::error(-32603, "Internal error: tool panicked".into()))
                                        }
                                    };
                                }
                            }
                        }
                    } else {
                        match tool_fut.await {
                            Ok(Ok(resp)) => resp,
                            Ok(Err(e)) => {
                                error!("tool call failed: {e}");
                                Some(McpResponse::error(-32603, format!("Internal error: {e}")))
                            }
                            Err(_) => {
                                error!("tool call panicked");
                                Some(McpResponse::error(-32603, "Internal error: tool panicked".into()))
                            }
                        }
                    }
                }
                Err(e) => Some(McpResponse::error(-32600, format!("Invalid request: {e}"))),
            },
            "ping" => {
                let id = id.clone().unwrap_or(Value::Null);
                Some(McpResponse::success(id, serde_json::json!({})))
            }
            _ => Some(McpResponse::error(-32601, "Method not found".into())),
        };

        if let Some(resp) = response {
            // Attach the request id if the error response has a null id
            let resp = if resp.id.is_null() {
                if let Some(req_id) = id {
                    McpResponse { id: req_id, ..resp }
                } else {
                    resp
                }
            } else {
                resp
            };
            write_response(&mut writer, &resp)?;
        }
    }

    debug!("stdio loop exiting (EOF)");
    Ok(())
}

/// Writes a JSON-RPC response as a single line to stdout.
fn write_response(out: &mut impl Write, response: &McpResponse) -> anyhow::Result<()> {
    let json = serde_json::to_string(response)?;
    writeln!(out, "{json}")?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::error::FactbaseError;
    use std::io::Cursor;
    use std::pin::Pin;

    /// Mock embedding provider for e2e stdio tests.
    struct StubEmbedding;

    impl EmbeddingProvider for StubEmbedding {
        fn generate<'a>(
            &'a self,
            _text: &'a str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<f32>, FactbaseError>> + Send + 'a>>
        {
            Box::pin(async { Ok(vec![0.0; 1024]) })
        }

        fn dimension(&self) -> usize {
            1024
        }
    }

    /// End-to-end test: pipe initialize → notifications/initialized → tools/list → ping
    /// through the stdio loop and verify all responses.
    #[tokio::test]
    async fn test_e2e_stdio_lifecycle() {
        let (db, _tmp) = test_db();
        let embedding = StubEmbedding;

        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
        ]
        .join("\n");

        let reader = Cursor::new(input);
        let mut output = Vec::new();
        run_stdio_io(&db, &embedding, None, reader, &mut output)
            .await
            .unwrap();

        let responses: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        // notifications/initialized produces no response → 3 responses total
        assert_eq!(responses.len(), 3);

        // Response 1: initialize
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(responses[0]["result"]["serverInfo"]["name"], "factbase");

        // Response 2: tools/list
        assert_eq!(responses[1]["id"], 2);
        let tools = responses[1]["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty());

        // Response 3: ping
        assert_eq!(responses[2]["id"], 3);
        assert_eq!(responses[2]["result"], serde_json::json!({}));
    }

    #[test]
    fn test_write_response_single_line_success() {
        let resp = McpResponse::success(serde_json::json!(1), serde_json::json!({"ok": true}));
        let mut buf = Vec::new();
        write_response(&mut buf, &resp).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.lines().count(), 1);
        assert!(!output.trim().contains('\n'));
    }

    #[test]
    fn test_write_response_single_line_error() {
        let resp = McpResponse::error(-32601, "Method not found".into());
        let mut buf = Vec::new();
        write_response(&mut buf, &resp).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.lines().count(), 1);
        assert!(!output.trim().contains('\n'));
    }

    #[test]
    fn test_notification_initialized_returns_none() {
        // Simulate the match logic for notifications/initialized
        let method = "notifications/initialized";
        let response: Option<McpResponse> = match method {
            "notifications/initialized" => None,
            _ => Some(McpResponse::error(-32601, "Method not found".into())),
        };
        assert!(response.is_none());
    }

    #[test]
    fn test_initialize_response_format() {
        let id = serde_json::json!(1);
        let resp = McpResponse::success(id, initialize_result());
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(json["result"]["serverInfo"]["name"], "factbase");
        assert!(json["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn test_tools_list_response_format() {
        let id = serde_json::json!(2);
        let resp = McpResponse::success(id, tools_list());
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 2);
        let tools = json["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
        // Every tool has a name
        for tool in tools {
            assert!(tool["name"].is_string(), "tool missing name: {tool}");
        }
    }

    #[test]
    fn test_ping_response_format() {
        let id = serde_json::json!(42);
        let resp = McpResponse::success(id, serde_json::json!({}));
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 42);
        assert_eq!(json["result"], serde_json::json!({}));
    }

    #[test]
    fn test_unknown_method_returns_method_not_found() {
        let method = "some/unknown/method";
        let id = serde_json::json!(99);
        let response: Option<McpResponse> = match method {
            "initialize" | "notifications/initialized" | "tools/list" | "tools/call" | "ping" => {
                panic!("should not match known methods")
            }
            _ => Some(McpResponse::error(-32601, "Method not found".into())),
        };
        let resp = response.unwrap();
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert_eq!(json["error"]["message"], "Method not found");
        // Verify id attachment logic works
        let resp = McpResponse { id, ..resp };
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 99);
    }

    #[test]
    fn test_tools_call_deserializes_to_mcp_request() {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "search_knowledge",
                "arguments": {"query": "test"}
            }
        });
        let request: McpRequest = serde_json::from_value(msg).unwrap();
        assert_eq!(request.method, "tools/call");
        assert_eq!(request.id, Some(serde_json::json!(3)));
        assert_eq!(request.params.name.as_deref(), Some("search_knowledge"));
        assert_eq!(request.params.arguments["query"], "test");
        assert!(!request.is_notification());
    }

    /// End-to-end test: update_document through stdio transport.
    /// Verifies that write operations produce a valid JSON-RPC response
    /// (not a transport error).
    #[tokio::test]
    async fn test_e2e_stdio_update_document() {
        use crate::models::Document;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let embedding = StubEmbedding;

        // Create a repo and document on disk
        let repo_dir = TempDir::new().unwrap();
        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file_path = repo_dir.path().join("test.md");
        std::fs::write(&file_path, "<!-- factbase:abc123 -->\n# Old Title\n\nOld body").unwrap();

        let doc = Document {
            id: "abc123".to_string(),
            repo_id: "test-repo".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            title: "Old Title".to_string(),
            content: "<!-- factbase:abc123 -->\n# Old Title\n\nOld body".to_string(),
            file_hash: "hash1".to_string(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Send update_document via stdio
        let input = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "update_document",
                "arguments": {
                    "id": "abc123",
                    "title": "New Title"
                }
            }
        })
        .to_string();

        let reader = Cursor::new(input);
        let mut output = Vec::new();
        run_stdio_io(&db, &embedding, None, reader, &mut output)
            .await
            .unwrap();

        let resp_str = String::from_utf8(output).unwrap();
        let resp: Value = serde_json::from_str(resp_str.trim()).unwrap();

        // Should be a success response, not an error
        assert_eq!(resp["id"], 1);
        assert!(
            resp.get("error").is_none(),
            "expected success but got error: {resp}"
        );

        // Verify the content field contains the tool result
        let content = resp["result"]["content"][0]["text"].as_str().unwrap();
        let tool_result: Value = serde_json::from_str(content).unwrap();
        assert_eq!(tool_result["id"], "abc123");
        assert_eq!(tool_result["title"], "New Title");

        // Verify file was actually updated
        let file_content = std::fs::read_to_string(&file_path).unwrap();
        assert!(file_content.contains("# New Title"));
    }

    /// End-to-end test: update_document with content writes new body to disk.
    #[tokio::test]
    async fn test_e2e_stdio_update_document_content() {
        use crate::models::Document;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let embedding = StubEmbedding;

        let repo_dir = TempDir::new().unwrap();
        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file_path = repo_dir.path().join("entity.md");
        std::fs::write(
            &file_path,
            "<!-- factbase:def456 -->\n# Old Name\n\n- old fact\n- garbage [^1]",
        )
        .unwrap();

        let doc = Document {
            id: "def456".to_string(),
            repo_id: "test-repo".to_string(),
            file_path: "entity.md".to_string(),
            title: "Old Name".to_string(),
            content: "<!-- factbase:def456 -->\n# Old Name\n\n- old fact\n- garbage [^1]"
                .to_string(),
            file_hash: "hash1".to_string(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Send update_document with new content (including fixed title)
        let input = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "update_document",
                "arguments": {
                    "id": "def456",
                    "content": "<!-- factbase:def456 -->\n# Fixed Name\n\n- cleaned fact"
                }
            }
        })
        .to_string();

        let reader = Cursor::new(input);
        let mut output = Vec::new();
        run_stdio_io(&db, &embedding, None, reader, &mut output)
            .await
            .unwrap();

        let resp_str = String::from_utf8(output).unwrap();
        let resp: Value = serde_json::from_str(resp_str.trim()).unwrap();
        assert!(resp.get("error").is_none(), "expected success: {resp}");

        let content = resp["result"]["content"][0]["text"].as_str().unwrap();
        let tool_result: Value = serde_json::from_str(content).unwrap();
        assert_eq!(tool_result["title"], "Fixed Name");

        // Verify file on disk has new content
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("# Fixed Name"),
            "title should be updated on disk: {on_disk}"
        );
        assert!(
            on_disk.contains("- cleaned fact"),
            "body should be updated on disk: {on_disk}"
        );
        assert!(
            !on_disk.contains("garbage"),
            "old content should be gone: {on_disk}"
        );
    }
}
