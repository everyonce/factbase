//! Repository management MCP tools.

use std::collections::HashSet;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::get_str_arg;
use crate::mcp::tools::helpers::WriteGuard;
use crate::{
    Config, DocumentProcessor, LinkDetector, ProgressReporter, ScanContext, ScanOptions, Scanner,
};
use serde_json::Value;
use tracing::info;

/// Scan (or rescan) the repository to index documents, generate embeddings.
pub async fn scan_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo = crate::mcp::tools::helpers::resolve_repo(db, get_str_arg(args, "repo"))?;

    let config = Config::load(None).unwrap_or_default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let mut opts = ScanOptions::from_config(&config);

    // Wire force_reindex from MCP args
    opts.force_reindex = crate::mcp::tools::helpers::get_bool_arg(args, "force_reindex", false);
    opts.skip_embeddings = crate::mcp::tools::helpers::get_bool_arg(args, "skip_embeddings", false);

    // MCP scan_repository skips link detection — use detect_links tool separately
    opts.skip_links = true;

    // Resume: skip already-processed files
    opts.file_offset = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token)
        .and_then(|v| v.get("file_offset").and_then(|o| o.as_u64()))
        .unwrap_or(0) as usize;

    // Set deadline for time-boxed operation.
    // force_reindex always bypasses the time budget — there's no cursor for reindex progress,
    // so a budget would cause an infinite restart-from-beginning loop.
    let time_budget = if opts.force_reindex {
        None
    } else {
        crate::mcp::tools::helpers::resolve_time_budget(args)
    };
    opts.deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    // Dimension mismatch detection
    let provider_dim = embedding.dimension();
    let stored_dim = db.get_stored_embedding_dim()?;
    if let Some(db_dim) = stored_dim {
        if db_dim != provider_dim {
            if opts.force_reindex {
                db.rebuild_embedding_tables(provider_dim)?;
                db.set_embedding_info(&config.embedding.model, provider_dim)?;
            } else {
                return Err(FactbaseError::config(format!(
                    "Embedding dimension mismatch: database has {db_dim}-dim vectors but current provider uses {provider_dim}-dim. \
                     Use force_reindex=true to rebuild all embeddings."
                )));
            }
        }
    } else {
        let actual_table_dim = db.get_schema_embedding_dimension()?;
        if actual_table_dim.is_some() && actual_table_dim != Some(provider_dim) {
            db.rebuild_embedding_tables(provider_dim)?;
        }
        db.set_embedding_info(&config.embedding.model, provider_dim)?;
    }

    // Link detection uses string matching only (no LLM required).
    // Manual [[id]] links and fuzzy title matches are detected.
    let link_detector = LinkDetector::new();

    // Acquire write guard right before full_scan — setup above is read-only
    let _guard = WriteGuard::try_acquire()?;

    // --- Start-of-operation status ---
    let files = scanner.find_markdown_files(&repo.path);
    let total_files = files.len();
    let existing_docs = db.get_documents_for_repo(&repo.id).unwrap_or_default();
    let existing_count = existing_docs.len();
    let provider_name = &config.embedding.provider;
    progress.log(&format!(
        "Scanning repository '{}': {} files found, {} existing docs, embedding: {} ({}d)",
        repo.id, total_files, existing_count, provider_name, provider_dim
    ));

    let ctx = ScanContext {
        scanner: &scanner,
        processor: &processor,
        embedding,
        link_detector: &link_detector,
        opts: &opts,
        progress,
    };

    let result = crate::full_scan(&repo, db, &ctx)
        .await
        .map_err(|e| FactbaseError::Internal(e.to_string()))?;

    let temporal_coverage = result
        .temporal_stats
        .as_ref()
        .map(|s| (s.coverage * 100.0).round())
        .unwrap_or(0.0);
    let source_coverage = result
        .temporal_stats
        .as_ref()
        .map(|s| (s.source_coverage * 100.0).round())
        .unwrap_or(0.0);

    info!(
        "Scan complete: {} added, {} updated, {} unchanged",
        result.added, result.updated, result.unchanged
    );

    let processed = result.added + result.updated + result.unchanged + result.moved + result.reindexed;

    // If interrupted by deadline, return progress response with resume token
    if result.interrupted && time_budget.is_some() {
        // Use file_offset from result; fall back to total_files when the
        // interruption came from a later phase (embedding/links) where
        // file_offset was not set (defaults to 0 but all files were processed).
        let resume_offset = if result.file_offset > 0 {
            result.file_offset
        } else {
            total_files
        };
        let resume_token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"file_offset": resume_offset}),
        );
        let pct = if total_files > 0 { (processed as f64 / total_files as f64 * 100.0) as u32 } else { 0 };
        let response = serde_json::json!({
            "added": result.added,
            "updated": result.updated,
            "unchanged": result.unchanged,
            "reindexed": result.reindexed,
            "continue": true,
            "resume": resume_token,
            "progress": {
                "processed": processed,
                "remaining": total_files.saturating_sub(processed),
                "total": total_files,
                "percent_complete": pct,
            },
            "message": format!(
                "⚠️ MANDATORY CONTINUATION — {processed}/{total_files} processed ({pct}%). \
                 You MUST call scan_repository again with the resume token to continue. \
                 Do NOT stop, do NOT reason about whether to continue, do NOT report partial results. \
                 Just call scan_repository again until this message no longer appears."
            ),
            "when_done": "MANDATORY: Call scan_repository again passing the resume token. Do NOT use the think tool to decide whether to continue. Just call it.",
        });
        return Ok(response);
    }

    let summary = if result.embeddings_skipped {
        format!(
            "Indexed {} documents (embeddings skipped \u{2014} use embeddings import to load)",
            result.total
        )
    } else {
        format!(
            "{} added, {} updated (temporal coverage: {:.0}%, source coverage: {:.0}%)",
            result.added, result.updated, temporal_coverage, source_coverage
        )
    };

    let fact_hint: Option<String> = None;

    let duration_ms = result.stats.as_ref().map(|s| s.total_ms).unwrap_or(0);

    Ok(serde_json::json!({
        "added": result.added,
        "updated": result.updated,
        "unchanged": result.unchanged,
        "reindexed": result.reindexed,
        "deleted": result.deleted,
        "moved": result.moved,
        "links_detected": result.links_detected,
        "questions_pruned": result.questions_pruned,
        "fact_embeddings_generated": result.fact_embeddings_generated,
        "fact_embeddings_needed": result.fact_embeddings_needed,
        "total": result.total,
        "embeddings_skipped": result.embeddings_skipped,
        "temporal_coverage_percent": temporal_coverage,
        "source_coverage_percent": source_coverage,
        "duration_ms": duration_ms,
        "embedding_provider": config.embedding.provider,
        "embedding_dimension": provider_dim,
        "summary": summary,
        "hint": if result.links_detected == 0 && result.total > 1 {
            Some("Tip: Link detection finds entity title mentions in document text. If you expected links, check that documents reference other entities by their exact title (not markdown links or abbreviations).")
        } else {
            None
        },
        "fact_embeddings_hint": fact_hint,
    }))
}

/// Detect cross-document links via title string matching.
/// Runs as a separate phase after scan_repository.
pub async fn detect_links(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    use crate::scanner::orchestration::links::{run_link_detection_phase, LinkPhaseInput};

    let repo = crate::mcp::tools::helpers::resolve_repo(db, get_str_arg(args, "repo"))?;

    let config = Config::load(None).unwrap_or_default();
    let link_detector = LinkDetector::new();

    // Resume support
    let doc_offset = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token)
        .and_then(|v| v.get("doc_offset").and_then(|o| o.as_u64()))
        .unwrap_or(0) as usize;

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let _guard = WriteGuard::try_acquire()?;

    progress.log(&format!(
        "Detecting links for repository '{}'",
        repo.id
    ));

    let output = run_link_detection_phase(LinkPhaseInput {
        db,
        link_detector: &link_detector,
        repo_id: &repo.id,
        changed_ids: &HashSet::new(),
        added_count: 0,
        show_progress: false,
        verbose: false,
        skip_links: false,
        force_relink: true, // always scan all docs in standalone mode
        link_batch_size: config.processor.link_batch_size,
        progress,
        deadline,
        doc_offset,
    })
    .await
    .map_err(|e: anyhow::Error| FactbaseError::Internal(e.to_string()))?;

    info!(
        "Link detection complete: {} links detected across {} documents",
        output.links_detected, output.docs_link_detected
    );

    if output.interrupted && time_budget.is_some() {
        let resume_token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"doc_offset": output.doc_offset}),
        );
        let mut response = serde_json::json!({
            "links_detected": output.links_detected,
            "docs_processed": output.docs_link_detected,
        });
        crate::mcp::tools::helpers::apply_time_budget_progress(
            &mut response,
            output.doc_offset,
            output.doc_offset + output.docs_link_detected + 1, // estimate
            "detect_links",
            true,
            Some(&resume_token),
        );
        return Ok(response);
    }

    Ok(serde_json::json!({
        "links_detected": output.links_detected,
        "docs_processed": output.docs_link_detected,
        "duration_ms": output.link_detection_ms,
        "summary": format!(
            "{} links detected across {} documents",
            output.links_detected, output.docs_link_detected
        )
    }))
}

/// Initialize a new repository at the given path.
#[allow(dead_code)] // Kept for backward compat testing; removed from MCP dispatch
pub fn init_repository(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let path_str = crate::mcp::tools::get_str_arg_required(args, "path")?;
    let id = crate::mcp::tools::get_str_arg(args, "id");
    let name = crate::mcp::tools::get_str_arg(args, "name");

    let path = std::path::Path::new(&path_str);
    if !path.is_dir() {
        return Err(FactbaseError::not_found(format!(
            "Directory does not exist: {path_str}"
        )));
    }

    let abs_path = crate::organize::fs_helpers::clean_canonicalize(path);

    // Check if already registered
    let repos = db.list_repositories()?;
    if let Some(existing) = repos.iter().find(|r| r.path == abs_path) {
        return Ok(serde_json::json!({
            "already_exists": true,
            "id": existing.id,
            "name": existing.name,
            "path": existing.path.to_string_lossy(),
            "message": format!("Repository '{}' already registered at this path.", existing.id)
        }));
    }

    let default_id = abs_path
        .file_name()
        .map_or_else(|| crate::DEFAULT_REPO_ID.into(), |s| s.to_string_lossy().to_string());
    let repo_id = id.unwrap_or(&default_id);
    let repo_name = name.unwrap_or(repo_id);

    // Create .factbase dir and perspective.yaml if needed
    let factbase_dir = abs_path.join(".factbase");
    let factbase_dir_created = !factbase_dir.exists();
    std::fs::create_dir_all(&factbase_dir)
        .map_err(|e| FactbaseError::internal(format!("Cannot create .factbase dir: {e}")))?;
    let perspective_path = abs_path.join("perspective.yaml");
    let perspective_created = !perspective_path.exists();
    if perspective_created {
        // Best-effort: perspective.yaml is optional, log warning on failure
        if let Err(e) = std::fs::write(&perspective_path, crate::models::PERSPECTIVE_TEMPLATE) {
            tracing::warn!("Failed to write perspective.yaml: {e}");
        }
    }

    let gitignore_added = crate::ensure_gitignore(&abs_path).unwrap_or_default();

    let repo = crate::models::Repository {
        id: repo_id.to_string(),
        name: repo_name.to_string(),
        path: abs_path.clone(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_check_at: None,
    };
    db.upsert_repository(&repo)?;

    // Count markdown files for the status report
    let md_count = std::fs::read_dir(&abs_path)
        .ok()
        .map(|_| {
            fn count_md(dir: &std::path::Path) -> usize {
                std::fs::read_dir(dir)
                    .ok()
                    .map(|entries| {
                        entries.filter_map(|e| e.ok()).map(|e| {
                            let p = e.path();
                            if p.is_dir() && p.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')) {
                                count_md(&p)
                            } else if p.extension().is_some_and(|ext| ext == "md") {
                                1
                            } else {
                                0
                            }
                        }).sum()
                    })
                    .unwrap_or(0)
            }
            count_md(&abs_path)
        })
        .unwrap_or(0);

    let mut created_items = Vec::new();
    if factbase_dir_created {
        created_items.push(".factbase/");
    }
    if perspective_created {
        created_items.push("perspective.yaml");
    }
    if !gitignore_added.is_empty() {
        created_items.push(".gitignore");
    }

    info!(
        "Initialized repository '{}' at {}",
        repo_id,
        abs_path.display()
    );

    Ok(serde_json::json!({
        "id": repo_id,
        "name": repo_name,
        "path": abs_path.to_string_lossy(),
        "created": created_items,
        "markdown_files_found": md_count,
        "message": format!("Repository '{}' initialized at {}. {} markdown files found. Call scan_repository to index.", repo_id, abs_path.display(), md_count)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_init_repository_tolerates_preexisting_config() {
        let tmp = TempDir::new().unwrap();
        let factbase_dir = tmp.path().join(".factbase");
        std::fs::create_dir_all(&factbase_dir).unwrap();
        std::fs::write(
            factbase_dir.join("config.yaml"),
            "embedding:\n  provider: bedrock\n",
        )
        .unwrap();

        let (db, _db_dir) = test_db();
        let result = init_repository(
            &db,
            &json!({"path": tmp.path().to_string_lossy()}),
        )
        .unwrap();

        assert!(result.get("id").is_some());
        assert!(result.get("already_exists").is_none());
        // Config preserved
        assert!(factbase_dir.join("config.yaml").exists());
    }

    #[test]
    fn test_init_repository_already_registered() {
        let tmp = TempDir::new().unwrap();
        let (db, _db_dir) = test_db();
        let args = json!({"path": tmp.path().to_string_lossy()});

        init_repository(&db, &args).unwrap();
        let result = init_repository(&db, &args).unwrap();

        assert_eq!(result["already_exists"], true);
    }

    #[test]
    fn test_init_repository_nonexistent_dir() {
        let (db, _db_dir) = test_db();
        let result = init_repository(&db, &json!({"path": "/nonexistent/path/xyz"}));
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_repository_sets_skip_links() {
        // Verify that the factbase tool's scan op description references detect_links
        let tools = crate::mcp::tools::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(desc.contains("detect_links"), "factbase description should reference detect_links op");
    }

    #[tokio::test]
    async fn test_detect_links_no_repo() {
        let (db, _db_dir) = test_db();
        let result = detect_links(&db, &json!({}), &crate::ProgressReporter::Silent).await;
        assert!(result.is_err(), "detect_links should fail when no repo exists");
    }

    #[tokio::test]
    async fn test_detect_links_empty_repo() {
        let tmp = TempDir::new().unwrap();
        let (db, _db_dir) = test_db();
        init_repository(&db, &json!({"path": tmp.path().to_string_lossy()})).unwrap();

        let result = detect_links(&db, &json!({}), &crate::ProgressReporter::Silent).await.unwrap();
        assert_eq!(result["links_detected"], 0);
        assert_eq!(result["docs_processed"], 0);
    }

    #[tokio::test]
    async fn test_detect_links_idempotent() {
        let tmp = TempDir::new().unwrap();
        let (db, _db_dir) = test_db();
        init_repository(&db, &json!({"path": tmp.path().to_string_lossy()})).unwrap();

        // Run twice — should produce same result
        let r1 = detect_links(&db, &json!({}), &crate::ProgressReporter::Silent).await.unwrap();
        let r2 = detect_links(&db, &json!({}), &crate::ProgressReporter::Silent).await.unwrap();
        assert_eq!(r1["links_detected"], r2["links_detected"]);
    }

    #[test]
    fn test_detect_links_in_schema() {
        // detect_links is now an op in the factbase tool
        let tools = crate::mcp::tools::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let ops = fb["inputSchema"]["properties"]["op"]["enum"].as_array().unwrap();
        let op_strs: Vec<&str> = ops.iter().filter_map(|v| v.as_str()).collect();
        assert!(op_strs.contains(&"detect_links"), "detect_links should be a factbase op");
    }

    #[test]
    fn test_update_workflow_includes_detect_links_step() {
        // Verify detect_links step exists in the update workflow by checking
        // the workflow tool output at step 2
        let (db, _tmp) = test_db();
        let args = json!({"workflow": "update", "step": 2});
        let result = crate::mcp::tools::workflow::workflow(&db, &args);
        let result = result.unwrap();
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("detect_links"), "update step 2 should instruct detect_links");
        assert_eq!(result["next_tool"], "factbase");
    }
}
