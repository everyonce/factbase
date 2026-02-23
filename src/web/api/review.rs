//! Review queue API endpoints.
//!
//! Wraps existing MCP review tools for the web UI.

use crate::mcp::tools::{answer_question, bulk_answer_questions, get_review_queue};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::super::server::WebAppState;
use super::errors::ApiError;

/// Query parameters for review queue listing.
#[derive(Debug, Deserialize)]
pub struct ReviewQueueQuery {
    /// Filter by repository ID
    pub repo: Option<String>,
    /// Filter by question type (temporal, conflict, missing, ambiguous, stale, duplicate)
    #[serde(rename = "type")]
    pub question_type: Option<String>,
}

/// Request body for answering a single question.
#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    /// Zero-based index of question in review queue
    pub question_index: u64,
    /// Answer text
    pub answer: String,
}

/// Request body for bulk answering questions.
#[derive(Debug, Deserialize)]
pub struct BulkAnswerRequest {
    /// Array of answers
    pub answers: Vec<BulkAnswerItem>,
}

/// Single answer in bulk request.
#[derive(Debug, Deserialize, Serialize)]
pub struct BulkAnswerItem {
    /// Document ID
    pub doc_id: String,
    /// Zero-based index of question
    pub question_index: u64,
    /// Answer text
    pub answer: String,
}

/// GET /api/review/queue - List pending review questions.
///
/// Query params:
/// - `repo`: Filter by repository ID
/// - `type`: Filter by question type
pub async fn list_review_queue(
    State(state): State<Arc<WebAppState>>,
    Query(query): Query<ReviewQueueQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    // Build args for MCP function
    let mut args = serde_json::Map::new();
    if let Some(repo) = query.repo {
        args.insert("repo".to_string(), Value::String(repo));
    }
    if let Some(qtype) = query.question_type {
        args.insert("type".to_string(), Value::String(qtype));
    }

    let result =
        super::run_blocking_web(move || get_review_queue(&db, &Value::Object(args))).await?;
    Ok(Json(result))
}

/// GET /api/review/queue/:doc_id - Get questions for specific document.
pub async fn get_document_questions(
    State(state): State<Arc<WebAppState>>,
    Path(doc_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    // Build args with doc_id filter
    let args = serde_json::json!({ "doc_id": doc_id });

    let result = super::run_blocking_web(move || get_review_queue(&db, &args)).await?;
    Ok(Json(result))
}

/// POST /api/review/answer/:doc_id - Answer a single question.
pub async fn post_answer(
    State(state): State<Arc<WebAppState>>,
    Path(doc_id): Path<String>,
    Json(body): Json<AnswerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let args = serde_json::json!({
        "doc_id": doc_id,
        "question_index": body.question_index,
        "answer": body.answer
    });

    let result = super::run_blocking_web(move || answer_question(&db, &args)).await?;
    Ok(Json(result))
}

/// POST /api/review/bulk-answer - Answer multiple questions.
pub async fn post_bulk_answer(
    State(state): State<Arc<WebAppState>>,
    Json(body): Json<BulkAnswerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    // Convert to format expected by MCP function
    let answers: Vec<Value> = body
        .answers
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "doc_id": a.doc_id,
                "question_index": a.question_index,
                "answer": a.answer
            })
        })
        .collect();

    let args = serde_json::json!({ "answers": answers });

    let result = super::run_blocking_web(move || bulk_answer_questions(&db, &args)).await?;
    Ok(Json(result))
}

/// GET /api/review/status - Get review queue summary.
pub async fn get_review_status(
    State(state): State<Arc<WebAppState>>,
    Query(query): Query<ReviewQueueQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    // Build args for MCP function - reuse get_review_queue which returns counts
    let mut args = serde_json::Map::new();
    if let Some(repo) = query.repo {
        args.insert("repo".to_string(), Value::String(repo));
    }
    if let Some(qtype) = query.question_type {
        args.insert("type".to_string(), Value::String(qtype));
    }

    let result =
        super::run_blocking_web(move || get_review_queue(&db, &Value::Object(args))).await?;

    // Extract just the summary fields
    let status = serde_json::json!({
        "total": result.get("total").unwrap_or(&Value::Null),
        "answered": result.get("answered").unwrap_or(&Value::Null),
        "unanswered": result.get("unanswered").unwrap_or(&Value::Null)
    });

    Ok(Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_queue_query_deserialize() {
        let json = r#"{"repo": "main", "type": "temporal"}"#;
        let query: ReviewQueueQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.repo, Some("main".to_string()));
        assert_eq!(query.question_type, Some("temporal".to_string()));
    }

    #[test]
    fn test_review_queue_query_deserialize_empty() {
        let json = r#"{}"#;
        let query: ReviewQueueQuery = serde_json::from_str(json).unwrap();
        assert!(query.repo.is_none());
        assert!(query.question_type.is_none());
    }

    #[test]
    fn test_answer_request_deserialize() {
        let json = r#"{"question_index": 0, "answer": "Started 2020"}"#;
        let req: AnswerRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.question_index, 0);
        assert_eq!(req.answer, "Started 2020");
    }

    #[test]
    fn test_bulk_answer_request_deserialize() {
        let json = r#"{
            "answers": [
                {"doc_id": "abc123", "question_index": 0, "answer": "Answer 1"},
                {"doc_id": "def456", "question_index": 1, "answer": "Answer 2"}
            ]
        }"#;
        let req: BulkAnswerRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.answers.len(), 2);
        assert_eq!(req.answers[0].doc_id, "abc123");
        assert_eq!(req.answers[1].question_index, 1);
    }

    // Edge cases for request parsing
    #[test]
    fn test_answer_request_empty_answer() {
        let json = r#"{"question_index": 0, "answer": ""}"#;
        let req: AnswerRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.answer, "");
    }

    #[test]
    fn test_answer_request_large_index() {
        let json = r#"{"question_index": 999999, "answer": "test"}"#;
        let req: AnswerRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.question_index, 999999);
    }

    #[test]
    fn test_bulk_answer_request_empty_array() {
        let json = r#"{"answers": []}"#;
        let req: BulkAnswerRequest = serde_json::from_str(json).unwrap();
        assert!(req.answers.is_empty());
    }

    #[test]
    fn test_bulk_answer_item_serialize_roundtrip() {
        let item = BulkAnswerItem {
            doc_id: "abc123".to_string(),
            question_index: 5,
            answer: "My answer".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let parsed: BulkAnswerItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.doc_id, item.doc_id);
        assert_eq!(parsed.question_index, item.question_index);
        assert_eq!(parsed.answer, item.answer);
    }

    #[test]
    fn test_review_queue_query_all_types() {
        // Test all valid question types
        for qtype in [
            "temporal",
            "conflict",
            "missing",
            "ambiguous",
            "stale",
            "duplicate",
        ] {
            let json = format!(r#"{{"type": "{}"}}"#, qtype);
            let query: ReviewQueueQuery = serde_json::from_str(&json).unwrap();
            assert_eq!(query.question_type, Some(qtype.to_string()));
        }
    }
}
