//! Organize suggestion API endpoints.
//!
//! Wraps existing organize detection and execution functions for the web UI.
//!
//! Note: Split detection is not available via web API as it requires an
//! embedding provider (Ollama). Use CLI `factbase organize analyze` instead.

use crate::error::FactbaseError;
use crate::organize::fs_helpers::{read_file, write_file};
use crate::organize::{
    detect_merge_candidates, detect_misplaced, load_orphan_entries, orphan_file_path,
    process_orphan_answers, validate_orphan_answer, DuplicateEntry, MergeCandidate,
    MisplacedCandidate, OrphanEntry,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::server::WebAppState;
use super::errors::ApiError;

/// Query parameters for suggestions listing.
#[derive(Debug, Deserialize)]
pub struct SuggestionsQuery {
    /// Filter by repository ID
    pub repo: Option<String>,
    /// Filter by suggestion type (merge, misplaced)
    #[serde(rename = "type")]
    pub suggestion_type: Option<String>,
    /// Similarity threshold for merge detection (default: 0.95)
    pub threshold: Option<f32>,
}

/// Combined suggestions response.
///
/// Note: `split` and `duplicate_entries` are always empty in web API.
/// Use CLI `factbase organize analyze` for split and duplicate detection
/// (they require an embedding provider).
#[derive(Debug, Serialize)]
pub struct SuggestionsResponse {
    pub merge: Vec<MergeCandidate>,
    pub misplaced: Vec<MisplacedCandidate>,
    pub duplicate_entries: Vec<DuplicateEntry>,
    pub total: usize,
}

/// Request body for approving a suggestion.
#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    /// Type of suggestion: "merge", "split", "move", "retype"
    #[serde(rename = "type")]
    pub suggestion_type: String,
    /// Primary document ID
    pub doc_id: String,
    /// Secondary document ID (for merge)
    pub target_id: Option<String>,
    /// Target folder (for move)
    pub target_folder: Option<String>,
    /// Target type (for retype)
    pub target_type: Option<String>,
}

/// Request body for dismissing a suggestion.
#[derive(Debug, Deserialize)]
pub struct DismissRequest {
    /// Type of suggestion: "merge", "split", "misplaced"
    #[serde(rename = "type")]
    pub suggestion_type: String,
    /// Primary document ID
    pub doc_id: String,
    /// Secondary document ID (for merge)
    pub target_id: Option<String>,
}

/// Orphan list response.
#[derive(Debug, Serialize)]
pub struct OrphansResponse {
    pub orphans: Vec<OrphanEntryResponse>,
    pub total: usize,
    pub answered: usize,
    pub unanswered: usize,
}

/// Single orphan entry for API response.
#[derive(Debug, Serialize)]
pub struct OrphanEntryResponse {
    pub content: String,
    pub source_doc: Option<String>,
    pub source_line: Option<usize>,
    pub answered: bool,
    pub answer: Option<String>,
    pub line_number: usize,
}

impl From<OrphanEntry> for OrphanEntryResponse {
    fn from(entry: OrphanEntry) -> Self {
        Self {
            content: entry.content,
            source_doc: entry.source_doc,
            source_line: entry.source_line,
            answered: entry.answered,
            answer: entry.answer,
            line_number: entry.line_number,
        }
    }
}

/// Request body for assigning an orphan.
#[derive(Debug, Deserialize)]
pub struct AssignOrphanRequest {
    /// Repository ID
    pub repo: String,
    /// Line number in _orphans.md
    pub line_number: usize,
    /// Target document ID or "dismiss"
    pub target: String,
}

/// GET /api/organize/suggestions - List pending organize suggestions.
///
/// Query params:
/// - `repo`: Filter by repository ID
/// - `type`: Filter by suggestion type (merge, misplaced)
/// - `threshold`: Similarity threshold for merge detection (default: 0.95)
///
/// Note: Split detection requires embedding provider and is not available via web API.
/// Use CLI `factbase organize analyze` for split detection.
pub async fn list_suggestions(
    State(state): State<Arc<WebAppState>>,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<SuggestionsResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let repo = query.repo.clone();
    let suggestion_type = query.suggestion_type.clone();
    let threshold = query.threshold.unwrap_or(0.95);

    let result = super::run_blocking_web(move || {
        let repo_ref = repo.as_deref();

        let mut merge = Vec::new();
        let mut misplaced = Vec::new();

        // Fetch based on type filter or all
        match suggestion_type.as_deref() {
            Some("merge") => {
                merge = detect_merge_candidates(
                    &db,
                    threshold,
                    repo_ref,
                    &crate::ProgressReporter::Silent,
                )?;
            }
            Some("misplaced") => {
                misplaced = detect_misplaced(&db, repo_ref, &crate::ProgressReporter::Silent)?;
            }
            Some("split") | Some("duplicate") => {
                // Split/duplicate detection requires embedding provider - not available via web API
                // Return empty result with note
            }
            _ => {
                // Fetch all types (except split which requires embedding)
                merge = detect_merge_candidates(
                    &db,
                    threshold,
                    repo_ref,
                    &crate::ProgressReporter::Silent,
                )?;
                misplaced = detect_misplaced(&db, repo_ref, &crate::ProgressReporter::Silent)?;
            }
        }

        let total = merge.len() + misplaced.len();

        Ok(SuggestionsResponse {
            merge,
            misplaced,
            duplicate_entries: Vec::new(),
            total,
        })
    })
    .await?;

    Ok(Json(result))
}

/// GET /api/organize/suggestions/:doc_id - Get suggestions for specific document.
pub async fn get_document_suggestions(
    State(state): State<Arc<WebAppState>>,
    Path(doc_id): Path<String>,
) -> Result<Json<SuggestionsResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();

    let result = super::run_blocking_web(move || {
        // Get all suggestions and filter to those involving this document
        let all_merge = detect_merge_candidates(&db, 0.95, None, &crate::ProgressReporter::Silent)?;
        let all_misplaced = detect_misplaced(&db, None, &crate::ProgressReporter::Silent)?;

        let merge: Vec<_> = all_merge
            .into_iter()
            .filter(|c| c.doc1_id == doc_id || c.doc2_id == doc_id)
            .collect();

        let misplaced: Vec<_> = all_misplaced
            .into_iter()
            .filter(|c| c.doc_id == doc_id)
            .collect();

        let total = merge.len() + misplaced.len();

        Ok(SuggestionsResponse {
            merge,
            misplaced,
            duplicate_entries: Vec::new(),
            total,
        })
    })
    .await?;

    Ok(Json(result))
}

/// POST /api/organize/approve - Approve and execute a suggestion.
///
/// Note: This endpoint is a placeholder. Full implementation requires
/// LLM integration for merge/split planning which is complex.
/// For now, returns an error indicating CLI should be used.
pub async fn approve_suggestion(
    Json(body): Json<ApproveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // For now, direct users to CLI for complex operations
    // Full implementation would require:
    // - LLM provider for merge/split planning
    // - Snapshot creation
    // - Verification
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiError::new(
            format!(
                "Use CLI for {} operations: `factbase organize {} {}`",
                body.suggestion_type, body.suggestion_type, body.doc_id
            ),
            "NOT_IMPLEMENTED",
        )),
    ))
}

/// POST /api/organize/dismiss - Dismiss a suggestion.
///
/// Note: Suggestions are detected dynamically, not stored.
/// Dismissing would require a "dismissed suggestions" table.
/// For now, returns success (no-op) since suggestions regenerate on next query.
pub async fn dismiss_suggestion(
    Json(_body): Json<DismissRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Suggestions are computed dynamically, not stored
    // A proper dismiss would need a "dismissed_suggestions" table
    // For now, acknowledge the dismiss (it will reappear on next detection)
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Suggestion dismissed. Note: suggestions are computed dynamically and may reappear."
    })))
}

/// GET /api/organize/orphans - List orphaned facts.
///
/// Query params:
/// - `repo`: Repository ID (required)
pub async fn list_orphans(
    State(state): State<Arc<WebAppState>>,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<OrphansResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let repo_id = query.repo.clone();

    let result = super::run_blocking_web(move || {
        let repo_id = repo_id.ok_or_else(|| FactbaseError::parse("repo parameter is required"))?;

        let repo = db.require_repository(&repo_id)?;

        let entries = load_orphan_entries(&repo.path)?;
        let total = entries.len();
        let answered = entries.iter().filter(|e| e.answered).count();
        let unanswered = total - answered;

        let orphans: Vec<OrphanEntryResponse> = entries.into_iter().map(Into::into).collect();

        Ok(OrphansResponse {
            orphans,
            total,
            answered,
            unanswered,
        })
    })
    .await?;

    Ok(Json(result))
}

/// POST /api/organize/assign-orphan - Assign an orphan to a document.
pub async fn assign_orphan(
    State(state): State<Arc<WebAppState>>,
    Json(body): Json<AssignOrphanRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Validate the target
    let _ = validate_orphan_answer(&body.target).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(e.to_string())),
        )
    })?;

    let db = state.db.clone();
    let repo_id = body.repo.clone();
    let line_number = body.line_number;
    let target = body.target.clone();

    let result = super::run_blocking_web(move || {
        let repo = db.require_repository(&repo_id)?;

        let orphan_path = orphan_file_path(&repo.path);

        if !orphan_path.exists() {
            return Err(FactbaseError::not_found("No orphan file found"));
        }

        let content = read_file(&orphan_path)?;

        // Update the specific line with the answer
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        let line_idx = line_number.saturating_sub(1);

        if line_idx >= lines.len() {
            return Err(FactbaseError::not_found(format!(
                "Line {line_number} not found in orphan file"
            )));
        }

        let line = &lines[line_idx];

        // Convert simple format to checkbox format with answer
        if line.contains("@r[orphan]") && !line.contains("[ ]") && !line.contains("[x]") {
            // Simple format: `- content @r[orphan] <!-- from doc line N -->`
            // Convert to: `- [x] content @r[orphan] <!-- from doc line N --> → answer`
            let new_line = line.replacen("- ", "- [x] ", 1);
            lines[line_idx] = format!("{new_line} → {target}");
        } else if line.contains("[ ]") {
            // Checkbox format unchecked: mark as checked and add answer
            let new_line = line.replace("[ ]", "[x]");
            lines[line_idx] = format!("{new_line} → {target}");
        } else {
            return Err(FactbaseError::parse(format!(
                "Line {line_number} is not a valid orphan entry"
            )));
        }

        // Write updated content
        let new_content = lines.join("\n");
        write_file(&orphan_path, &new_content)?;

        // Now process the answers
        let result = process_orphan_answers(&repo.path, &db)?;

        Ok(serde_json::json!({
            "success": true,
            "assigned": result.assigned_count,
            "dismissed": result.dismissed_count,
            "remaining": result.remaining_count
        }))
    })
    .await?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestions_query_deserialize() {
        let json = r#"{"repo": "main", "type": "merge", "threshold": 0.9}"#;
        let query: SuggestionsQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.repo, Some("main".to_string()));
        assert_eq!(query.suggestion_type, Some("merge".to_string()));
        assert_eq!(query.threshold, Some(0.9));
    }

    #[test]
    fn test_suggestions_query_deserialize_empty() {
        let json = r#"{}"#;
        let query: SuggestionsQuery = serde_json::from_str(json).unwrap();
        assert!(query.repo.is_none());
        assert!(query.suggestion_type.is_none());
        assert!(query.threshold.is_none());
    }

    #[test]
    fn test_approve_request_deserialize() {
        let json = r#"{"type": "merge", "doc_id": "abc123", "target_id": "def456"}"#;
        let req: ApproveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.suggestion_type, "merge");
        assert_eq!(req.doc_id, "abc123");
        assert_eq!(req.target_id, Some("def456".to_string()));
    }

    #[test]
    fn test_dismiss_request_deserialize() {
        let json = r#"{"type": "split", "doc_id": "abc123"}"#;
        let req: DismissRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.suggestion_type, "split");
        assert_eq!(req.doc_id, "abc123");
        assert!(req.target_id.is_none());
    }

    #[test]
    fn test_assign_orphan_request_deserialize() {
        let json = r#"{"repo": "main", "line_number": 5, "target": "abc123"}"#;
        let req: AssignOrphanRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.repo, "main");
        assert_eq!(req.line_number, 5);
        assert_eq!(req.target, "abc123");
    }

    #[test]
    fn test_orphan_entry_response_from() {
        let entry = OrphanEntry {
            content: "Some fact".to_string(),
            source_doc: Some("abc123".to_string()),
            source_line: Some(10),
            answered: true,
            answer: Some("def456".to_string()),
            line_number: 5,
        };

        let response: OrphanEntryResponse = entry.into();
        assert_eq!(response.content, "Some fact");
        assert_eq!(response.source_doc, Some("abc123".to_string()));
        assert_eq!(response.source_line, Some(10));
        assert!(response.answered);
        assert_eq!(response.answer, Some("def456".to_string()));
        assert_eq!(response.line_number, 5);
    }

    #[test]
    fn test_suggestions_response_serialize() {
        let response = SuggestionsResponse {
            merge: vec![],
            misplaced: vec![],
            duplicate_entries: vec![],
            total: 0,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"total\":0"));
        assert!(json.contains("\"merge\":[]"));
        assert!(json.contains("\"duplicate_entries\":[]"));
    }

    #[test]
    fn test_orphans_response_serialize() {
        let response = OrphansResponse {
            orphans: vec![],
            total: 5,
            answered: 2,
            unanswered: 3,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"total\":5"));
        assert!(json.contains("\"answered\":2"));
        assert!(json.contains("\"unanswered\":3"));
    }

    // Additional edge case tests
    #[test]
    fn test_suggestions_query_threshold_bounds() {
        // Test threshold at boundaries
        let json = r#"{"threshold": 0.0}"#;
        let query: SuggestionsQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.threshold, Some(0.0));

        let json = r#"{"threshold": 1.0}"#;
        let query: SuggestionsQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.threshold, Some(1.0));
    }

    #[test]
    fn test_approve_request_move_operation() {
        let json = r#"{"type": "move", "doc_id": "abc123", "target_folder": "people/"}"#;
        let req: ApproveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.suggestion_type, "move");
        assert_eq!(req.doc_id, "abc123");
        assert_eq!(req.target_folder, Some("people/".to_string()));
        assert!(req.target_id.is_none());
    }

    #[test]
    fn test_approve_request_retype_operation() {
        let json = r#"{"type": "retype", "doc_id": "abc123", "target_type": "person"}"#;
        let req: ApproveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.suggestion_type, "retype");
        assert_eq!(req.doc_id, "abc123");
        assert_eq!(req.target_type, Some("person".to_string()));
    }

    #[test]
    fn test_dismiss_request_with_target() {
        let json = r#"{"type": "merge", "doc_id": "abc123", "target_id": "def456"}"#;
        let req: DismissRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.suggestion_type, "merge");
        assert_eq!(req.target_id, Some("def456".to_string()));
    }

    #[test]
    fn test_assign_orphan_dismiss_target() {
        let json = r#"{"repo": "main", "line_number": 1, "target": "dismiss"}"#;
        let req: AssignOrphanRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.target, "dismiss");
    }

    #[test]
    fn test_orphan_entry_response_minimal() {
        let entry = OrphanEntry {
            content: "Minimal fact".to_string(),
            source_doc: None,
            source_line: None,
            answered: false,
            answer: None,
            line_number: 1,
        };

        let response: OrphanEntryResponse = entry.into();
        assert_eq!(response.content, "Minimal fact");
        assert!(response.source_doc.is_none());
        assert!(response.source_line.is_none());
        assert!(!response.answered);
        assert!(response.answer.is_none());
    }

    #[test]
    fn test_orphans_response_empty() {
        let response = OrphansResponse {
            orphans: vec![],
            total: 0,
            answered: 0,
            unanswered: 0,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"orphans\":[]"));
        assert!(json.contains("\"total\":0"));
    }

    #[test]
    fn test_suggestions_query_all_types() {
        for stype in ["merge", "misplaced", "split"] {
            let json = format!(r#"{{"type": "{}"}}"#, stype);
            let query: SuggestionsQuery = serde_json::from_str(&json).unwrap();
            assert_eq!(query.suggestion_type, Some(stype.to_string()));
        }
    }
}
