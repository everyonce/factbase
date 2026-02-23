//! Repository management MCP tools.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, ProgressSender};
use crate::{Config, DocumentProcessor, LinkDetector, ScanOptions, Scanner};
use serde_json::Value;
use tracing::info;

/// Scan (or rescan) the repository to index documents, generate embeddings, and detect links.
pub async fn scan_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    _llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: Option<ProgressSender>,
) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo");

    let repos = db.list_repositories()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    let repo = repo.ok_or_else(|| {
        FactbaseError::NotFound("No repository found.".into())
    })?;

    let config = Config::load(None).unwrap_or_default();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let opts = ScanOptions::from_config(&config);

    // Link detection uses LLM which requires 'static ownership.
    // MCP scan uses NoOpLlm — manual [[id]] links are still detected.
    // For LLM-powered entity detection, run lint_repository after scanning.
    let link_detector = LinkDetector::new(Box::new(NoOpLlm));

    info!("Scanning repository '{}'...", repo.id);

    let scan_fut = crate::full_scan(&repo, db, &scanner, &processor, embedding, &link_detector, &opts);
    tokio::pin!(scan_fut);
    let mut elapsed_secs = 0u64;
    let result = loop {
        tokio::select! {
            result = &mut scan_fut => {
                break result.map_err(|e| FactbaseError::Internal(e.to_string()))?;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                elapsed_secs += 10;
                info!("Scan in progress... ({}s elapsed)", elapsed_secs);
                if let Some(ref tx) = progress {
                    let _ = tx.send(serde_json::json!({
                        "progress": elapsed_secs,
                        "total": 0,
                        "message": format!("Scan in progress... ({}s elapsed)", elapsed_secs),
                    }));
                }
            }
        }
    };

    info!(
        "Scan complete: {} added, {} updated, {} unchanged",
        result.added, result.updated, result.unchanged
    );

    Ok(serde_json::json!({
        "added": result.added,
        "updated": result.updated,
        "unchanged": result.unchanged,
        "deleted": result.deleted,
        "links_detected": result.links_detected,
        "total": result.total,
    }))
}

/// No-op LLM for when no LLM is available (skips link detection).
struct NoOpLlm;

impl LlmProvider for NoOpLlm {
    fn complete<'a>(&'a self, _prompt: &'a str) -> crate::BoxFuture<'a, Result<String, FactbaseError>> {
        Box::pin(async { Ok("[]".to_string()) })
    }
}

/// Initialize a new repository at the given path.
pub fn init_repository(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let path_str = crate::mcp::tools::get_str_arg_required(args, "path")?;
    let id = crate::mcp::tools::get_str_arg(args, "id");
    let name = crate::mcp::tools::get_str_arg(args, "name");

    let path = std::path::Path::new(&path_str);
    if !path.is_dir() {
        return Err(FactbaseError::not_found(format!(
            "Directory does not exist: {}",
            path_str
        )));
    }

    let abs_path = path
        .canonicalize()
        .map_err(|e| FactbaseError::internal(format!("Cannot resolve path: {}", e)))?;

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
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "main".into());
    let repo_id = id.unwrap_or(&default_id);
    let repo_name = name.unwrap_or(repo_id);

    // Create .factbase dir and perspective.yaml if needed
    let factbase_dir = abs_path.join(".factbase");
    std::fs::create_dir_all(&factbase_dir)
        .map_err(|e| FactbaseError::internal(format!("Cannot create .factbase dir: {}", e)))?;
    let perspective_path = abs_path.join("perspective.yaml");
    if !perspective_path.exists() {
        let _ = std::fs::write(&perspective_path, "# Factbase perspective\n");
    }

    let repo = crate::models::Repository {
        id: repo_id.to_string(),
        name: repo_name.to_string(),
        path: abs_path.clone(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.upsert_repository(&repo)?;

    info!("Initialized repository '{}' at {}", repo_id, abs_path.display());

    Ok(serde_json::json!({
        "id": repo_id,
        "name": repo_name,
        "path": abs_path.to_string_lossy(),
        "message": format!("Repository '{}' initialized. Call scan_repository to index documents.", repo_id)
    }))
}
