//! Repository management MCP tools.

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

/// Scan (or rescan) the repository to index documents, generate embeddings, and detect links.
pub async fn scan_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = crate::mcp::tools::helpers::resolve_repo_filter(db, get_str_arg(args, "repo"))?;

    let repos = db.list_repositories()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    let repo = repo.ok_or_else(|| FactbaseError::NotFound("No repository found.".into()))?;

    let config = Config::load(None).unwrap_or_default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let mut opts = ScanOptions::from_config(&config);

    // Wire force_reindex from MCP args
    opts.force_reindex = crate::mcp::tools::helpers::get_bool_arg(args, "force_reindex", false);
    opts.skip_embeddings = crate::mcp::tools::helpers::get_bool_arg(args, "skip_embeddings", false);

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

    // Link detection uses string matching only (no LLM required).
    // Manual [[id]] links and fuzzy title matches are detected.
    let link_detector = LinkDetector::new();

    // Acquire write guard right before full_scan — setup above is read-only
    let _guard = WriteGuard::try_acquire()?;

    progress.log(&format!("Scanning repository '{}'...", repo.id));

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

    // If interrupted by deadline, return progress response
    if result.interrupted && time_budget.is_some() {
        let total_all = result.file_offset.max(processed);
        let resume_token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"file_offset": result.file_offset}),
        );
        let mut response = serde_json::json!({
            "added": result.added,
            "updated": result.updated,
            "unchanged": result.unchanged,
            "reindexed": result.reindexed,
        });
        crate::mcp::tools::helpers::apply_time_budget_progress(
            &mut response, processed, total_all, "scan_repository", true,
            Some(&resume_token),
        );
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

    let fact_hint = if result.fact_embeddings_needed > 0 {
        Some(format!(
            "Run check_repository with mode='embeddings' to generate fact embeddings for {} document(s).",
            result.fact_embeddings_needed
        ))
    } else {
        None
    };

    Ok(serde_json::json!({
        "added": result.added,
        "updated": result.updated,
        "unchanged": result.unchanged,
        "reindexed": result.reindexed,
        "deleted": result.deleted,
        "links_detected": result.links_detected,
        "fact_embeddings_generated": result.fact_embeddings_generated,
        "fact_embeddings_needed": result.fact_embeddings_needed,
        "total": result.total,
        "embeddings_skipped": result.embeddings_skipped,
        "temporal_coverage_percent": temporal_coverage,
        "source_coverage_percent": source_coverage,
        "summary": summary,
        "hint": if result.links_detected == 0 && result.total > 1 {
            Some("Tip: Link detection finds entity title mentions in document text. If you expected links, check that documents reference other entities by their exact title (not markdown links or abbreviations).")
        } else {
            None
        },
        "fact_embeddings_hint": fact_hint,
    }))
}

/// Initialize a new repository at the given path.
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
    std::fs::create_dir_all(&factbase_dir)
        .map_err(|e| FactbaseError::internal(format!("Cannot create .factbase dir: {e}")))?;
    let perspective_path = abs_path.join("perspective.yaml");
    if !perspective_path.exists() {
        let _ = std::fs::write(&perspective_path, crate::models::PERSPECTIVE_TEMPLATE);
    }

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

    info!(
        "Initialized repository '{}' at {}",
        repo_id,
        abs_path.display()
    );

    Ok(serde_json::json!({
        "id": repo_id,
        "name": repo_name,
        "path": abs_path.to_string_lossy(),
        "message": format!("Repository '{}' initialized. Call scan_repository to index documents.", repo_id)
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
}
