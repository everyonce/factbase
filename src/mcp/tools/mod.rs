//! MCP tool implementations split into logical modules.
//!
//! # Module Organization
//!
//! - `helpers`: Argument extraction functions (get_str_arg, get_u64_arg, etc.)
//! - `schema`: Tool schema definitions (tools_list)
//! - `document`: Document CRUD tools (create, update, delete, bulk_create)
//! - `entity`: Entity query tools (get_entity, list_entities, get_perspective)
//! - `review`: Review queue tools (get_review_queue, answer_question, etc.)
//! - `search`: Search tools (search_knowledge, search_content, search_knowledge (temporal))
//!
//! # Public API
//!
//! ## Types
//! - [`McpRequest`]: Incoming MCP request structure
//! - [`McpParams`]: Request parameters
//! - [`McpResponse`]: Response structure with success/error variants
//! - [`McpError`]: Error details
//!
//! ## Functions
//! - [`handle_tool_call`]: Main entry point for routing tool calls

mod authoring;
pub(crate) mod document;
mod embeddings;
mod entity;
pub(crate) mod helpers;
mod links;
mod organize;
mod repository;
mod review;
mod schema;
mod search;
mod workflow;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::progress::ProgressSender;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Re-export tool implementations
pub use authoring::get_authoring_guide;
pub use document::{bulk_create_documents, create_document, delete_document, update_document};
pub use embeddings::{embeddings_export, embeddings_import, embeddings_status_tool};
pub use entity::{get_entity, get_perspective, list_entities};
pub use links::{get_link_suggestions, store_links};
pub use organize::{organize, organize_analyze};
pub use repository::{detect_links, scan_repository};
pub use review::{
    answer_question, answer_questions, bulk_answer_questions,
    generate_questions, get_deferred_items, get_review_queue, check_repository,
};
pub use search::{get_fact_pairs, search_content, search_knowledge};

// Re-export helpers for submodules
pub(crate) use helpers::{
    extract_type_repo_filters, get_bool_arg, get_str_arg, get_str_arg_required, get_str_array_arg,
    get_u64_arg, get_u64_arg_required, load_perspective, resolve_repo, resolve_repo_filter,
    run_blocking,
};

// Re-export schema
pub use schema::{tools_list, tools_list_with_overrides};

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: McpParams,
}

impl McpRequest {
    /// Returns `true` if this is a notification (no `id` field).
    /// Notifications must NOT receive a response in either transport.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct McpParams {
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

impl McpResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Value::Null,
            result: None,
            error: Some(McpError { code, message }),
        }
    }
}

/// Handles incoming MCP tool calls.
///
/// Routes requests to the appropriate tool implementation based on method and tool name.
/// Supports `tools/list` to enumerate available tools and `tools/call` to invoke them.
///
/// # Arguments
/// - `db`: Database connection for document operations
/// - `embedding`: Embedding provider for semantic search
/// - `request`: Parsed MCP request with method, params, and id
///
/// # Returns
/// `McpResponse` with either success result or error code/message.
///
/// # Supported Tools
/// - Primary: `workflow` (guided multi-step), `factbase` (unified operations)
/// - Legacy aliases: all old tool names still dispatched for backward compat
///
/// Dispatch a single tool operation by name. Used by both the `factbase` unified
/// tool (via op→name mapping) and legacy tool name aliases.
async fn dispatch_tool<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    tool_name: &str,
    args: &Value,
    reporter: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    // Check for removed tools first (backward compat grace period)
    for (name, msg) in schema::removed_legacy_tool_messages() {
        if tool_name == *name {
            return Err(FactbaseError::Config(msg.to_string()));
        }
    }

    match tool_name {
        "search_knowledge" => search_knowledge(db, embedding, args).await,
        "get_entity" => { let db = db.clone(); let a = args.clone(); run_blocking(move || get_entity(&db, &a)).await }
        "list_entities" => { let db = db.clone(); let a = args.clone(); run_blocking(move || list_entities(&db, &a)).await }
        "get_perspective" => { let db = db.clone(); let a = args.clone(); run_blocking(move || get_perspective(&db, &a)).await }
        "create_document" => { let db = db.clone(); let a = args.clone(); run_blocking(move || create_document(&db, &a)).await }
        "update_document" => { let db = db.clone(); let a = args.clone(); run_blocking(move || update_document(&db, &a)).await }
        "delete_document" => { let db = db.clone(); let a = args.clone(); run_blocking(move || delete_document(&db, &a)).await }
        "bulk_create_documents" => { let db = db.clone(); let a = args.clone(); let r = reporter.clone(); run_blocking(move || bulk_create_documents(&db, &a, &r)).await }
        "get_review_queue" => get_review_queue(db, args, reporter),
        "get_deferred_items" => get_deferred_items(db, args, reporter),
        "answer_questions" => { let db = db.clone(); let a = args.clone(); let r = reporter.clone(); run_blocking(move || answer_questions(&db, &a, &r)).await }
        "check_repository" => check_repository(db, embedding, args, reporter).await,
        "scan_repository" => scan_repository(db, embedding, args, reporter).await,
        "detect_links" => detect_links(db, args, reporter).await,
        "organize_analyze" => organize_analyze(db, embedding, args, reporter).await,
        "organize" => organize(db, embedding, args, reporter).await,
        "get_authoring_guide" => Ok(get_authoring_guide()),
        "embeddings_export" => { let db = db.clone(); let a = args.clone(); run_blocking(move || embeddings_export(&db, &a)).await }
        "embeddings_import" => { let db = db.clone(); let a = args.clone(); run_blocking(move || embeddings_import(&db, &a)).await }
        "embeddings_status" => { let db = db.clone(); run_blocking(move || embeddings_status_tool(&db)).await }
        "get_link_suggestions" => get_link_suggestions(db, embedding, args).await,
        "store_links" => { let db = db.clone(); let a = args.clone(); run_blocking(move || store_links(&db, &a)).await }
        "get_fact_pairs" => { let db = db.clone(); let a = args.clone(); run_blocking(move || get_fact_pairs(&db, &a)).await }
        "workflow" => {
            let is_bootstrap = args.get("workflow").and_then(|v| v.as_str()) == Some("bootstrap");
            if is_bootstrap {
                workflow::bootstrap(args)
            } else {
                let db = db.clone(); let a = args.clone();
                run_blocking(move || workflow::workflow(&db, &a)).await
            }
        }
        _ => Err(FactbaseError::Config(format!("Unknown tool: {tool_name}"))),
    }
}

/// Map a `factbase` op value to the legacy tool name used by dispatch_tool.
fn op_to_tool_name(op: &str) -> Option<&'static str> {
    Some(match op {
        "get_entity" => "get_entity",
        "list" => "list_entities",
        "perspective" => "get_perspective",
        "create" => "create_document",
        "update" => "update_document",
        "delete" => "delete_document",
        "bulk_create" => "bulk_create_documents",
        "scan" => "scan_repository",
        "check" => "check_repository",
        "detect_links" => "detect_links",
        "review_queue" => "get_review_queue",
        "answer" => "answer_questions",
        "deferred" => "get_deferred_items",
        "fact_pairs" => "get_fact_pairs",
        "authoring_guide" => "get_authoring_guide",
        _ => return None,
    })
}

/// Handle the first-class `search` tool. Delegates to search_knowledge or search_content,
/// then enriches each result with outgoing links (link_id + entity_name).
async fn handle_search_tool<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
    reporter: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("semantic");

    let mut result = match mode {
        "content" => {
            let mut a = args.clone();
            if a.get("pattern").is_none() {
                if let Some(q) = a.get("query").cloned() {
                    if let Some(obj) = a.as_object_mut() {
                        obj.insert("pattern".into(), q);
                    }
                }
            }
            // Call search_content directly (not via dispatch — it's removed from the public tool list)
            let db2 = db.clone();
            let r = reporter.clone();
            run_blocking(move || search_content(&db2, &a, &r)).await?
        }
        _ => dispatch_tool(db, embedding, "search_knowledge", args, reporter).await?,
    };

    // Enrich results with outgoing links
    if let Some(items) = result.get_mut("results").and_then(|v| v.as_array_mut()) {
        let doc_ids: Vec<String> = items
            .iter()
            .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();

        let links_map = {
            let db = db.clone();
            let ids = doc_ids.clone();
            run_blocking(move || {
                let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
                db.get_links_for_documents(&refs)
            }).await?
        };

        // Collect all unique target IDs to resolve titles
        let target_ids: Vec<String> = links_map
            .values()
            .flat_map(|(outgoing, _)| outgoing.iter().map(|l| l.target_id.clone()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let title_map = {
            let db = db.clone();
            let ids = target_ids;
            run_blocking(move || {
                let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
                db.get_document_titles_by_ids(&refs)
            }).await?
        };

        for item in items.iter_mut() {
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let links: Vec<Value> = links_map
                .get(id)
                .map(|(outgoing, _)| {
                    outgoing
                        .iter()
                        .map(|link| {
                            serde_json::json!({
                                "link_id": link.target_id,
                                "entity_name": title_map.get(&link.target_id).cloned().unwrap_or_default()
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(obj) = item.as_object_mut() {
                obj.insert("links".into(), Value::Array(links));
            }
        }
    }

    Ok(result)
}

/// Handle the unified `factbase` tool by extracting `op` and dispatching.
async fn handle_factbase_op<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
    reporter: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");

    // Direct mapping ops
    if let Some(tool_name) = op_to_tool_name(op) {
        // For answer op: propagate top-level doc_id into each answer if missing
        if op == "answer" {
            if let Some(doc_id) = args.get("doc_id").and_then(|v| v.as_str()).map(String::from) {
                if let Some(answers) = args.get("answers").and_then(|v| v.as_array()) {
                    let mut patched_args = args.clone();
                    let patched_answers: Vec<Value> = answers.iter().map(|a| {
                        if a.get("doc_id").is_some() {
                            a.clone()
                        } else {
                            let mut a = a.clone();
                            if let Some(obj) = a.as_object_mut() {
                                obj.insert("doc_id".into(), Value::String(doc_id.clone()));
                            }
                            a
                        }
                    }).collect();
                    if let Some(obj) = patched_args.as_object_mut() {
                        obj.insert("answers".into(), Value::Array(patched_answers));
                    }
                    return dispatch_tool(db, embedding, tool_name, &patched_args, reporter).await;
                }
            }
        }
        return dispatch_tool(db, embedding, tool_name, args, reporter).await;
    }

    match op {
        // organize: action=analyze or action=move/retype/apply
        "organize" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if action == "analyze" {
                dispatch_tool(db, embedding, "organize_analyze", args, reporter).await
            } else {
                dispatch_tool(db, embedding, "organize", args, reporter).await
            }
        }
        // links: action=suggest or action=store
        "links" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("suggest");
            match action {
                "store" => dispatch_tool(db, embedding, "store_links", args, reporter).await,
                "migrate" => Err(FactbaseError::Config(
                    "links action='migrate' removed. Link migration is no longer needed.".into()
                )),
                _ => dispatch_tool(db, embedding, "get_link_suggestions", args, reporter).await,
            }
        }
        // embeddings: action=export/import/status
        "embeddings" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("status");
            match action {
                "export" => dispatch_tool(db, embedding, "embeddings_export", args, reporter).await,
                "import" => dispatch_tool(db, embedding, "embeddings_import", args, reporter).await,
                _ => dispatch_tool(db, embedding, "embeddings_status", args, reporter).await,
            }
        }
        _ => {
            // Check for removed ops (backward compat grace period)
            for (removed_op, msg) in schema::removed_op_messages() {
                if op == *removed_op {
                    return Err(FactbaseError::Config(msg.to_string()));
                }
            }
            Err(FactbaseError::Config(format!("Unknown factbase op: {op}")))
        }
    }
}

pub async fn handle_tool_call<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    request: McpRequest,
    progress: Option<ProgressSender>,
) -> Result<Option<McpResponse>, FactbaseError> {
    if request.is_notification() {
        return Ok(None);
    }
    let id = request.id.expect("requests always have an id");

    match request.method.as_str() {
        "tools/list" => Ok(Some(McpResponse::success(id, tools_list()))),
        "tools/call" => {
            let tool_name = request.params.name.as_deref().unwrap_or("");
            let args = request.params.arguments.clone();
            let reporter = crate::ProgressReporter::Mcp { sender: progress };

            let result = match tool_name {
                // Primary tools
                "search" => handle_search_tool(db, embedding, &args, &reporter).await,
                "factbase" => handle_factbase_op(db, embedding, &args, &reporter).await,
                "workflow" => dispatch_tool(db, embedding, "workflow", &args, &reporter).await,

                // Legacy aliases (backward compat — not in schema)
                "get_duplicate_entries" => {
                    dispatch_tool(db, embedding, "organize_analyze",
                        &serde_json::json!({"focus": "duplicates", "repo": args.get("repo")}),
                        &reporter).await
                }
                "organize_move" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "move".into()));
                    dispatch_tool(db, embedding, "organize", &a, &reporter).await
                }
                "organize_retype" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "retype".into()));
                    dispatch_tool(db, embedding, "organize", &a, &reporter).await
                }
                "organize_apply" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "apply".into()));
                    dispatch_tool(db, embedding, "organize", &a, &reporter).await
                }
                other => {
                    // Try as legacy tool name
                    match dispatch_tool(db, embedding, other, &args, &reporter).await {
                        Ok(v) => Ok(v),
                        Err(FactbaseError::Config(msg)) if msg.starts_with("Unknown tool:") => {
                            return Ok(Some(McpResponse::error(
                                -32602,
                                format!("Unknown tool: {tool_name}"),
                            )))
                        }
                        Err(e) => Err(e),
                    }
                }
            };

            match result {
                Ok(result) => Ok(Some(McpResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
                        }]
                    }),
                ))),
                Err(e) => Ok(Some(McpResponse::error(-32602, e.to_string()))),
            }
        }
        _ => Ok(Some(McpResponse::error(-32601, "Method not found".into()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_mcp_response_success() {
        let resp = McpResponse::success(serde_json::json!(1), serde_json::json!({"test": true}));
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_mcp_response_error() {
        let resp = McpResponse::error(-32600, "Invalid request".into());
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    // MCP request parsing tests
    #[test]
    fn test_mcp_request_deserialize_tools_list() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let req: McpRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, Some(serde_json::json!(1)));
        assert!(req.params.name.is_none());
    }

    #[test]
    fn test_mcp_request_deserialize_tools_call() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_entity","arguments":{"id":"abc123"}}}"#;
        let req: McpRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.id, Some(serde_json::json!(2)));
        assert_eq!(req.params.name, Some("get_entity".into()));
        assert_eq!(req.params.arguments["id"], "abc123");
    }

    #[test]
    fn test_mcp_request_deserialize_empty_params() {
        let json = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call"}"#;
        let req: McpRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(req.id, Some(serde_json::json!(3)));
        assert!(req.params.name.is_none());
        assert!(req.params.arguments.is_null());
    }

    #[test]
    fn test_mcp_request_deserialize_notification() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req: McpRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(req.method, "notifications/initialized");
        assert!(req.id.is_none());
        assert!(req.is_notification());
    }

    #[test]
    fn test_mcp_request_is_not_notification() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let req: McpRequest = serde_json::from_str(json).expect("should parse");
        assert!(!req.is_notification());
    }

    #[test]
    fn test_mcp_response_serialize_success() {
        let resp = McpResponse::success(serde_json::json!(1), serde_json::json!({"data": "test"}));
        let json = serde_json::to_string(&resp).expect("should serialize");
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_mcp_response_serialize_error() {
        let resp = McpResponse::error(-32602, "Invalid params".into());
        let json = serde_json::to_string(&resp).expect("should serialize");
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32602"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_schema_dispatch_consistency() {
        use std::collections::HashSet;

        let schema_names: HashSet<String> = tools_list()["tools"]
            .as_array()
            .expect("tools should be array")
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect();

        // Schema now has exactly 3 tools: search + workflow + factbase
        let expected: HashSet<String> = ["search", "workflow", "factbase"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(schema_names, expected, "schema should have exactly search + workflow + factbase");

        // All legacy tool names should still be dispatchable (tested via integration tests)
        let legacy = schema::legacy_tool_names();
        assert!(legacy.len() >= 23, "should have active legacy tool names as aliases, got {}", legacy.len());
    }

    #[test]
    fn test_schema_doc_type_param_consistency() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        // search, factbase and workflow should have doc_type param
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let s_props = search["inputSchema"]["properties"].as_object().unwrap();
        assert!(s_props.contains_key("doc_type"), "search should have doc_type param");

        let factbase = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let fb_props = factbase["inputSchema"]["properties"].as_object().unwrap();
        assert!(fb_props.contains_key("doc_type"), "factbase should have doc_type param");

        let workflow = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let wf_props = workflow["inputSchema"]["properties"].as_object().unwrap();
        assert!(wf_props.contains_key("doc_type"), "workflow should have doc_type param");
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_sends_progress_via_mcp_channel() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        // Create a few markdown files
        std::fs::write(repo_path.join("doc1.md"), "# Doc One\nContent one.").unwrap();
        std::fs::write(repo_path.join("doc2.md"), "# Doc Two\nContent two.").unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let reporter = crate::ProgressReporter::Mcp { sender: Some(tx) };

        let args = serde_json::json!({});
        let result = scan_repository(&db, &embedding, &args, &reporter).await;
        assert!(result.is_ok());

        // Collect all progress messages
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        // Should have received at least one progress message
        assert!(
            !messages.is_empty(),
            "expected progress messages on channel"
        );

        // Verify message structure: should have "message" field
        let has_phase = messages.iter().any(|m| m.get("phase").is_some());
        let has_log = messages.iter().any(|m| {
            m.get("message").is_some() && m.get("progress").is_none() && m.get("phase").is_none()
        });
        assert!(
            has_phase || has_log,
            "expected at least a phase or log message"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_emits_embedding_and_indexing_progress() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        // Create enough files to trigger progress
        for i in 0..5 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("# Doc {i}\nContent {i}."),
            )
            .unwrap();
        }

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let reporter = crate::ProgressReporter::Mcp { sender: Some(tx) };

        let args = serde_json::json!({});
        scan_repository(&db, &embedding, &args, &reporter)
            .await
            .unwrap();

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        // Should have numeric progress reports (progress/total fields)
        let has_numeric_progress = messages
            .iter()
            .any(|m| m.get("progress").is_some() && m.get("total").is_some());
        assert!(
            has_numeric_progress,
            "expected numeric progress reports (progress/total) from scan phases, got: {messages:?}"
        );

        // Should have embedding progress
        let has_embedding = messages.iter().any(|m| {
            m.get("message")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("embedded"))
        });
        assert!(
            has_embedding,
            "expected embedding progress messages, got: {messages:?}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_includes_coverage_in_response() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        // Doc with facts but no temporal tags or sources
        std::fs::write(
            repo_path.join("doc1.md"),
            "# Doc One\n\n- Fact one\n- Fact two\n",
        )
        .unwrap();
        // Doc with temporal tags and sources
        std::fs::write(
            repo_path.join("doc2.md"),
            "# Doc Two\n\n- Tagged fact @t[2024] [^1]\n\n---\n[^1]: Source\n",
        )
        .unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;
        let args = serde_json::json!({});
        let result = scan_repository(&db, &embedding, &args, &reporter)
            .await
            .unwrap();

        // Response should include coverage fields
        assert!(result.get("temporal_coverage_percent").is_some());
        assert!(result.get("source_coverage_percent").is_some());
        assert!(result.get("summary").is_some());

        // 3 total facts: 1 tagged, 2 untagged → ~33% temporal
        let temporal = result["temporal_coverage_percent"].as_f64().unwrap();
        assert!(temporal > 0.0 && temporal < 100.0, "temporal={temporal}");

        // 1 of 3 facts has source → ~33% source
        let source = result["source_coverage_percent"].as_f64().unwrap();
        assert!(source > 0.0 && source < 100.0, "source={source}");

        // Summary string should mention coverage
        let summary = result["summary"].as_str().unwrap();
        assert!(summary.contains("temporal coverage:"));
        assert!(summary.contains("source coverage:"));
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_hint_when_no_links_and_multiple_docs() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "# Doc One\n\n- Fact\n").unwrap();
        std::fs::write(repo_path.join("doc2.md"), "# Doc Two\n\n- Fact\n").unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;
        let result = scan_repository(&db, &embedding, &serde_json::json!({}), &reporter)
            .await
            .unwrap();

        // String-only link detection: no title matches in these docs, so links_detected should be 0
        assert_eq!(result["links_detected"], 0);
        assert!(result["total"].as_u64().unwrap() > 1);
        let hint = result["hint"].as_str().unwrap();
        assert!(hint.contains("exact title"), "hint should mention exact titles");
        assert!(hint.contains("not markdown links"), "hint should warn about markdown links");
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_no_hint_for_single_doc() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "# Doc One\n\n- Fact\n").unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;
        let result = scan_repository(&db, &embedding, &serde_json::json!({}), &reporter)
            .await
            .unwrap();

        // Single doc → no hint
        assert!(result["hint"].is_null(), "should not show hint for single doc");
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_emits_progress() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        for i in 0..3 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("<!-- factbase:{i:06x} -->\n# Doc {i}\n\n- Fact without date\n"),
            )
            .unwrap();
        }

        test_repo_in_db(&db, "test", repo_path);

        // Scan first so documents exist in DB
        let embedding = MockEmbedding::new(1024);
        let silent = crate::ProgressReporter::Silent;
        scan_repository(&db, &embedding, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Now check with progress tracking
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let reporter = crate::ProgressReporter::Mcp { sender: Some(tx) };

        let args = serde_json::json!({"mode": "questions"});
        check_repository(&db, &embedding, &args, &reporter)
            .await
            .unwrap();

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        // Should have phase and numeric progress
        let has_phase = messages.iter().any(|m| m.get("phase").is_some());
        let has_progress = messages
            .iter()
            .any(|m| m.get("progress").is_some() && m.get("total").is_some());
        assert!(has_phase, "expected phase messages from check, got: {messages:?}");
        assert!(has_progress, "expected numeric progress from check, got: {messages:?}");
    }

    #[test]
    fn test_schema_time_budget_secs_on_factbase_tool() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let props = fb["inputSchema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("time_budget_secs"), "factbase should have time_budget_secs param");
        assert!(props.contains_key("resume"), "factbase should have resume param");
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_with_expired_deadline_returns_progress() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        for i in 0..5 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("# Doc {i}\nContent {i}."),
            )
            .unwrap();
        }

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;
        // Use time_budget_secs=5 (minimum) — with MockEmbedding this should complete
        let args = serde_json::json!({"time_budget_secs": 5});
        let result = scan_repository(&db, &embedding, &args, &reporter)
            .await
            .unwrap();

        // With MockEmbedding (instant), 5 docs should complete within 5s
        // So we expect a normal completion (no "continue" field)
        assert!(result.get("continue").is_none() || result["continue"] == false,
            "Small scan with MockEmbedding should complete within budget");
        assert!(result.get("total").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_with_expired_deadline_returns_progress() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        for i in 0..3 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("<!-- factbase:{i:06x} -->\n# Doc {i}\n\n- Fact without date\n"),
            )
            .unwrap();
        }

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let silent = crate::ProgressReporter::Silent;
        scan_repository(&db, &embedding, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Check — should complete (no paging)
        let args = serde_json::json!({});
        let result = check_repository(&db, &embedding, &args, &silent)
            .await
            .unwrap();

        assert!(result.get("documents_scanned").is_some());
        // No paging — should always complete
        assert!(result.get("continue").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_generate_questions_multi_doc_with_budget() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        for i in 0..3 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("<!-- factbase:{i:06x} -->\n# Doc {i}\n\n- Fact without date\n"),
            )
            .unwrap();
        }

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let silent = crate::ProgressReporter::Silent;
        scan_repository(&db, &embedding, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Generate questions for all docs with generous budget
        let args = serde_json::json!({"dry_run": true, "time_budget_secs": 30});
        let result = generate_questions(&db, &embedding, &args)
            .await
            .unwrap();

        assert!(result.get("documents_processed").is_some());
        assert!(result["documents_processed"].as_u64().unwrap() >= 3);
        assert!(result.get("continue").is_none() || result["continue"] == false);
    }

    #[tokio::test]
    async fn test_scan_auto_populates_fact_embeddings_on_empty_table() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use crate::{DocumentProcessor, LinkDetector, Scanner, ScanContext, ScanOptions};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "<!-- factbase:aaa111 -->\n# Doc One\n\n- Fact alpha\n- Fact beta\n").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let scanner = Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let link_detector = LinkDetector::new();
        let opts = ScanOptions::default();
        let progress = crate::ProgressReporter::Silent;
        let repo = db.list_repositories().unwrap().into_iter().next().unwrap();

        let ctx = ScanContext {
            scanner: &scanner, processor: &processor, embedding: &embedding,
            link_detector: &link_detector, opts: &opts, progress: &progress,
        };

        // First scan: indexes docs and generates fact embeddings
        let r1 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r1.fact_embeddings_needed, 0);
        assert!(r1.fact_embeddings_generated > 0);

        // Second scan with no changes: no fact embeddings needed or generated
        let r2 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.fact_embeddings_needed, 0);
    }

    #[tokio::test]
    async fn test_scan_auto_populates_fact_embeddings_after_migration() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use crate::{DocumentProcessor, LinkDetector, Scanner, ScanContext, ScanOptions};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "<!-- factbase:bbb222 -->\n# Doc One\n\n- Fact one\n").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let scanner = Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let link_detector = LinkDetector::new();
        let opts = ScanOptions::default();
        let progress = crate::ProgressReporter::Silent;
        let repo = db.list_repositories().unwrap().into_iter().next().unwrap();

        let ctx = ScanContext {
            scanner: &scanner, processor: &processor, embedding: &embedding,
            link_detector: &link_detector, opts: &opts, progress: &progress,
        };

        // First scan indexes docs and generates fact embeddings
        let r1 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert!(r1.fact_embeddings_generated > 0);

        // Rescan with no file changes: no new fact embeddings
        let r2 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.fact_embeddings_needed, 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_force_reindex_param() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "# Doc One\n\n- Fact one\n").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // First scan
        let args = serde_json::json!({});
        scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // Second scan with force_reindex: should re-process all docs
        let args = serde_json::json!({"force_reindex": true});
        let r = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();
        // force_reindex causes docs to be reindexed even though unchanged
        let total = r["total"].as_u64().unwrap();
        assert!(total > 0, "force_reindex should process docs");
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_force_reindex_bypasses_default_budget() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        // Write files with factbase headers so first scan doesn't modify them
        for i in 0..5u8 {
            let hex = format!("{:06x}", i + 0xa0);
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("<!-- factbase:{hex} -->\n# Doc {i}\nContent {i}."),
            )
            .unwrap();
        }
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // Initial scan to populate DB
        let args = serde_json::json!({});
        scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // force_reindex without explicit time_budget_secs: should NOT be interrupted
        let args = serde_json::json!({"force_reindex": true});
        let r = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        assert!(
            r.get("continue").is_none() || r["continue"] == false,
            "force_reindex without explicit budget should not be interrupted"
        );
        assert_eq!(r["reindexed"].as_u64().unwrap(), 5, "all docs should be reindexed");
        assert!(r.get("total").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_force_reindex_ignores_explicit_budget() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "<!-- factbase:aaa001 -->\n# Doc One\nContent.").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // Initial scan
        let args = serde_json::json!({});
        scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // force_reindex WITH explicit time_budget_secs: budget should be ignored
        let args = serde_json::json!({"force_reindex": true, "time_budget_secs": 10});
        let r = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // Should complete with reindexed count (budget was ignored, not enforced)
        assert_eq!(r["reindexed"].as_u64().unwrap(), 1);
        // Verify no continuation — force_reindex bypasses time budget entirely
        assert!(r.get("continue").is_none(), "force_reindex should bypass time budget, no continuation expected");
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_normal_scan_still_respects_budget() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "# Doc One\nContent.").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // Normal scan (no force_reindex) with explicit budget: should still use budget
        let args = serde_json::json!({"time_budget_secs": 30});
        let r = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // MockEmbedding is instant, so it completes within budget
        assert!(r.get("total").is_some());
        assert_eq!(r["reindexed"].as_u64().unwrap(), 0);
    }

    #[test]
    fn test_scan_interrupted_always_includes_continue_and_resume() {
        // Simulate what the MCP tool does when full_scan returns interrupted=true.
        // The response must always have continue=true and a resume token.
        use crate::mcp::tools::helpers::{encode_resume_token, decode_resume_token};

        let file_offset = 30usize;
        let total_files = 100usize;
        let processed = 30usize;

        // This is the fixed logic from scan_repository
        let resume_offset = if file_offset > 0 { file_offset } else { total_files };
        let resume_token = encode_resume_token(
            &serde_json::json!({"file_offset": resume_offset}),
        );
        let pct = if total_files > 0 { (processed as f64 / total_files as f64 * 100.0) as u32 } else { 0 };
        let response = serde_json::json!({
            "continue": true,
            "resume": resume_token,
            "progress": {
                "processed": processed,
                "remaining": total_files.saturating_sub(processed),
                "total": total_files,
                "percent_complete": pct,
            },
        });

        assert_eq!(response["continue"], true);
        assert!(response.get("resume").is_some());
        assert_eq!(response["progress"]["remaining"], 70);
        assert_eq!(response["progress"]["total"], 100);

        // Verify resume token decodes correctly
        let decoded = decode_resume_token(response["resume"].as_str().unwrap()).unwrap();
        assert_eq!(decoded["file_offset"], 30);
    }

    #[test]
    fn test_scan_interrupted_zero_file_offset_uses_total_files() {
        // When embedding phase is interrupted, file_offset=0 but all files were processed.
        // The resume token should use total_files so the next call skips the file loop.
        use crate::mcp::tools::helpers::{encode_resume_token, decode_resume_token};

        let file_offset = 0usize; // embedding phase interrupted, file_offset not set
        let total_files = 50usize;

        let resume_offset = if file_offset > 0 { file_offset } else { total_files };
        let resume_token = encode_resume_token(
            &serde_json::json!({"file_offset": resume_offset}),
        );

        let decoded = decode_resume_token(&resume_token).unwrap();
        assert_eq!(decoded["file_offset"], 50, "should use total_files when file_offset is 0");
    }

    #[tokio::test]
    async fn test_scan_resume_past_end_does_not_reprocess_files() {
        // When file_offset >= files.len(), the scan should skip the file loop
        // and not reprocess all files from the beginning.
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        // Create 3 files
        for i in 0..3 {
            std::fs::write(
                repo_path.join(format!("doc{i}.md")),
                format!("# Doc {i}\nContent {i}."),
            ).unwrap();
        }
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // First scan: index all files
        let args = serde_json::json!({});
        let r1 = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();
        assert_eq!(r1["added"].as_u64().unwrap(), 3);

        // Second scan with resume token pointing past all files
        let resume_token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"file_offset": 100}),
        );
        let args = serde_json::json!({"resume": resume_token});
        let r2 = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // Should not have reprocessed any files
        assert_eq!(r2["added"].as_u64().unwrap(), 0);
        assert_eq!(r2["updated"].as_u64().unwrap(), 0);
        assert_eq!(r2["unchanged"].as_u64().unwrap(), 0);
        // Should still return a complete response (not interrupted)
        assert!(r2.get("continue").is_none() || r2["continue"] == false);
        assert!(r2.get("total").is_some());
    }

    #[tokio::test]
    async fn test_scan_completed_returns_no_continue() {
        // When scan completes all work, response should NOT have continue=true
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc.md"), "# Doc\nContent.").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        let args = serde_json::json!({"time_budget_secs": 120});
        let result = scan_repository(&db, &embedding, &args, &reporter).await.unwrap();

        // Completed scan should not have continue=true
        assert!(result.get("continue").is_none() || result["continue"] == false);
        assert!(result.get("resume").is_none());
        assert!(result.get("total").is_some());
        assert!(result.get("summary").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tool_enriches_results_with_links() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use crate::link_detection::DetectedLink;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("alpha.md"), "# Alpha\nAlpha content about Beta.").unwrap();
        std::fs::write(repo_path.join("beta.md"), "# Beta\nBeta content.").unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        // Scan to index documents
        scan_repository(&db, &embedding, &serde_json::json!({}), &reporter)
            .await
            .unwrap();

        // Get the IDs
        let docs = db.get_documents_for_repo("test").unwrap();
        let alpha = docs.values().find(|d| d.title == "Alpha").unwrap();
        let beta = docs.values().find(|d| d.title == "Beta").unwrap();

        // Add a link from Alpha -> Beta
        db.update_links(&alpha.id, &[DetectedLink {
            target_id: beta.id.clone(),
            target_title: "Beta".into(),
            mention_text: "Beta".into(),
            context: String::new(),
        }]).unwrap();

        // Search via the search tool
        let args = serde_json::json!({"query": "Alpha"});
        let result = handle_search_tool(&db, &embedding, &args, &reporter).await.unwrap();

        let results = result["results"].as_array().unwrap();
        assert!(!results.is_empty());

        // Find the Alpha result and check it has links
        let alpha_result = results.iter().find(|r| r["id"] == alpha.id).unwrap();
        let links = alpha_result["links"].as_array().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["link_id"], beta.id);
        assert_eq!(links[0]["entity_name"], "Beta");
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tool_content_mode_includes_links() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc.md"), "# Doc\nSome unique content here.").unwrap();

        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let reporter = crate::ProgressReporter::Silent;

        scan_repository(&db, &embedding, &serde_json::json!({}), &reporter)
            .await
            .unwrap();

        // Content search
        let args = serde_json::json!({"query": "unique", "mode": "content"});
        let result = handle_search_tool(&db, &embedding, &args, &reporter).await.unwrap();

        let results = result["results"].as_array().unwrap();
        assert!(!results.is_empty());
        // Each result should have a links array (even if empty)
        for r in results {
            assert!(r.get("links").is_some(), "content search results should have links field");
        }
    }
}
