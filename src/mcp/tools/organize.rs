//! Organize-related MCP tool: get_duplicate_entries

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_str_arg, run_blocking};
use crate::organize::{assess_staleness, detect_duplicate_entries};
use serde_json::Value;
use tracing::instrument;

/// Detects entity entries duplicated across multiple documents.
///
/// Finds named entities (e.g., people listed under company team sections) that
/// appear in two or more documents, and assesses which entries are stale.
///
/// # Arguments (from JSON)
/// - `repo` (optional): Filter by repository ID
///
/// # Returns
/// JSON with `duplicates` array (entity_name, entries with doc_id/title/section/facts),
/// `stale` array (entity_name, current entry, stale entries), and counts.
#[instrument(name = "mcp_get_duplicate_entries", skip(db, embedding, args))]
pub async fn get_duplicate_entries<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let repo = get_str_arg(args, "repo").map(String::from);

    let duplicates = detect_duplicate_entries(db, embedding, repo.as_deref()).await?;

    let db2 = db.clone();
    let dups_clone = duplicates.clone();
    let stale = run_blocking(move || assess_staleness(&dups_clone, &db2)).await?;

    let dup_json: Vec<Value> = duplicates
        .iter()
        .map(|d| {
            serde_json::json!({
                "entity_name": d.entity_name,
                "document_count": d.entries.len(),
                "entries": d.entries.iter().map(|e| serde_json::json!({
                    "doc_id": e.doc_id,
                    "doc_title": e.doc_title,
                    "section": e.section,
                    "line_start": e.line_start,
                    "fact_count": e.facts.len(),
                    "facts": e.facts,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let stale_json: Vec<Value> = stale
        .iter()
        .map(|s| {
            serde_json::json!({
                "entity_name": s.entity_name,
                "current": {
                    "doc_id": s.current.doc_id,
                    "doc_title": s.current.doc_title,
                    "section": s.current.section,
                },
                "stale": s.stale.iter().map(|e| serde_json::json!({
                    "doc_id": e.doc_id,
                    "doc_title": e.doc_title,
                    "section": e.section,
                    "line_start": e.line_start,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "duplicates": dup_json,
        "stale": stale_json,
        "duplicate_count": duplicates.len(),
        "stale_count": stale.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_duplicate_entries_extracts_repo_arg() {
        let args = serde_json::json!({"repo": "notes"});
        assert_eq!(get_str_arg(&args, "repo"), Some("notes"));
    }

    #[test]
    fn test_get_duplicate_entries_no_repo_arg() {
        let args = serde_json::json!({});
        assert_eq!(get_str_arg(&args, "repo"), None);
    }
}
