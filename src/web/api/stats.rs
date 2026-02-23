//! Stats API endpoints.
//!
//! Wraps existing stats functions for the web UI dashboard.

use super::errors::ApiError;
use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::get_review_queue;
use crate::organize::{detect_merge_candidates, detect_misplaced};
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

use super::super::server::WebAppState;

/// Response for aggregate stats endpoint.
#[derive(Debug, Serialize)]
pub struct AggregateStatsResponse {
    pub repos_count: usize,
    pub docs_count: usize,
    pub db_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan: Option<String>,
}

/// Response for review stats endpoint.
#[derive(Debug, Serialize)]
pub struct ReviewStatsResponse {
    pub total: u64,
    pub answered: u64,
    pub unanswered: u64,
}

/// Response for organize stats endpoint.
#[derive(Debug, Serialize)]
pub struct OrganizeStatsResponse {
    pub merge_candidates: usize,
    pub misplaced_candidates: usize,
    pub orphan_count: usize,
}

/// GET /api/stats - Aggregate statistics.
pub async fn get_stats(
    State(state): State<Arc<WebAppState>>,
) -> Result<Json<AggregateStatsResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let result = super::run_blocking_web(move || compute_aggregate_stats(&db)).await?;
    Ok(Json(result))
}

/// GET /api/stats/review - Review queue counts.
pub async fn get_review_stats(
    State(state): State<Arc<WebAppState>>,
) -> Result<Json<ReviewStatsResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let result = super::run_blocking_web(move || compute_review_stats(&db)).await?;
    Ok(Json(result))
}

/// GET /api/stats/organize - Organize suggestion counts.
pub async fn get_organize_stats(
    State(state): State<Arc<WebAppState>>,
) -> Result<Json<OrganizeStatsResponse>, (StatusCode, Json<ApiError>)> {
    let db = state.db.clone();
    let result = super::run_blocking_web(move || compute_organize_stats(&db)).await?;
    Ok(Json(result))
}

/// Compute aggregate stats from database.
fn compute_aggregate_stats(db: &Database) -> Result<AggregateStatsResponse, FactbaseError> {
    let repos = db.list_repositories_with_stats()?;
    let repos_count = repos.len();
    let docs_count: usize = repos.iter().map(|(_, c)| c).sum();
    let last_scan = repos.iter().filter_map(|(r, _)| r.last_indexed_at).max();

    // Get database size from pool stats (approximate)
    let db_size_bytes = db.pool_stats().connections as u64 * 1024; // Rough estimate

    Ok(AggregateStatsResponse {
        repos_count,
        docs_count,
        db_size_bytes,
        last_scan: last_scan.map(|ts| ts.to_rfc3339()),
    })
}

/// Compute review queue stats.
fn compute_review_stats(db: &Database) -> Result<ReviewStatsResponse, FactbaseError> {
    let args = serde_json::json!({});
    let result = get_review_queue(db, &args)?;

    let total = result.get("total").and_then(Value::as_u64).unwrap_or(0);
    let answered = result.get("answered").and_then(Value::as_u64).unwrap_or(0);
    let unanswered = result
        .get("unanswered")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    Ok(ReviewStatsResponse {
        total,
        answered,
        unanswered,
    })
}

/// Compute organize suggestion stats.
fn compute_organize_stats(db: &Database) -> Result<OrganizeStatsResponse, FactbaseError> {
    // Count merge candidates (default threshold 0.85)
    let merge_candidates = detect_merge_candidates(db, 0.85, None)?.len();

    // Count misplaced candidates
    let misplaced_candidates = detect_misplaced(db, None)?.len();

    // Count orphans across all repos
    let repos = db.list_repositories()?;
    let mut orphan_count = 0;
    for repo in &repos {
        orphan_count += crate::organize::load_orphan_entries(&repo.path)?.len();
    }

    Ok(OrganizeStatsResponse {
        merge_candidates,
        misplaced_candidates,
        orphan_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_stats_response_serialize() {
        let resp = AggregateStatsResponse {
            repos_count: 2,
            docs_count: 45,
            db_size_bytes: 131072,
            last_scan: Some("2024-01-25T12:00:00+00:00".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"repos_count\":2"));
        assert!(json.contains("\"docs_count\":45"));
        assert!(json.contains("\"db_size_bytes\":131072"));
        assert!(json.contains("\"last_scan\":"));
    }

    #[test]
    fn test_aggregate_stats_response_no_last_scan() {
        let resp = AggregateStatsResponse {
            repos_count: 0,
            docs_count: 0,
            db_size_bytes: 0,
            last_scan: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("last_scan"));
    }

    #[test]
    fn test_review_stats_response_serialize() {
        let resp = ReviewStatsResponse {
            total: 15,
            answered: 5,
            unanswered: 10,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"total\":15"));
        assert!(json.contains("\"answered\":5"));
        assert!(json.contains("\"unanswered\":10"));
    }

    #[test]
    fn test_organize_stats_response_serialize() {
        let resp = OrganizeStatsResponse {
            merge_candidates: 3,
            misplaced_candidates: 2,
            orphan_count: 5,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"merge_candidates\":3"));
        assert!(json.contains("\"misplaced_candidates\":2"));
        assert!(json.contains("\"orphan_count\":5"));
    }

    #[test]
    fn test_aggregate_stats_response_zero_values() {
        let resp = AggregateStatsResponse {
            repos_count: 0,
            docs_count: 0,
            db_size_bytes: 0,
            last_scan: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"repos_count\":0"));
        assert!(json.contains("\"docs_count\":0"));
        assert!(json.contains("\"db_size_bytes\":0"));
    }

    #[test]
    fn test_review_stats_response_all_answered() {
        let resp = ReviewStatsResponse {
            total: 10,
            answered: 10,
            unanswered: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"unanswered\":0"));
    }

    #[test]
    fn test_review_stats_response_none_answered() {
        let resp = ReviewStatsResponse {
            total: 10,
            answered: 0,
            unanswered: 10,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"answered\":0"));
    }

    #[test]
    fn test_organize_stats_response_zero_values() {
        let resp = OrganizeStatsResponse {
            merge_candidates: 0,
            misplaced_candidates: 0,
            orphan_count: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"merge_candidates\":0"));
        assert!(json.contains("\"misplaced_candidates\":0"));
        assert!(json.contains("\"orphan_count\":0"));
    }
}
