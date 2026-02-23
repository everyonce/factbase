use crate::database::Database;
use crate::embedding::{EmbeddingProvider, OllamaEmbedding};
use crate::error::FactbaseError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: McpParams,
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

pub async fn handle_tool_call(
    db: &Database,
    embedding: &OllamaEmbedding,
    request: McpRequest,
) -> Result<McpResponse, FactbaseError> {
    if request.method != "tools/call" {
        return Ok(McpResponse::error(-32601, "Method not found".into()));
    }

    let tool_name = request.params.name.as_deref().unwrap_or("");
    let args = &request.params.arguments;

    let result = match tool_name {
        "search_knowledge" => search_knowledge(db, embedding, args).await?,
        "get_entity" => get_entity(db, args)?,
        "list_entities" => list_entities(db, args)?,
        "get_perspective" => get_perspective(db, args)?,
        _ => {
            return Ok(McpResponse::error(
                -32602,
                format!("Unknown tool: {}", tool_name),
            ))
        }
    };

    Ok(McpResponse::success(request.id, result))
}

async fn search_knowledge(
    db: &Database,
    embedding: &OllamaEmbedding,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FactbaseError::Parse("Missing query parameter".into()))?;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let doc_type = args.get("type").and_then(|v| v.as_str());
    let repo = args.get("repo").and_then(|v| v.as_str());

    let query_embedding = embedding.generate(query).await?;
    let results = db.search_semantic(&query_embedding, limit, doc_type, repo)?;

    let items: Vec<Value> = results
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "title": r.title,
                "type": r.doc_type,
                "file_path": r.file_path,
                "relevance_score": r.relevance_score,
                "snippet": r.snippet
            })
        })
        .collect();

    Ok(serde_json::json!({ "results": items }))
}

fn get_entity(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FactbaseError::Parse("Missing id parameter".into()))?;

    let doc = db
        .get_document(id)?
        .ok_or_else(|| FactbaseError::NotFound(format!("Entity not found: {}", id)))?;

    let links_to = db.get_links_from(id)?;
    let linked_from = db.get_links_to(id)?;

    Ok(serde_json::json!({
        "id": doc.id,
        "title": doc.title,
        "type": doc.doc_type,
        "file_path": doc.file_path,
        "content": doc.content,
        "links_to": links_to,
        "linked_from": linked_from,
        "indexed_at": doc.indexed_at.to_rfc3339()
    }))
}

fn list_entities(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_type = args.get("type").and_then(|v| v.as_str());
    let repo = args.get("repo").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let docs = db.list_documents(doc_type, repo, limit)?;

    let items: Vec<Value> = docs
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "title": d.title,
                "type": d.doc_type,
                "file_path": d.file_path
            })
        })
        .collect();

    Ok(serde_json::json!({ "entities": items }))
}

fn get_perspective(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = args.get("repo").and_then(|v| v.as_str());

    let repos = db.list_repositories()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };

    let repo = repo.ok_or_else(|| FactbaseError::NotFound("No repository found".into()))?;

    Ok(serde_json::json!({
        "repo_id": repo.id,
        "repo_name": repo.name,
        "perspective": repo.perspective
    }))
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
}
