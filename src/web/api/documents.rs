//! Document context API endpoints (read-only).
//!
//! Wraps existing MCP entity tools for the web UI.

use crate::services;
use crate::services::entity::GetEntityParams;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use super::super::server::WebAppState;
use super::errors::ApiError;

/// Query parameters for document retrieval.
#[derive(Debug, Deserialize)]
pub struct DocumentQuery {
    /// Generate 500-char preview
    pub include_preview: Option<bool>,
    /// Truncate content at word boundary (0 = no limit)
    pub max_content_length: Option<u64>,
}

/// GET /api/documents/:id - Get document with content.
///
/// Query params:
/// - `include_preview`: Generate 500-char preview (default: false)
/// - `max_content_length`: Truncate content at word boundary (0 = no limit)
pub async fn get_document(
    State(state): State<Arc<WebAppState>>,
    Path(id): Path<String>,
    Query(query): Query<DocumentQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let params = GetEntityParams {
        id,
        include_preview: query.include_preview.unwrap_or(false),
        max_content_length: query.max_content_length.unwrap_or(0) as usize,
        ..Default::default()
    };

    let result = super::run_blocking_web(move || services::get_entity(&db, &params)).await?;
    Ok(Json(result))
}

/// GET /api/documents/:id/links - Get document links.
///
/// Returns incoming and outgoing links for a document.
pub async fn get_document_links(
    State(state): State<Arc<WebAppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let params = GetEntityParams {
        id,
        ..Default::default()
    };

    let result = super::run_blocking_web(move || services::get_entity(&db, &params)).await?;

    // Extract just the link fields
    let links = serde_json::json!({
        "id": result.get("id"),
        "title": result.get("title"),
        "links_to": result.get("links_to").unwrap_or(&Value::Array(vec![])),
        "linked_from": result.get("linked_from").unwrap_or(&Value::Array(vec![]))
    });

    Ok(Json(links))
}

/// Query parameters for document preview.
#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    /// Target line number (1-based)
    pub line: Option<u64>,
    /// Number of context lines on each side (default: 10)
    pub context: Option<u64>,
}

/// GET /api/documents/:id/preview - Get windowed document content around a line.
///
/// Query params:
/// - `line`: Target line number (1-based). If omitted, returns first `context*2` lines.
/// - `context`: Lines of context on each side (default: 10)
pub async fn get_document_preview(
    State(state): State<Arc<WebAppState>>,
    Path(id): Path<String>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let target_line = query.line.unwrap_or(0) as usize;
    let context = query.context.unwrap_or(10) as usize;

    let result = super::run_blocking_web(move || {
        let doc = db.require_document(&id)?;
        let lines: Vec<&str> = doc.content.lines().collect();
        let total = lines.len();

        let (start, end) = if target_line == 0 {
            (0, (context * 2).min(total))
        } else {
            let idx = target_line.saturating_sub(1); // 0-based
            let start = idx.saturating_sub(context);
            let end = (idx + context + 1).min(total);
            (start, end)
        };

        let window: Vec<Value> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let line_num = start + i + 1;
                serde_json::json!({
                    "line": line_num,
                    "content": line,
                    "highlighted": target_line > 0 && line_num == target_line,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "doc_id": doc.id,
            "doc_title": doc.title,
            "file_path": doc.file_path,
            "target_line": if target_line > 0 { Value::Number(target_line.into()) } else { Value::Null },
            "start_line": start + 1,
            "end_line": end,
            "total_lines": total,
            "lines": window,
        }))
    })
    .await?;
    Ok(Json(result))
}

/// GET /api/repos - List all repositories.
pub async fn list_repos(
    State(state): State<Arc<WebAppState>>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let result = super::run_blocking_web(move || services::list_repositories(&db)).await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_query_deserialize_empty() {
        let json = r#"{}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert!(query.include_preview.is_none());
        assert!(query.max_content_length.is_none());
    }

    #[test]
    fn test_document_query_deserialize_full() {
        let json = r#"{"include_preview": true, "max_content_length": 1000}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_preview, Some(true));
        assert_eq!(query.max_content_length, Some(1000));
    }

    #[test]
    fn test_document_query_deserialize_partial() {
        let json = r#"{"include_preview": false}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_preview, Some(false));
        assert!(query.max_content_length.is_none());
    }

    #[test]
    fn test_document_query_zero_max_length() {
        let json = r#"{"max_content_length": 0}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.max_content_length, Some(0));
    }

    #[test]
    fn test_document_query_large_max_length() {
        let json = r#"{"max_content_length": 1000000}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.max_content_length, Some(1000000));
    }

    #[test]
    fn test_document_query_only_max_length() {
        let json = r#"{"max_content_length": 500}"#;
        let query: DocumentQuery = serde_json::from_str(json).unwrap();
        assert!(query.include_preview.is_none());
        assert_eq!(query.max_content_length, Some(500));
    }

    #[test]
    fn test_preview_query_deserialize_empty() {
        let json = r#"{}"#;
        let query: PreviewQuery = serde_json::from_str(json).unwrap();
        assert!(query.line.is_none());
        assert!(query.context.is_none());
    }

    #[test]
    fn test_preview_query_deserialize_full() {
        let json = r#"{"line": 27, "context": 5}"#;
        let query: PreviewQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.line, Some(27));
        assert_eq!(query.context, Some(5));
    }

    #[test]
    fn test_preview_query_line_only() {
        let json = r#"{"line": 1}"#;
        let query: PreviewQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.line, Some(1));
        assert!(query.context.is_none());
    }
}
