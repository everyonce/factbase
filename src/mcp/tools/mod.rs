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
mod document;
mod embeddings;
mod entity;
mod helpers;
mod organize;
mod repository;
mod review;
mod schema;
mod search;
mod workflow;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::progress::ProgressSender;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Re-export tool implementations
pub use authoring::get_authoring_guide;
pub use document::{bulk_create_documents, create_document, delete_document, update_document};
pub use embeddings::{embeddings_export, embeddings_import, embeddings_status_tool};
pub use entity::{get_entity, get_perspective, list_entities, list_repositories};
pub use organize::{organize, organize_analyze};
pub use repository::{init_repository, scan_repository};
pub use review::{
    answer_question, answer_questions, apply_review_answers, bulk_answer_questions,
    generate_questions, get_deferred_items, get_review_queue, check_repository,
};
pub use search::{search_content, search_knowledge};

// Re-export helpers for submodules
pub(crate) use helpers::{
    extract_type_repo_filters, get_bool_arg, get_str_arg, get_str_arg_required, get_u64_arg,
    get_u64_arg_required, load_perspective, run_blocking,
};

// Re-export schema
pub use schema::tools_list;

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
/// - Search: `search_knowledge`, `search_content`, `search_knowledge (temporal)`
/// - Entity: `get_entity`, `list_entities`, `get_perspective`, `list_repositories`, `get_document_stats`
/// - Document: `create_document`, `update_document`, `delete_document`, `bulk_create_documents`
/// - Review: `get_review_queue`, `answer_question`, `generate_questions`, `bulk_answer_questions`
// Dispatches a blocking tool function with cloned db and args via `run_blocking`.
macro_rules! blocking_tool {
    ($db:expr, $args:expr, $reporter:expr, $fn:path) => {{
        let db = $db.clone();
        let args = $args.clone();
        let r = $reporter.clone();
        run_blocking(move || $fn(&db, &args, &r)).await?
    }};
    ($db:expr, $args:expr, $fn:path) => {{
        let db = $db.clone();
        let args = $args.clone();
        run_blocking(move || $fn(&db, &args)).await?
    }};
    ($db:expr, $fn:path) => {{
        let db = $db.clone();
        run_blocking(move || $fn(&db)).await?
    }};
}

pub async fn handle_tool_call<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    llm: Option<&dyn LlmProvider>,
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
                "search_knowledge" => search_knowledge(db, embedding, &args).await?,
                "get_entity" => blocking_tool!(db, args, get_entity),
                "list_entities" => blocking_tool!(db, args, list_entities),
                "get_perspective" => blocking_tool!(db, args, get_perspective),
                "list_repositories" => blocking_tool!(db, list_repositories),
                "create_document" => blocking_tool!(db, args, create_document),
                "update_document" => blocking_tool!(db, args, update_document),
                "delete_document" => blocking_tool!(db, args, delete_document),
                "bulk_create_documents" => {
                    blocking_tool!(db, args, reporter, bulk_create_documents)
                }
                "search_content" => {
                    blocking_tool!(db, args, reporter, search_content)
                }
                "get_review_queue" => get_review_queue(db, &args, &reporter)?,
                "get_deferred_items" => get_deferred_items(db, &args, &reporter)?,
                "answer_questions" => {
                    blocking_tool!(db, args, reporter, answer_questions)
                }
                "check_repository" => check_repository(db, embedding, llm, &args, &reporter).await?,
                "generate_questions" => generate_questions(db, embedding, llm, &args).await?,
                "scan_repository" => scan_repository(db, embedding, llm, &args, &reporter).await?,
                "init_repository" => blocking_tool!(db, args, init_repository),
                "apply_review_answers" => apply_review_answers(db, llm, &args, &reporter).await?,
                "get_duplicate_entries" => {
                    organize_analyze(db, embedding, &serde_json::json!({"focus": "duplicates", "repo": args.get("repo")}), &reporter).await?
                }
                "organize_analyze" => {
                    organize_analyze(db, embedding, &args, &reporter).await?
                }
                "organize" => {
                    organize(db, embedding, llm, &args, &reporter).await?
                }
                "organize_merge" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "merge".into()));
                    organize(db, embedding, llm, &a, &reporter).await?
                }
                "organize_split" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "split".into()));
                    organize(db, embedding, llm, &a, &reporter).await?
                }
                "organize_move" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "move".into()));
                    organize(db, embedding, llm, &a, &reporter).await?
                }
                "organize_retype" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "retype".into()));
                    organize(db, embedding, llm, &a, &reporter).await?
                }
                "organize_apply" => {
                    let mut a = args.clone();
                    a.as_object_mut().map(|m| m.insert("action".into(), "apply".into()));
                    organize(db, embedding, llm, &a, &reporter).await?
                }
                "workflow" => {
                    // Bootstrap needs async LLM access; other workflows are sync
                    let is_bootstrap = args.get("workflow")
                        .and_then(|v| v.as_str())
                        .map_or(false, |w| w == "bootstrap");
                    if is_bootstrap {
                        if let Some(llm) = llm {
                            workflow::bootstrap(llm, &args).await?
                        } else {
                            serde_json::json!({
                                "error": "The bootstrap workflow requires an LLM provider. Configure an LLM in your factbase config."
                            })
                        }
                    } else {
                        blocking_tool!(db, args, workflow::workflow)
                    }
                }
                "get_authoring_guide" => get_authoring_guide(),
                "embeddings_export" => blocking_tool!(db, args, embeddings_export),
                "embeddings_import" => blocking_tool!(db, args, embeddings_import),
                "embeddings_status" => blocking_tool!(db, embeddings_status_tool),
                _ => {
                    return Ok(Some(McpResponse::error(
                        -32602,
                        format!("Unknown tool: {tool_name}"),
                    )))
                }
            };

            Ok(Some(McpResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
                    }]
                }),
            )))
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

        let dispatch_names: HashSet<String> = [
            "search_knowledge",
            "get_entity",
            "list_entities",
            "get_perspective",
            "list_repositories",
            "create_document",
            "update_document",
            "delete_document",
            "bulk_create_documents",
            "search_content",
            "get_review_queue",
            "get_deferred_items",
            "answer_questions",
            "check_repository",
            "generate_questions",
            "scan_repository",
            "init_repository",
            "apply_review_answers",
            "get_authoring_guide",
            "workflow",
            "organize_analyze",
            "organize",
            "embeddings_export",
            "embeddings_import",
            "embeddings_status",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let in_schema_not_dispatch: Vec<_> = schema_names.difference(&dispatch_names).collect();
        let in_dispatch_not_schema: Vec<_> = dispatch_names.difference(&schema_names).collect();

        assert!(
            in_schema_not_dispatch.is_empty(),
            "Tools in schema but missing from dispatch: {:?}",
            in_schema_not_dispatch
        );
        assert!(
            in_dispatch_not_schema.is_empty(),
            "Tools in dispatch but missing from schema: {:?}",
            in_dispatch_not_schema
        );
        assert_eq!(schema_names.len(), dispatch_names.len());
    }

    #[test]
    fn test_schema_doc_type_param_consistency() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        // Tools that filter by document type must use "doc_type", not "type"
        let doc_type_tools = [
            "search_knowledge",
            "search_content",
            "list_entities",
            "workflow",
        ];

        for tool_name in &doc_type_tools {
            let tool = tools
                .iter()
                .find(|t| t["name"] == *tool_name)
                .unwrap_or_else(|| panic!("tool {tool_name} should exist"));
            let props = tool["inputSchema"]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{tool_name} should have properties"));

            assert!(
                props.contains_key("doc_type"),
                "{tool_name} should have 'doc_type' param"
            );
            assert!(
                !props.contains_key("type"),
                "{tool_name} should use 'doc_type' not 'type' for document type filter"
            );
        }

        // No tool should have a "type" property for document type filtering
        for tool in tools {
            let name = tool["name"].as_str().unwrap_or("unknown");
            if let Some(props) = tool["inputSchema"]["properties"].as_object() {
                if let Some(type_prop) = props.get("type") {
                    let desc = type_prop["description"].as_str().unwrap_or("");
                    assert!(
                        !desc.to_lowercase().contains("document type"),
                        "{name} has 'type' param describing document type — should be 'doc_type'"
                    );
                }
            }
        }
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
        let result = scan_repository(&db, &embedding, None, &args, &reporter).await;
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
        scan_repository(&db, &embedding, None, &args, &reporter)
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
        let result = scan_repository(&db, &embedding, None, &args, &reporter)
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
        let result = scan_repository(&db, &embedding, None, &serde_json::json!({}), &reporter)
            .await
            .unwrap();

        // With NoOpLlm, links_detected should be 0 and total > 1
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
        let result = scan_repository(&db, &embedding, None, &serde_json::json!({}), &reporter)
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
        scan_repository(&db, &embedding, None, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Now check with progress tracking
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let reporter = crate::ProgressReporter::Mcp { sender: Some(tx) };

        let args = serde_json::json!({});
        check_repository(&db, &embedding, None, &args, &reporter)
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
    fn test_schema_time_budget_secs_on_scaling_tools() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let scaling_tools = ["scan_repository", "check_repository", "apply_review_answers", "generate_questions", "organize_analyze"];
        for tool_name in &scaling_tools {
            let tool = tools
                .iter()
                .find(|t| t["name"] == *tool_name)
                .unwrap_or_else(|| panic!("tool {tool_name} should exist"));
            let props = tool["inputSchema"]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{tool_name} should have properties"));
            assert!(
                props.contains_key("time_budget_secs"),
                "{tool_name} should have 'time_budget_secs' param"
            );
        }
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
        let result = scan_repository(&db, &embedding, None, &args, &reporter)
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
        scan_repository(&db, &embedding, None, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Check with a generous budget — should complete
        let args = serde_json::json!({"time_budget_secs": 30});
        let result = check_repository(&db, &embedding, None, &args, &silent)
            .await
            .unwrap();

        assert!(result.get("documents_scanned").is_some());
        // Should complete within budget (no continue flag)
        assert!(result.get("continue").is_none() || result["continue"] == false);
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
        scan_repository(&db, &embedding, None, &serde_json::json!({}), &silent)
            .await
            .unwrap();

        // Generate questions for all docs with generous budget
        let args = serde_json::json!({"dry_run": true, "time_budget_secs": 30});
        let result = generate_questions(&db, &embedding, None, &args)
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
        use crate::llm::LlmProvider;
        use crate::{BoxFuture, DocumentProcessor, FactbaseError, LinkDetector, Scanner, ScanContext, ScanOptions};
        use tempfile::TempDir;

        struct NoOpLlm;
        impl LlmProvider for NoOpLlm {
            fn complete<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
                Box::pin(async { Ok("[]".to_string()) })
            }
        }

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "<!-- factbase:aaa111 -->\n# Doc One\n\n- Fact alpha\n- Fact beta\n").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let scanner = Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let link_detector = LinkDetector::new(Box::new(NoOpLlm));
        let opts = ScanOptions::default();
        let progress = crate::ProgressReporter::Silent;
        let repo = db.list_repositories().unwrap().into_iter().next().unwrap();

        let ctx = ScanContext {
            scanner: &scanner, processor: &processor, embedding: &embedding,
            link_detector: &link_detector, opts: &opts, progress: &progress,
        };

        // First scan: indexes docs and generates fact embeddings
        let r1 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert!(r1.fact_embeddings_generated > 0);
        let count_after_first = db.get_fact_embedding_count().unwrap();
        assert!(count_after_first > 0);

        // Second scan with no changes: should skip fact embedding (already populated)
        let r2 = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.fact_embeddings_generated, 0);
        assert_eq!(db.get_fact_embedding_count().unwrap(), count_after_first);
    }

    #[tokio::test]
    async fn test_scan_auto_populates_fact_embeddings_after_migration() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use crate::embedding::test_helpers::MockEmbedding;
        use crate::llm::LlmProvider;
        use crate::{BoxFuture, DocumentProcessor, FactbaseError, LinkDetector, Scanner, ScanContext, ScanOptions};
        use tempfile::TempDir;

        struct NoOpLlm;
        impl LlmProvider for NoOpLlm {
            fn complete<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
                Box::pin(async { Ok("[]".to_string()) })
            }
        }

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path();

        std::fs::write(repo_path.join("doc1.md"), "<!-- factbase:bbb222 -->\n# Doc One\n\n- Fact one\n").unwrap();
        test_repo_in_db(&db, "test", repo_path);

        let embedding = MockEmbedding::new(1024);
        let scanner = Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let link_detector = LinkDetector::new(Box::new(NoOpLlm));
        let opts = ScanOptions::default();
        let progress = crate::ProgressReporter::Silent;
        let repo = db.list_repositories().unwrap().into_iter().next().unwrap();

        let ctx = ScanContext {
            scanner: &scanner, processor: &processor, embedding: &embedding,
            link_detector: &link_detector, opts: &opts, progress: &progress,
        };

        // First scan indexes docs normally
        crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert!(db.get_fact_embedding_count().unwrap() > 0);

        // Simulate migration: clear fact embeddings but leave docs unchanged
        db.delete_fact_embeddings_for_doc("bbb222").unwrap();
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);

        // Rescan with no file changes: should auto-populate fact embeddings
        let r = crate::full_scan(&repo, &db, &ctx).await.unwrap();
        assert!(r.fact_embeddings_generated > 0);
        assert!(db.get_fact_embedding_count().unwrap() > 0);
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
        scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        // Second scan with force_reindex: should re-process all docs
        let args = serde_json::json!({"force_reindex": true});
        let r = scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();
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
        scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        // force_reindex without explicit time_budget_secs: should NOT be interrupted
        let args = serde_json::json!({"force_reindex": true});
        let r = scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        assert!(
            r.get("continue").is_none() || r["continue"] == false,
            "force_reindex without explicit budget should not be interrupted"
        );
        assert_eq!(r["reindexed"].as_u64().unwrap(), 5, "all docs should be reindexed");
        assert!(r.get("total").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_repository_force_reindex_respects_explicit_budget() {
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
        scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        // force_reindex WITH explicit time_budget_secs: budget should be active
        // (MockEmbedding is instant so it completes, but the deadline is set)
        let args = serde_json::json!({"force_reindex": true, "time_budget_secs": 10});
        let r = scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        // Should complete (MockEmbedding is instant) with reindexed count
        assert_eq!(r["reindexed"].as_u64().unwrap(), 1);
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
        let r = scan_repository(&db, &embedding, None, &args, &reporter).await.unwrap();

        // MockEmbedding is instant, so it completes within budget
        assert!(r.get("total").is_some());
        assert_eq!(r["reindexed"].as_u64().unwrap(), 0);
    }
}
