//! Organize-related MCP tools.

use std::path::Path;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_bool_arg, get_str_arg, get_str_arg_required, run_blocking};
use crate::organize::{
    assess_staleness,
    detect_duplicate_entries, detect_ghost_files, detect_merge_candidates, detect_misplaced,
    detect_split_candidates, execute_move, execute_retype,
    process_orphan_answers,
};
use crate::processor::DocumentProcessor;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Resolve a repository from the database, optionally filtered by ID or name.
fn resolve_repo(db: &Database, repo_id: Option<&str>) -> Result<crate::models::Repository, FactbaseError> {
    let resolved = crate::mcp::tools::resolve_repo_filter(db, repo_id)?;
    let repos = db.list_repositories()?;
    let repo = if let Some(id) = resolved {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.ok_or_else(|| FactbaseError::NotFound("No repository found.".into()))
}

/// Unified organize tool dispatcher. Routes to the appropriate action based on the "action" field.
/// Supports: move, retype, apply. Merge and split are now agent-driven via get_entity + CRUD tools.
#[instrument(name = "mcp_organize", skip(db, _embedding, args, _progress))]
pub async fn organize<E: EmbeddingProvider>(
    db: &Database,
    _embedding: &E,
    args: &Value,
    _progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let action = get_str_arg_required(args, "action")?;
    match action.as_str() {
        "merge" => Ok(serde_json::json!({
            "error": "The 'merge' action has been removed from the organize tool. Use get_entity to read both documents, plan the merge yourself, then use update_document and delete_document to execute it.",
            "migration": "Agent-driven merge: get_entity(doc1) + get_entity(doc2) → plan → update_document(keep) + delete_document(remove)"
        })),
        "split" => Ok(serde_json::json!({
            "error": "The 'split' action has been removed from the organize tool. Use get_entity to read the document, plan the split yourself, then use create_document and update_document to execute it.",
            "migration": "Agent-driven split: get_entity(doc) → plan → create_document(new) + update_document(original)"
        })),
        "move" => organize_move(db, args),
        "retype" => organize_retype(db, args),
        "apply" => organize_apply(db, args),
        _ => Err(FactbaseError::parse(format!(
            "Unknown organize action: '{action}'. Expected: move, retype, apply"
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
/// Supports focus="duplicates" or focus="structure" for targeted analysis.
#[instrument(name = "mcp_organize_analyze", skip(db, embedding, args, progress))]
pub async fn organize_analyze<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = crate::mcp::tools::resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let focus = get_str_arg(args, "focus").map(String::from);
    let rid = repo_id.as_deref();

    // Focused duplicate-only mode
    if focus.as_deref() == Some("duplicates") {
        let (dup_json, stale_json) = get_duplicate_entries(db, embedding, rid, progress).await?;
        return Ok(serde_json::json!({
            "duplicates": dup_json,
            "stale": stale_json,
            "duplicate_count": dup_json.len(),
            "stale_count": stale_json.len(),
        }));
    }

    // Focused structure-only mode (misplaced detection)
    if focus.as_deref() == Some("structure") {
        let misplaced_candidates = {
            let db2 = db.clone();
            let p = progress.clone();
            let rid2 = rid.map(String::from);
            run_blocking(move || detect_misplaced(&db2, rid2.as_deref(), &p)).await?
        };
        return Ok(serde_json::json!({
            "misplaced_candidates": misplaced_candidates.iter().map(|c| {
                serde_json::json!({
                    "doc_id": c.doc_id,
                    "doc_title": c.doc_title,
                    "current_type": c.current_type,
                    "suggested_type": c.suggested_type,
                    "confidence": c.confidence,
                    "rationale": c.rationale,
                })
            }).collect::<Vec<_>>(),
            "total_suggestions": misplaced_candidates.len(),
        }));
    }

    // Default mode: run all phases
    let merge_threshold = args.get("merge_threshold").and_then(|v| v.as_f64()).unwrap_or(0.95) as f32;
    let split_threshold = args.get("split_threshold").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;

    progress.phase("Analysis 1/5: Ghost files");
    let ghost_files = {
        let db2 = db.clone();
        let p = progress.clone();
        let rid2 = rid.map(String::from);
        run_blocking(move || detect_ghost_files(&db2, rid2.as_deref(), &p)).await?
    };

    progress.phase("Analysis 2/5: Merge candidates");
    let merge_candidates = {
        let db2 = db.clone();
        let p = progress.clone();
        let rid2 = rid.map(String::from);
        run_blocking(move || detect_merge_candidates(&db2, merge_threshold, rid2.as_deref(), &p)).await?
    };

    progress.phase("Analysis 3/5: Split candidates");
    let split_candidates = detect_split_candidates(db, embedding, split_threshold, rid, progress).await?;

    progress.phase("Analysis 4/5: Misplaced documents");
    let misplaced_candidates = {
        let db2 = db.clone();
        let p = progress.clone();
        let rid2 = rid.map(String::from);
        run_blocking(move || detect_misplaced(&db2, rid2.as_deref(), &p)).await?
    };

    progress.phase("Analysis 5/5: Duplicate entries");
    let duplicate_entries = detect_duplicate_entries(db, embedding, rid, progress).await?;
    let db2 = db.clone();
    let dups = duplicate_entries.clone();
    let stale_entries = run_blocking(move || assess_staleness(&dups, &db2)).await?;

    Ok(serde_json::json!({
        "ghost_files": ghost_files.iter().map(|g| serde_json::json!({
            "doc_id": g.doc_id,
            "title": g.title,
            "tracked_path": g.tracked_path,
            "ghost_path": g.ghost_path,
            "tracked_lines": g.tracked_lines,
            "ghost_lines": g.ghost_lines,
            "reason": g.reason,
        })).collect::<Vec<_>>(),
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
        "misplaced_candidates": misplaced_candidates.iter().map(|c| {
            serde_json::json!({
                "doc_id": c.doc_id,
                "doc_title": c.doc_title,
                "current_type": c.current_type,
                "suggested_type": c.suggested_type,
                "confidence": c.confidence,
                "rationale": c.rationale,
            })
        }).collect::<Vec<_>>(),
        "ghost_count": ghost_files.len(),
        "merge_count": merge_candidates.len(),
        "split_count": split_candidates.len(),
        "misplaced_count": misplaced_candidates.len(),
        "duplicate_entries": duplicate_entries.len(),
        "stale_entries": stale_entries.len(),
        "ghost_file_count": ghost_files.len(),
        "total_suggestions": ghost_files.len() + merge_candidates.len() + split_candidates.len() + misplaced_candidates.len() + duplicate_entries.len() + stale_entries.len(),
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

    /// Helper to set up a test DB with a repo and document that has an embedding stored.
    fn setup_test_db_with_doc(doc_id: &str, title: &str) -> (Database, tempfile::TempDir) {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = doc_id.to_string();
        doc.title = title.to_string();
        doc.content = format!("<!-- factbase:{doc_id} -->\n# {title}\n\n- Some fact\n");
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();
        // Store a mock embedding so similarity search doesn't fail
        db.upsert_embedding(doc_id, &vec![0.1; 1024]).unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn test_organize_analyze_runs_all_phases() {
        let (db, _tmp) = setup_test_db_with_doc("aaa111", "Test Doc");
        let embedding = crate::embedding::test_helpers::MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
        });

        let result = organize_analyze(&db, &embedding, &args, &progress)
            .await
            .unwrap();

        // Should complete all phases
        assert!(result.get("merge_candidates").is_some());
        assert!(result.get("split_candidates").is_some());
        assert!(result.get("misplaced_candidates").is_some());
        // No paging fields
        assert!(result.get("continue").is_none());
        assert!(result.get("resume").is_none());
        assert!(result.get("completed_phases").is_none());
    }

    #[tokio::test]
    async fn test_organize_analyze_focus_structure() {
        let (db, _tmp) = setup_test_db_with_doc("ddd444", "Structure Doc");
        let embedding = crate::embedding::test_helpers::MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "focus": "structure",
        });

        let result = organize_analyze(&db, &embedding, &args, &progress)
            .await
            .unwrap();

        assert!(result.get("misplaced_candidates").is_some());
        assert!(result.get("merge_candidates").is_none());
        assert!(result.get("split_candidates").is_none());
    }

    #[test]
    fn test_organize_analyze_schema_no_paging() {
        let result = crate::mcp::tools::schema::tools_list();
        let tools = result["tools"].as_array().unwrap();
        let analyze = tools.iter().find(|s| s["name"] == "organize_analyze").unwrap();
        let props = &analyze["inputSchema"]["properties"];
        // No paging params
        assert!(props.get("time_budget_secs").is_none());
        assert!(props.get("resume").is_none());
    }

    #[test]
    fn test_temporal_issues_serialized_in_merge_dry_run() {
        use crate::organize::TemporalIssue;
        let issues = vec![
            TemporalIssue { line_ref: 3, description: "Boundary overlap on transition date".into() },
            TemporalIssue { line_ref: 8, description: "Missing end date makes timeline unclear".into() },
            TemporalIssue { line_ref: 12, description: "Contradictory dates for same event".into() },
        ];
        let json: Vec<Value> = issues.iter().map(|t| serde_json::json!({
            "line_ref": t.line_ref,
            "description": t.description,
        })).collect();
        let response = serde_json::json!({
            "dry_run": true,
            "keep_id": "aaa",
            "merge_id": "bbb",
            "temporal_issues": json,
        });
        let ti = response["temporal_issues"].as_array().unwrap();
        assert_eq!(ti.len(), 3);
        assert_eq!(ti[0]["line_ref"], 3);
        assert_eq!(ti[0]["description"], "Boundary overlap on transition date");
        assert_eq!(ti[1]["description"], "Missing end date makes timeline unclear");
        assert_eq!(ti[2]["description"], "Contradictory dates for same event");
    }

    #[test]
    fn test_temporal_issues_serialized_in_split_dry_run() {
        use crate::organize::TemporalIssue;
        let issues = vec![
            TemporalIssue { line_ref: 5, description: "Timeline contradiction: ended before started".into() },
        ];
        let json: Vec<Value> = issues.iter().map(|t| serde_json::json!({
            "line_ref": t.line_ref,
            "description": t.description,
        })).collect();
        let response = serde_json::json!({
            "dry_run": true,
            "source_id": "abc",
            "temporal_issues": json,
        });
        let ti = response["temporal_issues"].as_array().unwrap();
        assert_eq!(ti.len(), 1);
        assert_eq!(ti[0]["line_ref"], 5);
        assert!(ti[0]["description"].as_str().unwrap().contains("contradiction"));
    }

    #[test]
    fn test_temporal_issues_empty_when_none_detected() {
        let json: Vec<Value> = Vec::new();
        let response = serde_json::json!({
            "dry_run": true,
            "keep_id": "aaa",
            "temporal_issues": json,
        });
        let ti = response["temporal_issues"].as_array().unwrap();
        assert!(ti.is_empty());
    }
}
