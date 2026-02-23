//! Organize-related MCP tools.

use std::path::Path;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_bool_arg, get_str_arg, get_str_arg_required, run_blocking};
use crate::organize::{
    assess_staleness, create_snapshot, cleanup, rollback,
    detect_duplicate_entries, detect_merge_candidates, detect_misplaced, detect_split_candidates,
    execute_merge, execute_move, execute_retype, execute_split, extract_sections,
    plan_merge, plan_split, process_orphan_answers, verify_merge, verify_split,
};
use crate::processor::DocumentProcessor;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Resolve a repository from the database, optionally filtered by ID.
fn resolve_repo(db: &Database, repo_id: Option<&str>) -> Result<crate::models::Repository, FactbaseError> {
    let repos = db.list_repositories()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.ok_or_else(|| FactbaseError::NotFound("No repository found.".into()))
}

/// Unified organize tool dispatcher. Routes to the appropriate action based on the "action" field.
#[instrument(name = "mcp_organize", skip(db, _embedding, llm, args, progress))]
pub async fn organize<E: EmbeddingProvider>(
    db: &Database,
    _embedding: &E,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let action = get_str_arg_required(args, "action")?;
    match action.as_str() {
        "merge" => organize_merge(db, llm, args, progress).await,
        "split" => organize_split(db, llm, args, progress).await,
        "move" => organize_move(db, args),
        "retype" => organize_retype(db, args),
        "apply" => organize_apply(db, args),
        _ => Err(FactbaseError::parse(format!(
            "Unknown organize action: '{action}'. Expected: merge, split, move, retype, apply"
        ))),
    }
}

/// Detects entity entries duplicated across multiple documents.
async fn get_duplicate_entries<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    repo: Option<&str>,
    progress: &ProgressReporter,
) -> Result<(Vec<Value>, Vec<Value>), FactbaseError> {
    let duplicates = detect_duplicate_entries(db, embedding, repo, progress).await?;

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

    Ok((dup_json, stale_json))
}

/// Analyze repository for reorganization opportunities (merge, split, misplaced, duplicates).
/// Also supports focus="duplicates" to return only detailed duplicate/stale entry info.
#[instrument(name = "mcp_organize_analyze", skip(db, embedding, args, progress))]
pub async fn organize_analyze<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo").map(String::from);
    let focus = get_str_arg(args, "focus").map(String::from);
    let rid = repo_id.as_deref();

    // Focused duplicate-only mode (replaces the old get_duplicate_entries tool)
    if focus.as_deref() == Some("duplicates") {
        let (dup_json, stale_json) = get_duplicate_entries(db, embedding, rid, progress).await?;
        return Ok(serde_json::json!({
            "duplicates": dup_json,
            "stale": stale_json,
            "duplicate_count": dup_json.len(),
            "stale_count": stale_json.len(),
        }));
    }

    let merge_threshold = args.get("merge_threshold").and_then(|v| v.as_f64()).unwrap_or(0.95) as f32;
    let split_threshold = args.get("split_threshold").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;

    progress.phase("Analysis 1/4: Merge candidates");
    let merge_candidates = {
        let db2 = db.clone();
        let p = progress.clone();
        let rid2 = rid.map(String::from);
        run_blocking(move || detect_merge_candidates(&db2, merge_threshold, rid2.as_deref(), &p)).await?
    };

    progress.phase("Analysis 2/4: Split candidates");
    let split_candidates = detect_split_candidates(db, embedding, split_threshold, rid, progress).await?;

    progress.phase("Analysis 3/4: Misplaced documents");
    let misplaced_candidates = {
        let db2 = db.clone();
        let p = progress.clone();
        let rid2 = rid.map(String::from);
        run_blocking(move || detect_misplaced(&db2, rid2.as_deref(), &p)).await?
    };

    progress.phase("Analysis 4/4: Duplicate entries");
    let duplicate_entries = detect_duplicate_entries(db, embedding, rid, progress).await?;

    let db2 = db.clone();
    let dups = duplicate_entries.clone();
    let stale_entries = run_blocking(move || assess_staleness(&dups, &db2)).await?;

    Ok(serde_json::json!({
        "merge_candidates": merge_candidates.iter().map(|c| serde_json::json!({
            "doc1_id": c.doc1_id,
            "doc1_title": c.doc1_title,
            "doc2_id": c.doc2_id,
            "doc2_title": c.doc2_title,
            "similarity": c.similarity,
            "suggested_keep": c.suggested_keep,
            "rationale": c.rationale,
        })).collect::<Vec<_>>(),
        "split_candidates": split_candidates.iter().map(|c| serde_json::json!({
            "doc_id": c.doc_id,
            "doc_title": c.doc_title,
            "sections": c.sections.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            "avg_similarity": c.avg_similarity,
            "rationale": c.rationale,
        })).collect::<Vec<_>>(),
        "misplaced_candidates": misplaced_candidates.iter().map(|c| serde_json::json!({
            "doc_id": c.doc_id,
            "doc_title": c.doc_title,
            "current_type": c.current_type,
            "suggested_type": c.suggested_type,
            "confidence": c.confidence,
            "rationale": c.rationale,
        })).collect::<Vec<_>>(),
        "duplicate_entries": duplicate_entries.len(),
        "stale_entries": stale_entries.len(),
        "total_suggestions": merge_candidates.len() + split_candidates.len() + misplaced_candidates.len() + duplicate_entries.len() + stale_entries.len(),
    }))
}

/// Merge two documents into one with fact-level accounting.
#[instrument(name = "mcp_organize_merge", skip(db, llm, args, progress))]
async fn organize_merge(
    db: &Database,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let llm = llm.ok_or_else(|| FactbaseError::internal("LLM provider required for organize_merge"))?;
    let doc1 = get_str_arg_required(args, "doc1")?;
    let doc2 = get_str_arg_required(args, "doc2")?;
    let into = get_str_arg(args, "into");
    let dry_run = get_bool_arg(args, "dry_run", false);

    // Validate documents exist
    let d1 = db.require_document(&doc1)?;
    let d2 = db.require_document(&doc2)?;

    // Determine which to keep
    let keep_id = if let Some(into_id) = into {
        if into_id != doc1 && into_id != doc2 {
            return Err(FactbaseError::parse(format!(
                "'into' must be one of the documents being merged ({doc1} or {doc2})"
            )));
        }
        into_id.to_string()
    } else {
        let links1 = db.get_links_from(&doc1).unwrap_or_default().len();
        let links2 = db.get_links_from(&doc2).unwrap_or_default().len();
        if d1.content.len() + links1 * 100 >= d2.content.len() + links2 * 100 { doc1.clone() } else { doc2.clone() }
    };
    let merge_id = if keep_id == doc1 { &doc2 } else { &doc1 };

    // Resolve repo path
    let repo = resolve_repo(db, Some(d1.repo_id.as_str()))?;

    progress.log(&format!("Planning merge: keep {} ← {}", keep_id, merge_id));
    let plan = plan_merge(&keep_id, &[merge_id.as_str()], db, llm).await?;

    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "keep_id": keep_id,
            "merge_id": merge_id,
            "fact_count": plan.ledger.source_facts.len(),
            "duplicate_count": plan.duplicate_count(),
            "orphan_count": plan.orphan_count(),
        }));
    }

    // Execute with snapshot-based rollback
    let doc_ids: Vec<&str> = vec![&keep_id, merge_id.as_str()];
    let snapshot = create_snapshot(&doc_ids, db, &repo.path)?;

    let result = match execute_merge(&plan, db, &repo.path) {
        Ok(r) => r,
        Err(e) => {
            rollback(&snapshot, db)?;
            return Err(e);
        }
    };

    let verification = verify_merge(&plan, &result, db, &repo.path)?;
    if !verification.passed {
        rollback(&snapshot, db)?;
        return Err(FactbaseError::internal(format!(
            "Merge verification failed: {}",
            verification.mismatch_details.as_deref().unwrap_or("unknown")
        )));
    }
    cleanup(&snapshot)?;

    Ok(serde_json::json!({
        "kept_id": result.kept_id,
        "merged_ids": result.merged_ids,
        "fact_count": result.fact_count,
        "duplicate_count": result.duplicate_count,
        "orphan_count": result.orphan_count,
        "orphan_path": result.orphan_path.map(|p| p.display().to_string()),
        "links_redirected": result.links_redirected,
        "message": format!("Merged {} into {}. Run scan_repository to re-index.", merge_id, keep_id),
    }))
}

/// Split a multi-topic document into separate documents.
#[instrument(name = "mcp_organize_split", skip(db, llm, args, progress))]
async fn organize_split(
    db: &Database,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let llm = llm.ok_or_else(|| FactbaseError::internal("LLM provider required for organize_split"))?;
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let at = get_str_arg(args, "at").map(String::from);
    let dry_run = get_bool_arg(args, "dry_run", false);

    let doc = db.require_document(&doc_id)?;
    let repo = resolve_repo(db, Some(doc.repo_id.as_str()))?;

    let mut sections = extract_sections(&doc.content);
    sections.retain(|s| s.content.len() >= 50);

    if let Some(ref at_title) = at {
        let matching: Vec<_> = sections
            .into_iter()
            .filter(|s| s.title.to_lowercase().contains(&at_title.to_lowercase()))
            .collect();
        if matching.is_empty() {
            return Err(FactbaseError::parse(format!("No section matching '{at_title}' found")));
        }
        sections = matching;
    }

    if sections.len() < 2 {
        return Err(FactbaseError::parse(format!(
            "Document has {} section(s). Need at least 2 to split.", sections.len()
        )));
    }

    progress.log(&format!("Planning split for {} ({} sections)", doc_id, sections.len()));
    let plan = plan_split(&doc_id, &sections, db, llm).await?;

    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "source_id": doc_id,
            "source_title": doc.title,
            "sections": plan.new_documents.iter().map(|d| serde_json::json!({
                "title": d.title,
                "section_title": d.section_title,
            })).collect::<Vec<_>>(),
            "fact_count": plan.ledger.source_facts.len(),
            "orphan_count": plan.orphan_count(),
        }));
    }

    let doc_ids: Vec<&str> = vec![doc_id.as_str()];
    let snapshot = create_snapshot(&doc_ids, db, &repo.path)?;

    let result = match execute_split(&plan, db, &repo.path) {
        Ok(r) => r,
        Err(e) => {
            rollback(&snapshot, db)?;
            return Err(e);
        }
    };

    let verification = verify_split(&plan, &result, db, &repo.path)?;
    if !verification.passed {
        rollback(&snapshot, db)?;
        return Err(FactbaseError::internal(format!(
            "Split verification failed: {}",
            verification.mismatch_details.as_deref().unwrap_or("unknown")
        )));
    }
    cleanup(&snapshot)?;

    Ok(serde_json::json!({
        "source_id": result.source_id,
        "new_doc_ids": result.new_doc_ids,
        "fact_count": result.fact_count,
        "orphan_count": result.orphan_count,
        "orphan_path": result.orphan_path.map(|p| p.display().to_string()),
        "message": format!("Split {} into {} documents. Run scan_repository to re-index.", doc_id, result.new_doc_ids.len()),
    }))
}

/// Move a document to a different folder.
#[instrument(name = "mcp_organize_move", skip(db, args))]
fn organize_move(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let to = get_str_arg_required(args, "to")?;
    let dry_run = get_bool_arg(args, "dry_run", false);

    let doc = db.require_document(&doc_id)?;
    let repo = resolve_repo(db, Some(doc.repo_id.as_str()))?;

    let old_path = Path::new(&doc.file_path);
    let filename = old_path.file_name()
        .ok_or_else(|| FactbaseError::internal(format!("Invalid file path: {}", doc.file_path)))?;

    let dest = to.trim_end_matches('/');
    let new_path = if dest.ends_with(".md") {
        dest.to_string()
    } else {
        format!("{}/{}", dest, filename.to_string_lossy())
    };

    let processor = DocumentProcessor::new();
    let new_type = processor.derive_type(Path::new(&new_path), Path::new(""));

    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "doc_id": doc_id,
            "doc_title": doc.title,
            "old_path": doc.file_path,
            "new_path": new_path,
            "old_type": doc.doc_type,
            "new_type": new_type,
        }));
    }

    let result = execute_move(&doc_id, Path::new(&new_path), db, &repo.path)?;

    Ok(serde_json::json!({
        "doc_id": result.doc_id,
        "old_path": result.old_path,
        "new_path": result.new_path,
        "old_type": result.old_type,
        "new_type": result.new_type,
        "message": format!("Moved {} → {}. Type: {} → {}", result.old_path, result.new_path, result.old_type.as_deref().unwrap_or("none"), result.new_type),
    }))
}

/// Change a document's type without moving it.
#[instrument(name = "mcp_organize_retype", skip(db, args))]
fn organize_retype(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let new_type = get_str_arg_required(args, "new_type")?;
    let persist = get_bool_arg(args, "persist", false);

    let doc = db.require_document(&doc_id)?;
    let repo_path = if persist {
        Some(resolve_repo(db, Some(doc.repo_id.as_str()))?.path)
    } else {
        None
    };

    let result = execute_retype(&doc_id, &new_type, db, persist, repo_path.as_deref())?;

    Ok(serde_json::json!({
        "doc_id": result.doc_id,
        "old_type": result.old_type,
        "new_type": result.new_type,
        "persisted_to_file": result.persisted_to_file,
    }))
}

/// Process answered orphan markers from _orphans.md.
#[instrument(name = "mcp_organize_apply", skip(db, args))]
fn organize_apply(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo").map(String::from);
    let repo = resolve_repo(db, repo_id.as_deref())?;

    let result = process_orphan_answers(&repo.path, db)?;

    Ok(serde_json::json!({
        "assigned_count": result.assigned_count,
        "dismissed_count": result.dismissed_count,
        "remaining_count": result.remaining_count,
        "modified_docs": result.modified_docs,
        "message": if result.assigned_count + result.dismissed_count > 0 {
            format!("Processed {} orphan(s): {} assigned, {} dismissed, {} remaining. Run scan_repository to re-index.",
                result.assigned_count + result.dismissed_count, result.assigned_count, result.dismissed_count, result.remaining_count)
        } else {
            "No answered orphans to process.".to_string()
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::get_str_arg;

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

    #[test]
    fn test_organize_move_extracts_args() {
        let args = serde_json::json!({"doc_id": "abc123", "to": "people/"});
        assert_eq!(get_str_arg(&args, "doc_id"), Some("abc123"));
        assert_eq!(get_str_arg(&args, "to"), Some("people/"));
    }

    #[test]
    fn test_organize_retype_extracts_args() {
        let args = serde_json::json!({"doc_id": "abc123", "new_type": "person", "persist": true});
        assert_eq!(get_str_arg(&args, "doc_id"), Some("abc123"));
        assert_eq!(get_str_arg(&args, "new_type"), Some("person"));
        assert!(get_bool_arg(&args, "persist", false));
    }

    #[test]
    fn test_organize_merge_validates_into() {
        let args = serde_json::json!({"doc1": "aaa", "doc2": "bbb", "into": "ccc"});
        let into = get_str_arg(&args, "into").unwrap();
        let doc1 = get_str_arg(&args, "doc1").unwrap();
        let doc2 = get_str_arg(&args, "doc2").unwrap();
        assert!(into != doc1 && into != doc2);
    }
}
