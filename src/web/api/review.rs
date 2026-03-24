//! Review queue API endpoints.
//!
//! Wraps existing MCP review tools for the web UI.

use crate::services::{self, AnswerQuestionParams, ReviewQueueParams, ServiceBulkAnswerItem};
use crate::ProgressReporter;
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
    /// Filter by question type (temporal, conflict, missing, ambiguous, stale, duplicate, corruption, precision, weak-source)
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

    let params = ReviewQueueParams {
        repo: query.repo,
        question_type: query.question_type,
        limit: 10,
        ..Default::default()
    };

    let result = super::run_blocking_web(move || {
        services::get_review_queue(&db, &params, &ProgressReporter::Silent)
    })
    .await?;
    Ok(Json(group_questions_by_doc(&result)))
}

/// GET /api/review/queue/:doc_id - Get questions for specific document.
pub async fn get_document_questions(
    State(state): State<Arc<WebAppState>>,
    Path(doc_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let params = ReviewQueueParams {
        doc_id: Some(doc_id.clone()),
        limit: 10,
        ..Default::default()
    };

    let result = super::run_blocking_web(move || {
        services::get_review_queue(&db, &params, &ProgressReporter::Silent)
    })
    .await?;
    let grouped = group_questions_by_doc(&result);
    // Return the single DocumentReview for this doc, or an empty one
    let doc = grouped["documents"]
        .as_array()
        .and_then(|docs| docs.first().cloned())
        .unwrap_or_else(|| {
            serde_json::json!({
                "doc_id": doc_id,
                "doc_title": "",
                "file_path": "",
                "questions": [],
            })
        });
    Ok(Json(doc))
}

/// POST /api/review/answer/:doc_id - Answer a single question.
pub async fn post_answer(
    State(state): State<Arc<WebAppState>>,
    Path(doc_id): Path<String>,
    Json(body): Json<AnswerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let params = AnswerQuestionParams {
        doc_id,
        question_index: body.question_index as usize,
        answer: body.answer,
        confidence: None,
        agent_suggestion: None,
    };

    let result = super::run_blocking_web(move || services::answer_question(&db, &params)).await?;
    Ok(Json(result))
}

/// POST /api/review/bulk-answer - Answer multiple questions.
pub async fn post_bulk_answer(
    State(state): State<Arc<WebAppState>>,
    Json(body): Json<BulkAnswerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let items: Vec<ServiceBulkAnswerItem> = body
        .answers
        .into_iter()
        .map(|a| ServiceBulkAnswerItem {
            doc_id: a.doc_id,
            question_index: a.question_index as usize,
            answer: a.answer,
            confidence: None,
            agent_suggestion: None,
        })
        .collect();

    let result = super::run_blocking_web(move || {
        services::bulk_answer_questions(&db, &items, &crate::ProgressReporter::Silent)
    })
    .await?;
    Ok(Json(result))
}

/// GET /api/review/status - Get review queue summary.
pub async fn get_review_status(
    State(state): State<Arc<WebAppState>>,
    Query(query): Query<ReviewQueueQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let params = ReviewQueueParams {
        repo: query.repo,
        question_type: query.question_type,
        status: Some("all".to_string()),
        limit: 1000000,
        ..Default::default()
    };

    let result = super::run_blocking_web(move || {
        services::get_review_queue(&db, &params, &ProgressReporter::Silent)
    })
    .await?;

    // Extract just the summary fields
    let status = serde_json::json!({
        "total": result.get("total").unwrap_or(&Value::Null),
        "answered": result.get("answered").unwrap_or(&Value::Null),
        "unanswered": result.get("unanswered").unwrap_or(&Value::Null),
        "deferred": result.get("deferred").unwrap_or(&Value::Null)
    });

    Ok(Json(status))
}

/// Request body for applying review answers.
#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    /// Filter by repository ID
    pub repo: Option<String>,
    /// Filter by document ID
    pub doc_id: Option<String>,
    /// Preview changes without writing
    pub dry_run: Option<bool>,
}

/// POST /api/apply - Apply answered review questions to documents.
pub async fn post_apply(
    State(state): State<Arc<WebAppState>>,
    Json(body): Json<ApplyRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    use crate::answer_processor::apply_all::{apply_all_review_answers, ApplyConfig, ApplyStatus};

    let db = state.db.clone();
    let config = ApplyConfig {
        doc_id_filter: body.doc_id.as_deref(),
        repo_filter: body.repo.as_deref(),
        dry_run: body.dry_run.unwrap_or(false),
        since: None,
        deadline: None,
        acquire_write_guard: false,
    };

    let result = apply_all_review_answers(&db, &config, &crate::ProgressReporter::Silent)
        .await
        .map_err(super::errors::handle_error)?;

    let documents: Vec<Value> = result
        .documents
        .iter()
        .map(|d| {
            serde_json::json!({
                "doc_id": d.doc_id,
                "doc_title": d.doc_title,
                "questions_applied": d.questions_applied,
                "status": match d.status {
                    ApplyStatus::Applied => "applied",
                    ApplyStatus::DryRun => "dry_run",
                    ApplyStatus::Error => "error",
                },
                "error": d.error,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "status": "ok",
        "total_applied": result.total_applied,
        "total_errors": result.total_errors,
        "documents": documents,
    })))
}

/// POST /api/scan - Trigger repository scan.
///
/// Scan requires embedding provider and full scanner infrastructure.
/// Returns CLI instructions.
pub async fn post_scan(
    Json(body): Json<ScanCheckRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let mut cmd = "factbase scan".to_string();
    if let Some(ref repo) = body.repo {
        cmd.push_str(&format!(" --repo {repo}"));
    }
    Ok(Json(serde_json::json!({
        "status": "cli_required",
        "message": format!("Scan requires embedding provider. Run via CLI: `{cmd}`"),
        "command": cmd,
    })))
}

/// POST /api/check - Trigger quality checks.
///
/// Check requires embedding provider for question generation.
/// Returns CLI instructions.
pub async fn post_check(
    Json(body): Json<ScanCheckRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let mut cmd = "factbase check".to_string();
    if let Some(ref repo) = body.repo {
        cmd.push_str(&format!(" --repo {repo}"));
    }
    Ok(Json(serde_json::json!({
        "status": "cli_required",
        "message": format!("Check requires embedding provider. Run via CLI: `{cmd}`"),
        "command": cmd,
    })))
}

/// Request body for scan/check endpoints.
#[derive(Debug, Deserialize)]
pub struct ScanCheckRequest {
    pub repo: Option<String>,
}

/// Request body for bulk-approving agent-pre-filled answers.
#[derive(Debug, Deserialize)]
pub struct ApproveBulkRequest {
    /// DB row IDs of questions to approve (uses each question's agent_suggestion as the answer)
    pub question_ids: Vec<i64>,
    /// If true, immediately apply answers to documents
    #[serde(default)]
    pub apply: bool,
}

/// POST /api/approve-bulk - Approve agent-pre-filled answers for multiple questions.
///
/// Uses each question's stored `agent_suggestion` as the answer.
/// If `apply: true`, also runs apply_all to write changes to documents.
pub async fn post_approve_bulk(
    State(state): State<Arc<WebAppState>>,
    Json(body): Json<ApproveBulkRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    use crate::answer_processor::apply_all::{apply_all_review_answers, ApplyConfig};

    let db = state.db.clone();
    let question_ids = body.question_ids.clone();

    let approve_result = super::run_blocking_web(move || {
        services::bulk_approve_questions(&db, &question_ids, &crate::ProgressReporter::Silent)
    })
    .await?;

    let approved = approve_result["approved"].as_u64().unwrap_or(0) as usize;
    let errors = approve_result["errors"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let applied = if body.apply && approved > 0 {
        let db2 = state.db.clone();
        let config = ApplyConfig {
            doc_id_filter: None,
            repo_filter: None,
            dry_run: false,
            since: None,
            deadline: None,
            acquire_write_guard: false,
        };
        let apply_result = apply_all_review_answers(&db2, &config, &crate::ProgressReporter::Silent)
            .await
            .map_err(super::errors::handle_error)?;
        apply_result.total_applied
    } else {
        0
    };

    Ok(Json(serde_json::json!({
        "approved": approved,
        "applied": applied,
        "errors": errors,
    })))
}

/// Groups a flat `questions` array from the service layer into `documents: DocumentReview[]`
/// as expected by the frontend `ReviewQueueResponse` type.
fn group_questions_by_doc(result: &Value) -> Value {
    let questions = result["questions"].as_array().cloned().unwrap_or_default();

    let mut doc_order: Vec<String> = Vec::new();
    let mut doc_data: std::collections::HashMap<String, (String, String, Vec<Value>)> =
        std::collections::HashMap::new();

    for q in &questions {
        let doc_id = q["doc_id"].as_str().unwrap_or("").to_string();
        let question = serde_json::json!({
            "question_type": q["type"],
            "description": q["description"],
            "line_ref": q["line_ref"],
            "answered": q["answered"],
            "answer": q["answer"],
            "confidence": q["confidence"],
            "agent_suggestion": q["agent_suggestion"],
        });
        if !doc_data.contains_key(&doc_id) {
            doc_order.push(doc_id.clone());
            doc_data.insert(
                doc_id.clone(),
                (
                    q["doc_title"].as_str().unwrap_or("").to_string(),
                    q["file_path"].as_str().unwrap_or("").to_string(),
                    Vec::new(),
                ),
            );
        }
        doc_data.get_mut(&doc_id).unwrap().2.push(question);
    }

    let documents: Vec<Value> = doc_order
        .into_iter()
        .map(|doc_id| {
            let (doc_title, file_path, qs) = doc_data.remove(&doc_id).unwrap();
            serde_json::json!({
                "doc_id": doc_id,
                "doc_title": doc_title,
                "file_path": file_path,
                "questions": qs,
            })
        })
        .collect();

    serde_json::json!({
        "documents": documents,
        "total": result["total"],
        "answered": result["answered"],
        "unanswered": result["unanswered"],
    })
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
            "corruption",
        ] {
            let json = format!(r#"{{"type": "{}"}}"#, qtype);
            let query: ReviewQueueQuery = serde_json::from_str(&json).unwrap();
            assert_eq!(query.question_type, Some(qtype.to_string()));
        }
    }

    #[test]
    fn test_apply_request_deserialize() {
        let json = r#"{"repo": "main", "doc_id": "abc123", "dry_run": true}"#;
        let req: ApplyRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.repo, Some("main".to_string()));
        assert_eq!(req.doc_id, Some("abc123".to_string()));
        assert_eq!(req.dry_run, Some(true));
    }

    #[test]
    fn test_apply_request_deserialize_empty() {
        let json = r#"{}"#;
        let req: ApplyRequest = serde_json::from_str(json).unwrap();
        assert!(req.repo.is_none());
        assert!(req.doc_id.is_none());
        assert!(req.dry_run.is_none());
    }

    #[test]
    fn test_scan_check_request_deserialize() {
        let json = r#"{"repo": "main"}"#;
        let req: ScanCheckRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.repo, Some("main".to_string()));
    }

    #[test]
    fn test_scan_check_request_deserialize_empty() {
        let json = r#"{}"#;
        let req: ScanCheckRequest = serde_json::from_str(json).unwrap();
        assert!(req.repo.is_none());
    }

    #[test]
    fn test_approve_bulk_request_deserialize() {
        let json = r#"{"question_ids": [1, 2, 3], "apply": true}"#;
        let req: ApproveBulkRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.question_ids, vec![1, 2, 3]);
        assert!(req.apply);
    }

    #[test]
    fn test_approve_bulk_request_apply_defaults_false() {
        let json = r#"{"question_ids": [42]}"#;
        let req: ApproveBulkRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.question_ids, vec![42]);
        assert!(!req.apply);
    }

    #[test]
    fn test_approve_bulk_request_empty_ids() {
        let json = r#"{"question_ids": []}"#;
        let req: ApproveBulkRequest = serde_json::from_str(json).unwrap();
        assert!(req.question_ids.is_empty());
    }

    #[test]
    fn test_group_questions_by_doc_empty() {
        let result = serde_json::json!({
            "questions": [],
            "total": 0,
            "answered": 0,
            "unanswered": 0,
        });
        let grouped = group_questions_by_doc(&result);
        assert_eq!(grouped["documents"].as_array().unwrap().len(), 0);
        assert_eq!(grouped["total"], 0);
    }

    #[test]
    fn test_group_questions_by_doc_single_doc() {
        let result = serde_json::json!({
            "questions": [
                {
                    "doc_id": "abc123",
                    "doc_title": "My Doc",
                    "file_path": "/kb/my-doc.md",
                    "type": "temporal",
                    "description": "When did this happen?",
                    "line_ref": 5,
                    "answered": false,
                    "answer": null,
                    "confidence": "deferred",
                    "agent_suggestion": null,
                }
            ],
            "total": 1,
            "answered": 0,
            "unanswered": 1,
        });
        let grouped = group_questions_by_doc(&result);
        let docs = grouped["documents"].as_array().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["doc_id"], "abc123");
        assert_eq!(docs[0]["doc_title"], "My Doc");
        assert_eq!(docs[0]["file_path"], "/kb/my-doc.md");
        let qs = docs[0]["questions"].as_array().unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["question_type"], "temporal");
        assert_eq!(qs[0]["description"], "When did this happen?");
    }

    #[test]
    fn test_group_questions_by_doc_multiple_docs() {
        let result = serde_json::json!({
            "questions": [
                {
                    "doc_id": "aaa",
                    "doc_title": "Doc A",
                    "file_path": "/kb/a.md",
                    "type": "conflict",
                    "description": "Q1",
                    "line_ref": null,
                    "answered": false,
                    "answer": null,
                    "confidence": "deferred",
                    "agent_suggestion": null,
                },
                {
                    "doc_id": "bbb",
                    "doc_title": "Doc B",
                    "file_path": "/kb/b.md",
                    "type": "missing",
                    "description": "Q2",
                    "line_ref": null,
                    "answered": false,
                    "answer": null,
                    "confidence": "deferred",
                    "agent_suggestion": null,
                },
                {
                    "doc_id": "aaa",
                    "doc_title": "Doc A",
                    "file_path": "/kb/a.md",
                    "type": "stale",
                    "description": "Q3",
                    "line_ref": null,
                    "answered": false,
                    "answer": null,
                    "confidence": "deferred",
                    "agent_suggestion": null,
                },
            ],
            "total": 3,
            "answered": 0,
            "unanswered": 3,
        });
        let grouped = group_questions_by_doc(&result);
        let docs = grouped["documents"].as_array().unwrap();
        assert_eq!(docs.len(), 2);
        // First doc is "aaa" (insertion order preserved)
        assert_eq!(docs[0]["doc_id"], "aaa");
        assert_eq!(docs[0]["questions"].as_array().unwrap().len(), 2);
        assert_eq!(docs[1]["doc_id"], "bbb");
        assert_eq!(docs[1]["questions"].as_array().unwrap().len(), 1);
    }
}
