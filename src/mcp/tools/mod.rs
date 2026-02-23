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
pub use entity::{get_entity, get_perspective, list_entities, list_repositories};
pub use organize::get_duplicate_entries;
pub use repository::{init_repository, scan_repository};
pub use review::{
    answer_question, answer_questions, apply_review_answers, bulk_answer_questions,
    generate_questions, get_review_queue, lint_repository,
};
pub use search::{search_content, search_knowledge};

// Re-export helpers for submodules
pub(crate) use helpers::{
    extract_type_repo_filters, get_bool_arg, get_str_arg, get_str_arg_required, get_u64_arg,
    get_u64_arg_required, run_blocking,
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
                "answer_questions" => {
                    blocking_tool!(db, args, reporter, answer_questions)
                }
                "lint_repository" => lint_repository(db, embedding, llm, &args, &reporter).await?,
                "scan_repository" => scan_repository(db, embedding, llm, &args, &reporter).await?,
                "init_repository" => blocking_tool!(db, args, init_repository),
                "apply_review_answers" => apply_review_answers(db, llm, &args, &reporter).await?,
                "get_duplicate_entries" => {
                    get_duplicate_entries(db, embedding, &args, &reporter).await?
                }
                "workflow" => blocking_tool!(db, args, workflow::workflow),
                "get_authoring_guide" => get_authoring_guide(),
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
            "answer_questions",
            "lint_repository",
            "scan_repository",
            "init_repository",
            "apply_review_answers",
            "get_duplicate_entries",
            "get_authoring_guide",
            "workflow",
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
}
