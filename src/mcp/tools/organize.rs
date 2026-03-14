//! Organize-related MCP tools.

use std::path::Path;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{
    get_bool_arg, get_str_arg, get_str_arg_required, resolve_repo, run_blocking,
};
use crate::organize::{
    assess_staleness, detect_duplicate_entries, detect_ghost_files, detect_merge_candidates,
    detect_misplaced, detect_split_candidates, execute_move, execute_retype,
    process_orphan_answers,
};
use crate::processor::DocumentProcessor;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Unified organize tool dispatcher. Routes to the appropriate action based on the "action" field.
#[instrument(name = "mcp_organize", skip(db, _embedding, args, _progress))]
pub async fn organize<E: EmbeddingProvider>(
    db: &Database,
    _embedding: &E,
    args: &Value,
    _progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let action = get_str_arg_required(args, "action")?;
    match action.as_str() {
        "merge" => organize_merge(db, args),
        "split" => organize_split(db, args),
        "delete" => organize_delete(db, args),
        "move" => organize_move(db, args),
        "retype" => organize_retype(db, args),
        "apply" => organize_apply(db, args),
        "execute_suggestions" => organize_execute_suggestions(db, args),
        _ => Err(FactbaseError::parse(format!(
            "Unknown organize action: '{action}'. Expected: merge, split, delete, move, retype, apply, execute_suggestions"
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
    let merge_threshold = args
        .get("merge_threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.95) as f32;
    let split_threshold = args
        .get("split_threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;

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
        run_blocking(move || detect_merge_candidates(&db2, merge_threshold, rid2.as_deref(), &p))
            .await?
    };

    progress.phase("Analysis 3/5: Split candidates");
    let split_candidates =
        detect_split_candidates(db, embedding, split_threshold, rid, progress).await?;

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

/// Merge source document into target: append unique content, redirect links, delete source.
#[instrument(name = "mcp_organize_merge", skip(db, args))]
fn organize_merge(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let source_id = get_str_arg_required(args, "source_id")?;
    let target_id = get_str_arg_required(args, "target_id")?;

    if source_id == target_id {
        return Err(FactbaseError::parse(
            "source_id and target_id must be different",
        ));
    }

    let source = db.require_document(&source_id)?;
    let target = db.require_document(&target_id)?;
    let repo = resolve_repo(db, Some(target.repo_id.as_str()))?;

    let source_path = repo.path.join(&source.file_path);
    let target_path = repo.path.join(&target.file_path);

    if !target_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "Target file not found: {}",
            target_path.display()
        )));
    }

    // Read target content from disk (authoritative)
    let target_content = std::fs::read_to_string(&target_path)?;

    // Extract source body (strip factbase header + title)
    let source_body = crate::mcp::tools::document::strip_factbase_header(&source.content);

    // Append source body to target
    let mut merged = target_content.clone();
    if !merged.ends_with('\n') {
        merged.push('\n');
    }
    merged.push_str(&format!(
        "\n## Merged from {}\n\n{}\n",
        source.title, source_body
    ));

    // Write merged content
    std::fs::write(&target_path, &merged)?;

    // Redirect links from source to target
    let links_redirected = crate::organize::redirect_links(db, &source_id, &target_id, &repo.path)?;

    // Delete source file and mark deleted
    if source_path.exists() {
        std::fs::remove_file(&source_path)?;
    }
    db.mark_deleted(&source_id)?;

    Ok(serde_json::json!({
        "source_id": source_id,
        "source_title": source.title,
        "target_id": target_id,
        "target_title": target.title,
        "links_redirected": links_redirected,
        "message": format!("Merged '{}' into '{}'. {} links redirected. Source deleted. Run scan to re-index.",
            source.title, target.title, links_redirected)
    }))
}

/// Split a document into multiple new documents. Agent provides sections.
#[instrument(name = "mcp_organize_split", skip(db, args))]
fn organize_split(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let sections = args
        .get("sections")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            FactbaseError::parse("sections array is required (each with 'title' and 'content')")
        })?;

    if sections.is_empty() {
        return Err(FactbaseError::parse("sections array cannot be empty"));
    }

    let doc = db.require_document(&doc_id)?;
    let repo = resolve_repo(db, Some(doc.repo_id.as_str()))?;
    let doc_path = repo.path.join(&doc.file_path);
    let parent_dir = doc_path
        .parent()
        .ok_or_else(|| FactbaseError::internal("Document has no parent directory"))?;

    let processor = DocumentProcessor::new();
    let mut new_docs = Vec::new();

    for section in sections {
        let title = section
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse("Each section requires a 'title'"))?;
        let content = section
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse("Each section requires 'content'"))?;

        let new_id = processor.generate_unique_id(db);
        let safe_name = title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim()
            .replace(' ', "-")
            .to_lowercase();
        let new_path = parent_dir.join(format!("{safe_name}.md"));

        let full_content = format!("<!-- factbase:{new_id} -->\n# {title}\n\n{content}\n");
        if let Some(p) = new_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&new_path, &full_content)?;

        new_docs.push(serde_json::json!({
            "id": new_id,
            "title": title,
            "file_path": new_path.strip_prefix(&repo.path).unwrap_or(&new_path).display().to_string(),
        }));
    }

    // Delete original
    if doc_path.exists() {
        std::fs::remove_file(&doc_path)?;
    }
    db.mark_deleted(&doc_id)?;

    Ok(serde_json::json!({
        "source_id": doc_id,
        "source_title": doc.title,
        "new_documents": new_docs,
        "message": format!("Split '{}' into {} documents. Source deleted. Run scan to re-index.",
            doc.title, new_docs.len())
    }))
}

/// Clean delete: remove file, DB entries (documents, links, embeddings), update wikilinks.
#[instrument(name = "mcp_organize_delete", skip(db, args))]
fn organize_delete(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;

    let doc = db.require_document(&doc_id)?;
    let repo = resolve_repo(db, Some(doc.repo_id.as_str()))?;
    let file_path = repo.path.join(&doc.file_path);

    // Find incoming links before deletion
    let incoming_links = db.get_links_to(&doc_id)?;
    let links_affected = incoming_links.len();

    // Remove [[id]] references from files that link to this doc
    let mut wikilinks_cleaned = 0;
    for link in &incoming_links {
        if let Some(source_doc) = db.get_document(&link.source_id)? {
            let source_path = repo.path.join(&source_doc.file_path);
            if source_path.exists() {
                let content = std::fs::read_to_string(&source_path)?;
                let pattern = format!("[[{}]]", doc_id);
                if content.contains(&pattern) {
                    let cleaned = content.replace(&pattern, &doc.title);
                    std::fs::write(&source_path, &cleaned)?;
                    wikilinks_cleaned += 1;
                }
            }
        }
    }

    // Delete file from disk
    if file_path.exists() {
        std::fs::remove_file(&file_path)?;
    }

    // Hard delete from DB (documents, links, embeddings, facts)
    db.hard_delete_document(&doc_id)?;

    Ok(serde_json::json!({
        "doc_id": doc_id,
        "title": doc.title,
        "file_path": doc.file_path,
        "links_affected": links_affected,
        "wikilinks_cleaned": wikilinks_cleaned,
        "message": format!("Deleted '{}' ({}). File removed, DB cleaned. {} incoming links affected, {} wikilinks updated.",
            doc.title, doc_id, links_affected, wikilinks_cleaned)
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
    let filename = old_path
        .file_name()
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

#[instrument(name = "mcp_organize_execute_suggestions", skip(db, args))]
fn organize_execute_suggestions(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo").map(String::from);
    let dry_run = get_bool_arg(args, "dry_run", false);

    let result = crate::organize::execute_suggestions(db, repo_id.as_deref(), dry_run)?;

    let moves: Vec<Value> = result.moves.iter().map(|m| serde_json::json!({
        "doc_id": m.doc_id, "old_path": m.old_path, "new_path": m.new_path, "links_updated": m.links_updated,
    })).collect();
    let renames: Vec<Value> = result.renames.iter().map(|r| serde_json::json!({
        "doc_id": r.doc_id, "old_path": r.old_path, "new_path": r.new_path, "links_updated": r.links_updated,
    })).collect();
    let title_changes: Vec<Value> = result.title_changes.iter().map(|t| serde_json::json!({
        "doc_id": t.doc_id, "old_title": t.old_title, "new_title": t.new_title, "links_updated": t.links_updated,
    })).collect();

    let total = moves.len() + renames.len() + title_changes.len();

    Ok(serde_json::json!({
        "dry_run": dry_run,
        "moves": moves,
        "renames": renames,
        "title_changes": title_changes,
        "errors": result.errors,
        "message": if dry_run {
            format!("{total} suggestion(s) would be executed ({} moves, {} renames, {} title changes)",
                moves.len(), renames.len(), title_changes.len())
        } else {
            format!("Executed {total} suggestion(s): {} moves, {} renames, {} title changes. Run scan to re-index.",
                moves.len(), renames.len(), title_changes.len())
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
        // organize op in factbase tool should not have its own paging params
        // (time_budget_secs and resume are for scan/detect_links ops)
        let result = crate::mcp::tools::schema::tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|s| s["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        // organize op should be mentioned in the compact description
        assert!(desc.contains("ORGANIZE:"));
    }

    #[test]
    fn test_temporal_issues_serialized_in_merge_dry_run() {
        use crate::organize::TemporalIssue;
        let issues = vec![
            TemporalIssue {
                line_ref: 3,
                description: "Boundary overlap on transition date".into(),
            },
            TemporalIssue {
                line_ref: 8,
                description: "Missing end date makes timeline unclear".into(),
            },
            TemporalIssue {
                line_ref: 12,
                description: "Contradictory dates for same event".into(),
            },
        ];
        let json: Vec<Value> = issues
            .iter()
            .map(|t| {
                serde_json::json!({
                    "line_ref": t.line_ref,
                    "description": t.description,
                })
            })
            .collect();
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
        assert_eq!(
            ti[1]["description"],
            "Missing end date makes timeline unclear"
        );
        assert_eq!(ti[2]["description"], "Contradictory dates for same event");
    }

    #[test]
    fn test_temporal_issues_serialized_in_split_dry_run() {
        use crate::organize::TemporalIssue;
        let issues = vec![TemporalIssue {
            line_ref: 5,
            description: "Timeline contradiction: ended before started".into(),
        }];
        let json: Vec<Value> = issues
            .iter()
            .map(|t| {
                serde_json::json!({
                    "line_ref": t.line_ref,
                    "description": t.description,
                })
            })
            .collect();
        let response = serde_json::json!({
            "dry_run": true,
            "source_id": "abc",
            "temporal_issues": json,
        });
        let ti = response["temporal_issues"].as_array().unwrap();
        assert_eq!(ti.len(), 1);
        assert_eq!(ti[0]["line_ref"], 5);
        assert!(ti[0]["description"]
            .as_str()
            .unwrap()
            .contains("contradiction"));
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

    #[test]
    fn test_update_with_suggested_move_stores_suggestion() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // Create a file on disk
        let file_path = tmp.path().join("doc.md");
        std::fs::write(&file_path, "<!-- factbase:aaa111 -->\n# Test\n\nContent").unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test".to_string();
        doc.content = "<!-- factbase:aaa111 -->\n# Test\n\nContent".to_string();
        doc.file_path = "doc.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "id": "aaa111",
            "suggested_move": "people/",
        });
        let result = crate::mcp::tools::document::update_document(&db, &args).unwrap();
        assert!(result["suggestions_stored"]
            .as_array()
            .unwrap()
            .contains(&Value::String("move".into())));

        let suggestions = db.list_suggestions(None).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].suggestion_type, "move");
        assert_eq!(suggestions[0].suggested_value, "people/");
    }

    #[test]
    fn test_execute_suggestions_move_with_link_cascade() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // Create source file
        let src_dir = tmp.path().join("old");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("target.md"),
            "<!-- factbase:tgt001 -->\n# Target\n\nContent",
        )
        .unwrap();

        // Create a referencing file with wikilink
        std::fs::write(
            tmp.path().join("ref.md"),
            "<!-- factbase:ref001 -->\n# Ref\n\nSee [[old/target|Target]] for details.",
        )
        .unwrap();

        let mut target_doc = crate::models::Document::test_default();
        target_doc.id = "tgt001".to_string();
        target_doc.title = "Target".to_string();
        target_doc.content = "<!-- factbase:tgt001 -->\n# Target\n\nContent".to_string();
        target_doc.file_path = "old/target.md".to_string();
        target_doc.repo_id = "test".to_string();
        db.upsert_document(&target_doc).unwrap();

        let mut ref_doc = crate::models::Document::test_default();
        ref_doc.id = "ref001".to_string();
        ref_doc.title = "Ref".to_string();
        ref_doc.content =
            "<!-- factbase:ref001 -->\n# Ref\n\nSee [[old/target|Target]] for details.".to_string();
        ref_doc.file_path = "ref.md".to_string();
        ref_doc.repo_id = "test".to_string();
        db.upsert_document(&ref_doc).unwrap();

        // Insert move suggestion
        db.insert_suggestion("tgt001", "move", "new/", "update")
            .unwrap();

        // Execute
        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        let moves = result["moves"].as_array().unwrap();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["old_path"], "old/target.md");
        assert_eq!(moves[0]["new_path"], "new/target.md");
        assert_eq!(moves[0]["links_updated"], 1);

        // Verify file was moved
        assert!(!src_dir.join("target.md").exists());
        assert!(tmp.path().join("new/target.md").exists());

        // Verify wikilink was updated in referencing file
        let ref_content = std::fs::read_to_string(tmp.path().join("ref.md")).unwrap();
        assert!(ref_content.contains("[[new/target|Target]]"));
        assert!(!ref_content.contains("[[old/target|Target]]"));

        // Verify suggestions were consumed
        assert!(db.list_suggestions(None).unwrap().is_empty());
    }

    #[test]
    fn test_execute_suggestions_rename_with_link_cascade() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("old-name.md"),
            "<!-- factbase:ren001 -->\n# Entity\n\nContent",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("other.md"),
            "<!-- factbase:oth001 -->\n# Other\n\nSee [[old-name|Entity]].",
        )
        .unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "ren001".to_string();
        doc.title = "Entity".to_string();
        doc.content = "<!-- factbase:ren001 -->\n# Entity\n\nContent".to_string();
        doc.file_path = "old-name.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let mut other = crate::models::Document::test_default();
        other.id = "oth001".to_string();
        other.title = "Other".to_string();
        other.content = "<!-- factbase:oth001 -->\n# Other\n\nSee [[old-name|Entity]].".to_string();
        other.file_path = "other.md".to_string();
        other.repo_id = "test".to_string();
        db.upsert_document(&other).unwrap();

        db.insert_suggestion("ren001", "rename", "new-name.md", "update")
            .unwrap();

        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        let renames = result["renames"].as_array().unwrap();
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0]["new_path"], "new-name.md");

        assert!(!tmp.path().join("old-name.md").exists());
        assert!(tmp.path().join("new-name.md").exists());

        let other_content = std::fs::read_to_string(tmp.path().join("other.md")).unwrap();
        assert!(other_content.contains("[[new-name|Entity]]"));
    }

    #[test]
    fn test_execute_suggestions_title_change_updates_display() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("entity.md"),
            "<!-- factbase:ent001 -->\n# Old Name\n\nContent",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("ref.md"),
            "<!-- factbase:ref001 -->\n# Ref\n\nSee [[entity|Old Name]].",
        )
        .unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "ent001".to_string();
        doc.title = "Old Name".to_string();
        doc.content = "<!-- factbase:ent001 -->\n# Old Name\n\nContent".to_string();
        doc.file_path = "entity.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let mut ref_doc = crate::models::Document::test_default();
        ref_doc.id = "ref001".to_string();
        ref_doc.title = "Ref".to_string();
        ref_doc.content = "<!-- factbase:ref001 -->\n# Ref\n\nSee [[entity|Old Name]].".to_string();
        ref_doc.file_path = "ref.md".to_string();
        ref_doc.repo_id = "test".to_string();
        db.upsert_document(&ref_doc).unwrap();

        db.insert_suggestion("ent001", "title", "New Name", "update")
            .unwrap();

        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        let titles = result["title_changes"].as_array().unwrap();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0]["old_title"], "Old Name");
        assert_eq!(titles[0]["new_title"], "New Name");

        let ref_content = std::fs::read_to_string(tmp.path().join("ref.md")).unwrap();
        assert!(ref_content.contains("[[entity|New Name]]"));
        assert!(!ref_content.contains("[[entity|Old Name]]"));
    }

    #[test]
    fn test_execute_suggestions_dry_run() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("doc.md"),
            "<!-- factbase:dry001 -->\n# Doc\n\nContent",
        )
        .unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "dry001".to_string();
        doc.title = "Doc".to_string();
        doc.content = "<!-- factbase:dry001 -->\n# Doc\n\nContent".to_string();
        doc.file_path = "doc.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        db.insert_suggestion("dry001", "move", "archive/", "update")
            .unwrap();

        let args =
            serde_json::json!({"action": "execute_suggestions", "repo": "test", "dry_run": true});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        assert_eq!(result["dry_run"], true);
        assert_eq!(result["moves"].as_array().unwrap().len(), 1);

        // File should NOT have moved
        assert!(tmp.path().join("doc.md").exists());
        // Suggestion should NOT have been consumed
        assert_eq!(db.list_suggestions(None).unwrap().len(), 1);
    }

    #[test]
    fn test_execute_suggestions_deleted_doc_cleaned_up() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // Create doc, add suggestion, then delete the doc
        let mut doc = crate::models::Document::test_default();
        doc.id = "del001".to_string();
        doc.title = "Will Delete".to_string();
        doc.content = "<!-- factbase:del001 -->\n# Will Delete\n\nContent".to_string();
        doc.file_path = "del.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        db.insert_suggestion("del001", "move", "new/", "update")
            .unwrap();

        // Soft-delete the document
        db.mark_deleted("del001").unwrap();

        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        // Should succeed with no actions (doc deleted → suggestion cleaned up)
        assert_eq!(result["moves"].as_array().unwrap().len(), 0);
    }

    // --- Tests for merge, split, delete ---

    fn setup_two_docs(db: &Database, tmp: &std::path::Path) {
        crate::database::tests::test_repo_in_db(db, "test", tmp);

        std::fs::write(
            tmp.join("source.md"),
            "<!-- factbase:src001 -->\n# Source Doc\n\n- Source fact A\n- Source fact B\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join("target.md"),
            "<!-- factbase:tgt001 -->\n# Target Doc\n\n- Target fact X\n",
        )
        .unwrap();

        let mut src = crate::models::Document::test_default();
        src.id = "src001".to_string();
        src.title = "Source Doc".to_string();
        src.content =
            "<!-- factbase:src001 -->\n# Source Doc\n\n- Source fact A\n- Source fact B\n"
                .to_string();
        src.file_path = "source.md".to_string();
        src.repo_id = "test".to_string();
        db.upsert_document(&src).unwrap();

        let mut tgt = crate::models::Document::test_default();
        tgt.id = "tgt001".to_string();
        tgt.title = "Target Doc".to_string();
        tgt.content = "<!-- factbase:tgt001 -->\n# Target Doc\n\n- Target fact X\n".to_string();
        tgt.file_path = "target.md".to_string();
        tgt.repo_id = "test".to_string();
        db.upsert_document(&tgt).unwrap();
    }

    #[test]
    fn test_organize_merge_appends_source_to_target() {
        let (db, tmp) = crate::database::tests::test_db();
        setup_two_docs(&db, tmp.path());

        let args =
            serde_json::json!({"action": "merge", "source_id": "src001", "target_id": "tgt001"});
        let result = organize_merge(&db, &args).unwrap();

        assert_eq!(result["source_id"], "src001");
        assert_eq!(result["target_id"], "tgt001");

        // Source file deleted
        assert!(!tmp.path().join("source.md").exists());
        // Target file has merged content
        let content = std::fs::read_to_string(tmp.path().join("target.md")).unwrap();
        assert!(content.contains("Target fact X"));
        assert!(content.contains("Source fact A"));
        assert!(content.contains("Source fact B"));
        assert!(content.contains("Merged from Source Doc"));
    }

    #[test]
    fn test_organize_merge_with_duplicate_facts() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // Both docs have "Shared fact"
        std::fs::write(
            tmp.path().join("a.md"),
            "<!-- factbase:aaa001 -->\n# Doc A\n\n- Shared fact\n- Unique A\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("b.md"),
            "<!-- factbase:bbb001 -->\n# Doc B\n\n- Shared fact\n- Unique B\n",
        )
        .unwrap();

        let mut a = crate::models::Document::test_default();
        a.id = "aaa001".to_string();
        a.title = "Doc A".to_string();
        a.content = "<!-- factbase:aaa001 -->\n# Doc A\n\n- Shared fact\n- Unique A\n".to_string();
        a.file_path = "a.md".to_string();
        a.repo_id = "test".to_string();
        db.upsert_document(&a).unwrap();

        let mut b = crate::models::Document::test_default();
        b.id = "bbb001".to_string();
        b.title = "Doc B".to_string();
        b.content = "<!-- factbase:bbb001 -->\n# Doc B\n\n- Shared fact\n- Unique B\n".to_string();
        b.file_path = "b.md".to_string();
        b.repo_id = "test".to_string();
        db.upsert_document(&b).unwrap();

        let args =
            serde_json::json!({"action": "merge", "source_id": "aaa001", "target_id": "bbb001"});
        let result = organize_merge(&db, &args).unwrap();
        assert_eq!(result["source_id"], "aaa001");

        // Merge appends all source body (dedup is agent's job)
        let content = std::fs::read_to_string(tmp.path().join("b.md")).unwrap();
        assert!(content.contains("Unique A"));
        assert!(content.contains("Unique B"));
    }

    #[test]
    fn test_organize_merge_redirects_links() {
        let (db, tmp) = crate::database::tests::test_db();
        setup_two_docs(&db, tmp.path());

        // Create a third doc that links to source
        std::fs::write(
            tmp.path().join("ref.md"),
            "<!-- factbase:ref001 -->\n# Ref\n\nSee [[src001]] for details.",
        )
        .unwrap();
        let mut r = crate::models::Document::test_default();
        r.id = "ref001".to_string();
        r.title = "Ref".to_string();
        r.content = "<!-- factbase:ref001 -->\n# Ref\n\nSee [[src001]] for details.".to_string();
        r.file_path = "ref.md".to_string();
        r.repo_id = "test".to_string();
        db.upsert_document(&r).unwrap();

        db.update_links(
            "ref001",
            &[crate::link_detection::DetectedLink {
                target_id: "src001".to_string(),
                target_title: "Source Doc".to_string(),
                mention_text: "Source Doc".to_string(),
                context: "references".to_string(),
            }],
        )
        .unwrap();

        let args =
            serde_json::json!({"action": "merge", "source_id": "src001", "target_id": "tgt001"});
        let result = organize_merge(&db, &args).unwrap();
        assert!(result["links_redirected"].as_u64().unwrap() >= 1);

        // DB link redirected
        let links = db.get_links_from("ref001").unwrap();
        assert_eq!(links[0].target_id, "tgt001");
    }

    #[test]
    fn test_organize_merge_same_id_rejected() {
        let (db, _tmp) = crate::database::tests::test_db();
        let args = serde_json::json!({"action": "merge", "source_id": "abc", "target_id": "abc"});
        assert!(organize_merge(&db, &args).is_err());
    }

    #[test]
    fn test_organize_delete_cleans_up() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("victim.md"),
            "<!-- factbase:vic001 -->\n# Victim\n\n- Some fact\n",
        )
        .unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "vic001".to_string();
        doc.title = "Victim".to_string();
        doc.content = "<!-- factbase:vic001 -->\n# Victim\n\n- Some fact\n".to_string();
        doc.file_path = "victim.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({"action": "delete", "doc_id": "vic001"});
        let result = organize_delete(&db, &args).unwrap();

        assert_eq!(result["doc_id"], "vic001");
        assert_eq!(result["title"], "Victim");

        // File deleted
        assert!(!tmp.path().join("victim.md").exists());
        // DB hard-deleted
        assert!(db.get_document("vic001").unwrap().is_none());
    }

    #[test]
    fn test_organize_delete_cleans_wikilinks() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("target.md"),
            "<!-- factbase:del001 -->\n# Target\n\n- Fact\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("ref.md"),
            "<!-- factbase:ref001 -->\n# Ref\n\nSee [[del001]] here.",
        )
        .unwrap();

        let mut target = crate::models::Document::test_default();
        target.id = "del001".to_string();
        target.title = "Target".to_string();
        target.content = "<!-- factbase:del001 -->\n# Target\n\n- Fact\n".to_string();
        target.file_path = "target.md".to_string();
        target.repo_id = "test".to_string();
        db.upsert_document(&target).unwrap();

        let mut r = crate::models::Document::test_default();
        r.id = "ref001".to_string();
        r.title = "Ref".to_string();
        r.content = "<!-- factbase:ref001 -->\n# Ref\n\nSee [[del001]] here.".to_string();
        r.file_path = "ref.md".to_string();
        r.repo_id = "test".to_string();
        db.upsert_document(&r).unwrap();

        db.update_links(
            "ref001",
            &[crate::link_detection::DetectedLink {
                target_id: "del001".to_string(),
                target_title: "Target".to_string(),
                mention_text: "Target".to_string(),
                context: "references".to_string(),
            }],
        )
        .unwrap();

        let args = serde_json::json!({"action": "delete", "doc_id": "del001"});
        let result = organize_delete(&db, &args).unwrap();

        assert_eq!(result["links_affected"], 1);
        assert_eq!(result["wikilinks_cleaned"], 1);

        // Wikilink replaced with title text
        let ref_content = std::fs::read_to_string(tmp.path().join("ref.md")).unwrap();
        assert!(!ref_content.contains("[[del001]]"));
        assert!(ref_content.contains("Target"));
    }

    #[test]
    fn test_organize_split_creates_new_docs() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        std::fs::write(
            tmp.path().join("multi.md"),
            "<!-- factbase:mul001 -->\n# Multi Topic\n\n## Section A\n- Fact A\n\n## Section B\n- Fact B\n",
        ).unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "mul001".to_string();
        doc.title = "Multi Topic".to_string();
        doc.content = "<!-- factbase:mul001 -->\n# Multi Topic\n\n## Section A\n- Fact A\n\n## Section B\n- Fact B\n".to_string();
        doc.file_path = "multi.md".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "action": "split",
            "doc_id": "mul001",
            "sections": [
                {"title": "Section A", "content": "- Fact A"},
                {"title": "Section B", "content": "- Fact B"}
            ]
        });
        let result = organize_split(&db, &args).unwrap();

        assert_eq!(result["source_id"], "mul001");
        let new_docs = result["new_documents"].as_array().unwrap();
        assert_eq!(new_docs.len(), 2);

        // Original deleted
        assert!(!tmp.path().join("multi.md").exists());

        // New files created
        assert!(tmp.path().join("section-a.md").exists());
        assert!(tmp.path().join("section-b.md").exists());

        let a_content = std::fs::read_to_string(tmp.path().join("section-a.md")).unwrap();
        assert!(a_content.contains("# Section A"));
        assert!(a_content.contains("Fact A"));
        assert!(a_content.starts_with("<!-- factbase:"));
    }

    #[test]
    fn test_organize_split_empty_sections_rejected() {
        let (db, _tmp) = crate::database::tests::test_db();
        let args = serde_json::json!({"action": "split", "doc_id": "x", "sections": []});
        assert!(organize_split(&db, &args).is_err());
    }

    #[test]
    fn test_maintain_workflow_lists_organize_operations() {
        // Test via the workflow function output (step 6 = organize step)
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", _tmp.path());
        let args = serde_json::json!({"workflow": "maintain", "step": 5});
        let result = crate::mcp::tools::workflow::workflow(&db, &args).unwrap();
        let instruction = result["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("merge"),
            "should mention merge: {instruction}"
        );
        assert!(instruction.contains("split"), "should mention split");
        assert!(instruction.contains("delete"), "should mention delete");
        assert!(instruction.contains("move"), "should mention move");
        assert!(
            instruction.contains("execute_suggestions"),
            "should mention execute_suggestions"
        );
        assert!(
            instruction.contains("Do NOT use shell commands"),
            "should warn against shell commands"
        );
    }

    #[test]
    fn test_correct_workflow_mentions_execute_suggestions() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", _tmp.path());
        let args = serde_json::json!({"workflow": "correct", "step": 4, "correction": "test", "source": "test"});
        let result = crate::mcp::tools::workflow::workflow(&db, &args).unwrap();
        let instruction = result["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("execute_suggestions"),
            "should mention execute_suggestions: {instruction}"
        );
    }

    /// End-to-end test: 3 cross-referencing entities, full pipeline from
    /// update_document (storing suggestions) → execute_suggestions (applying them).
    ///
    /// KB layout:
    ///   entities/alpha.md  — references beta and gamma
    ///   entities/beta.md   — references alpha
    ///   entities/gamma.md  — references alpha and beta
    ///
    /// Suggestions applied:
    ///   alpha: suggested_move → "archive/"
    ///   beta:  suggested_rename → "beta-renamed.md"
    ///   gamma: suggested_title → "Gamma Prime"
    #[test]
    fn test_e2e_suggestions_three_cross_referencing_entities() {
        use crate::mcp::tools::document::update_document;
        use std::fs;

        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // --- Create files on disk ---
        let ent_dir = tmp.path().join("entities");
        fs::create_dir_all(&ent_dir).unwrap();

        // alpha references beta and gamma
        fs::write(
            ent_dir.join("alpha.md"),
            "<!-- factbase:aaa001 -->\n# Alpha\n\n- See [[entities/beta|Beta]] for details.\n- Also see [[entities/gamma|Gamma]].\n",
        ).unwrap();
        // beta references alpha
        fs::write(
            ent_dir.join("beta.md"),
            "<!-- factbase:bbb001 -->\n# Beta\n\n- Related to [[entities/alpha|Alpha]].\n",
        ).unwrap();
        // gamma references alpha and beta
        fs::write(
            ent_dir.join("gamma.md"),
            "<!-- factbase:ccc001 -->\n# Gamma\n\n- Linked to [[entities/alpha|Alpha]] and [[entities/beta|Beta]].\n",
        ).unwrap();

        // --- Seed DB ---
        let make_doc = |id: &str, title: &str, path: &str, content: &str| {
            let mut d = crate::models::Document::test_default();
            d.id = id.to_string();
            d.title = title.to_string();
            d.file_path = path.to_string();
            d.content = content.to_string();
            d.repo_id = "test".to_string();
            d
        };

        db.upsert_document(&make_doc(
            "aaa001", "Alpha", "entities/alpha.md",
            "<!-- factbase:aaa001 -->\n# Alpha\n\n- See [[entities/beta|Beta]] for details.\n- Also see [[entities/gamma|Gamma]].\n",
        )).unwrap();
        db.upsert_document(&make_doc(
            "bbb001", "Beta", "entities/beta.md",
            "<!-- factbase:bbb001 -->\n# Beta\n\n- Related to [[entities/alpha|Alpha]].\n",
        )).unwrap();
        db.upsert_document(&make_doc(
            "ccc001", "Gamma", "entities/gamma.md",
            "<!-- factbase:ccc001 -->\n# Gamma\n\n- Linked to [[entities/alpha|Alpha]] and [[entities/beta|Beta]].\n",
        )).unwrap();

        // --- Step 2: store suggestions via update_document ---
        // alpha: suggested_move to "archive/"
        let r = update_document(&db, &serde_json::json!({
            "id": "aaa001",
            "suggested_move": "archive/",
        })).unwrap();
        assert!(r["suggestions_stored"].as_array().unwrap().contains(&serde_json::json!("move")));

        // beta: suggested_rename to "beta-renamed.md"
        let r = update_document(&db, &serde_json::json!({
            "id": "bbb001",
            "suggested_rename": "beta-renamed.md",
        })).unwrap();
        assert!(r["suggestions_stored"].as_array().unwrap().contains(&serde_json::json!("rename")));

        // gamma: suggested_title to "Gamma Prime"
        let r = update_document(&db, &serde_json::json!({
            "id": "ccc001",
            "suggested_title": "Gamma Prime",
        })).unwrap();
        assert!(r["suggestions_stored"].as_array().unwrap().contains(&serde_json::json!("title")));

        // --- Step 3: verify suggestions stored in DB ---
        let suggestions = db.list_suggestions(Some("test")).unwrap();
        assert_eq!(suggestions.len(), 3, "expected 3 pending suggestions");
        assert!(suggestions.iter().any(|s| s.doc_id == "aaa001" && s.suggestion_type == "move"));
        assert!(suggestions.iter().any(|s| s.doc_id == "bbb001" && s.suggestion_type == "rename"));
        assert!(suggestions.iter().any(|s| s.doc_id == "ccc001" && s.suggestion_type == "title"));

        // --- Step 4: execute suggestions ---
        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        // --- Step 5: verify outcomes ---

        // Move: alpha moved to archive/
        let moves = result["moves"].as_array().unwrap();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["old_path"], "entities/alpha.md");
        assert_eq!(moves[0]["new_path"], "archive/alpha.md");
        // 2 wikilinks to alpha (in beta and gamma)
        assert_eq!(moves[0]["links_updated"].as_u64().unwrap(), 2);

        // Rename: beta renamed
        let renames = result["renames"].as_array().unwrap();
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0]["old_path"], "entities/beta.md");
        assert_eq!(renames[0]["new_path"], "entities/beta-renamed.md");

        // Title change: gamma title updated
        let title_changes = result["title_changes"].as_array().unwrap();
        assert_eq!(title_changes.len(), 1);
        assert_eq!(title_changes[0]["old_title"], "Gamma");
        assert_eq!(title_changes[0]["new_title"], "Gamma Prime");

        assert!(result["errors"].as_array().unwrap().is_empty(), "no errors expected");

        // File system checks
        assert!(!ent_dir.join("alpha.md").exists(), "alpha should be moved");
        assert!(tmp.path().join("archive/alpha.md").exists(), "alpha at new location");
        assert!(!ent_dir.join("beta.md").exists(), "beta should be renamed");
        assert!(ent_dir.join("beta-renamed.md").exists(), "beta at new name");
        assert!(ent_dir.join("gamma.md").exists(), "gamma file unchanged");

        // Wikilink cascade: beta and gamma should reference archive/alpha now
        let beta_content = fs::read_to_string(ent_dir.join("beta-renamed.md")).unwrap();
        assert!(beta_content.contains("[[archive/alpha|Alpha]]"), "beta wikilink updated: {beta_content}");
        assert!(!beta_content.contains("[[entities/alpha"), "old alpha link gone: {beta_content}");

        let gamma_content = fs::read_to_string(ent_dir.join("gamma.md")).unwrap();
        assert!(gamma_content.contains("[[archive/alpha|Alpha]]"), "gamma alpha link updated: {gamma_content}");
        assert!(!gamma_content.contains("[[entities/alpha"), "old alpha link gone: {gamma_content}");
        // gamma's beta link should also be updated (beta was renamed)
        assert!(gamma_content.contains("[[entities/beta-renamed|Beta]]"), "gamma beta link updated: {gamma_content}");
        assert!(!gamma_content.contains("[[entities/beta|Beta]]"), "old beta link gone: {gamma_content}");

        // Title cascade: gamma's wikilink display for gamma itself is updated in alpha
        // (alpha was moved, its content is now at archive/alpha.md)
        let alpha_content = fs::read_to_string(tmp.path().join("archive/alpha.md")).unwrap();
        // alpha's link to gamma should now show "Gamma Prime"
        assert!(alpha_content.contains("[[entities/gamma|Gamma Prime]]"), "alpha gamma link updated: {alpha_content}");
        assert!(!alpha_content.contains("[[entities/gamma|Gamma]]"), "old gamma display gone: {alpha_content}");

        // DB checks
        let alpha_db = db.get_document("aaa001").unwrap().unwrap();
        assert_eq!(alpha_db.file_path, "archive/alpha.md");

        let beta_db = db.get_document("bbb001").unwrap().unwrap();
        assert_eq!(beta_db.file_path, "entities/beta-renamed.md");

        let gamma_db = db.get_document("ccc001").unwrap().unwrap();
        assert_eq!(gamma_db.title, "Gamma Prime");

        // Suggestions consumed
        assert!(db.list_suggestions(None).unwrap().is_empty(), "all suggestions consumed");
    }

    /// Edge case: move destination already exists → auto-merge.
    /// Entity A is moved to a folder where a file with the same name already exists.
    #[test]
    fn test_e2e_suggestions_move_destination_exists_auto_merges() {
        use crate::mcp::tools::document::update_document;
        use std::fs;

        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        let src_dir = tmp.path().join("drafts");
        let dst_dir = tmp.path().join("final");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Source entity
        fs::write(
            src_dir.join("report.md"),
            "<!-- factbase:src001 -->\n# Draft Report\n\n- Draft fact A\n- Draft fact B\n",
        ).unwrap();
        // Destination entity (same filename, different folder)
        fs::write(
            dst_dir.join("report.md"),
            "<!-- factbase:dst001 -->\n# Final Report\n\n- Final fact X\n",
        ).unwrap();

        let mut src = crate::models::Document::test_default();
        src.id = "src001".to_string();
        src.title = "Draft Report".to_string();
        src.file_path = "drafts/report.md".to_string();
        src.content = "<!-- factbase:src001 -->\n# Draft Report\n\n- Draft fact A\n- Draft fact B\n".to_string();
        src.repo_id = "test".to_string();
        db.upsert_document(&src).unwrap();

        let mut dst = crate::models::Document::test_default();
        dst.id = "dst001".to_string();
        dst.title = "Final Report".to_string();
        dst.file_path = "final/report.md".to_string();
        dst.content = "<!-- factbase:dst001 -->\n# Final Report\n\n- Final fact X\n".to_string();
        dst.repo_id = "test".to_string();
        db.upsert_document(&dst).unwrap();

        // Store move suggestion via update_document
        update_document(&db, &serde_json::json!({
            "id": "src001",
            "suggested_move": "final/",
        })).unwrap();

        // Execute — destination exists, should auto-merge
        let args = serde_json::json!({"action": "execute_suggestions", "repo": "test"});
        let result = organize_execute_suggestions(&db, &args).unwrap();

        // Should report a move (merged), no errors
        let moves = result["moves"].as_array().unwrap();
        assert_eq!(moves.len(), 1, "one move action");
        assert!(
            moves[0]["new_path"].as_str().unwrap().contains("merged into"),
            "should indicate merge: {}",
            moves[0]["new_path"]
        );
        assert!(result["errors"].as_array().unwrap().is_empty(), "no errors: {:?}", result["errors"]);

        // Source file deleted
        assert!(!src_dir.join("report.md").exists(), "source deleted after merge");

        // Destination file has merged content
        let merged = fs::read_to_string(dst_dir.join("report.md")).unwrap();
        assert!(merged.contains("Final fact X"), "target content preserved");
        assert!(merged.contains("Draft fact A"), "source content merged in");
        assert!(merged.contains("Draft fact B"), "source content merged in");
        assert!(merged.contains("Merged from"), "merge header present");

        // Suggestions consumed
        assert!(db.list_suggestions(None).unwrap().is_empty(), "suggestion consumed");
    }
}
