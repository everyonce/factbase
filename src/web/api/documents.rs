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
}
