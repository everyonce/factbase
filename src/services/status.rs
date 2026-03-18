//! KB health status service.
//!
//! Aggregates health metrics from the database into a structured report
//! used by `factbase(op='status')`, the CLI `factbase status`, and the
//! maintain workflow final report step.

use crate::database::Database;
use crate::error::FactbaseError;
use serde_json::Value;

/// Compute a structured KB health status report for a repository.
///
/// Returns both machine-readable JSON fields and a human-readable `summary` table.
/// When `repo_id` is `None`, uses the first registered repository.
pub fn kb_status(db: &Database, repo_id: Option<&str>) -> Result<Value, FactbaseError> {
    // Resolve repository
    let repos = db.list_repositories()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };

    let (active, deleted, by_type, last_scan, last_maintain, docs_with_review) =
        if let Some(ref r) = repo {
            let stats = db.get_stats(&r.id, None)?;
            let docs_with_review = db.count_docs_with_review_queue(Some(&r.id))?;
            (
                stats.active,
                stats.deleted,
                stats.by_type,
                r.last_indexed_at,
                r.last_lint_at,
                docs_with_review,
            )
        } else {
            (0, 0, std::collections::HashMap::new(), None, None, 0)
        };

    // Temporal and source coverage
    let (temporal_pct, source_pct, total_facts, facts_with_temporal, facts_with_sources) =
        if let Some(ref r) = repo {
            let ts = db.compute_temporal_stats(&r.id)?;
            let ss = db.compute_source_stats(&r.id)?;
            (
                ts.coverage_percent,
                ss.coverage_percent,
                ts.total_facts,
                ts.facts_with_tags,
                ss.facts_with_sources,
            )
        } else {
            (0.0f32, 0.0f32, 0usize, 0usize, 0usize)
        };

    // Review question counts (fast DB query — no file parsing)
    let repo_id_ref = repo.as_ref().map(|r| r.id.as_str());
    let (answered, open, deferred) = db.count_review_questions_by_status(repo_id_ref)?;
    let by_type_questions = db.count_open_questions_by_type(repo_id_ref)?;

    // Build human-readable summary table
    let summary = build_summary_table(SummaryTableArgs {
        active,
        docs_with_review,
        temporal_pct,
        source_pct,
        total_facts,
        open,
        deferred,
        by_type: &by_type_questions,
        last_scan,
        last_maintain,
    });

    Ok(serde_json::json!({
        "docs": {
            "active": active,
            "deleted": deleted,
            "with_review_sections": docs_with_review,
            "by_type": by_type,
        },
        "coverage": {
            "temporal_percent": temporal_pct,
            "source_percent": source_pct,
            "total_facts": total_facts,
            "facts_with_temporal": facts_with_temporal,
            "facts_with_sources": facts_with_sources,
        },
        "review": {
            "open": open,
            "deferred": deferred,
            "answered": answered,
            "by_type": by_type_questions,
        },
        "last_scan": last_scan.map(|t| t.to_rfc3339()),
        "last_maintain": last_maintain.map(|t| t.to_rfc3339()),
        "summary": summary,
    }))
}

struct SummaryTableArgs<'a> {
    active: usize,
    docs_with_review: usize,
    temporal_pct: f32,
    source_pct: f32,
    total_facts: usize,
    open: u64,
    deferred: u64,
    by_type: &'a std::collections::HashMap<String, u64>,
    last_scan: Option<chrono::DateTime<chrono::Utc>>,
    last_maintain: Option<chrono::DateTime<chrono::Utc>>,
}

fn build_summary_table(args: SummaryTableArgs<'_>) -> String {
    let SummaryTableArgs {
        active,
        docs_with_review,
        temporal_pct,
        source_pct,
        total_facts,
        open,
        deferred,
        by_type,
        last_scan,
        last_maintain,
    } = args;
    let mut lines = vec![
        "KB Health Status".to_string(),
        "─────────────────────────────────────────".to_string(),
        format!(
            "Documents:        {} active  ({} with review sections)",
            active, docs_with_review
        ),
        format!("Facts:            {} total", total_facts),
        format!(
            "Temporal coverage: {:.1}%  ({} facts tagged)",
            temporal_pct,
            (total_facts as f32 * temporal_pct / 100.0) as usize
        ),
        format!(
            "Source coverage:   {:.1}%  ({} facts cited)",
            source_pct,
            (total_facts as f32 * source_pct / 100.0) as usize
        ),
        "─────────────────────────────────────────".to_string(),
        format!("Open review questions: {}", open),
    ];

    if !by_type.is_empty() {
        let mut sorted: Vec<_> = by_type.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (qt, count) in &sorted {
            lines.push(format!("  {:20} {}", qt, count));
        }
    }

    if deferred > 0 {
        lines.push(format!("Deferred questions:    {}", deferred));
    }

    lines.push("─────────────────────────────────────────".to_string());

    if let Some(ts) = last_scan {
        lines.push(format!("Last scan:    {}", ts.format("%Y-%m-%d %H:%M UTC")));
    } else {
        lines.push("Last scan:    never".to_string());
    }
    if let Some(ts) = last_maintain {
        lines.push(format!(
            "Last maintain: {}",
            ts.format("%Y-%m-%d %H:%M UTC")
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db};
    use tempfile::TempDir;

    #[test]
    fn test_kb_status_empty_repo() {
        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        test_repo_in_db(&db, "test", repo_dir.path());

        let result = kb_status(&db, None).unwrap();
        assert_eq!(result["docs"]["active"], 0);
        assert_eq!(result["review"]["open"], 0);
        assert!(result["summary"].is_string());
    }

    #[test]
    fn test_kb_status_has_required_fields() {
        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        test_repo_in_db(&db, "test", repo_dir.path());

        let result = kb_status(&db, None).unwrap();
        assert!(result.get("docs").is_some());
        assert!(result.get("coverage").is_some());
        assert!(result.get("review").is_some());
        assert!(result.get("summary").is_some());
        assert!(result["docs"].get("active").is_some());
        assert!(result["docs"].get("with_review_sections").is_some());
        assert!(result["coverage"].get("temporal_percent").is_some());
        assert!(result["coverage"].get("source_percent").is_some());
        assert!(result["review"].get("open").is_some());
        assert!(result["review"].get("deferred").is_some());
        assert!(result["review"].get("by_type").is_some());
    }

    #[test]
    fn test_kb_status_no_repo_returns_zeros() {
        let (db, _tmp) = test_db();
        // No repo registered
        let result = kb_status(&db, None).unwrap();
        assert_eq!(result["docs"]["active"], 0);
        assert_eq!(result["review"]["open"], 0);
    }

    #[test]
    fn test_kb_status_summary_contains_key_info() {
        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        test_repo_in_db(&db, "test", repo_dir.path());

        let result = kb_status(&db, None).unwrap();
        let summary = result["summary"].as_str().unwrap();
        assert!(summary.contains("KB Health Status"));
        assert!(summary.contains("Documents:"));
        assert!(summary.contains("Temporal coverage:"));
        assert!(summary.contains("Source coverage:"));
        assert!(summary.contains("Open review questions:"));
    }
}
