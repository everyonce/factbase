use super::{create_repository, find_repo_with_config, setup_cached_embedding, setup_llm_with_timeout};
use factbase::mcp::run_stdio;
use factbase::Config;
use std::fs;
use tracing::info;

/// Run the MCP stdio transport (reads JSON-RPC from stdin, writes to stdout).
pub async fn cmd_mcp() -> anyhow::Result<()> {
    info!("factbase v{} starting (MCP stdio)", env!("CARGO_PKG_VERSION"));
    let result = find_repo_with_config(None);
    let (config, db, _) = match result {
        Ok(tuple) => tuple,
        Err(_) => {
            // Auto-init cwd so the MCP server always starts
            let cwd = std::env::current_dir()?;
            let factbase_dir = cwd.join(".factbase");
            fs::create_dir_all(&factbase_dir)?;
            let perspective_path = cwd.join("perspective.yaml");
            if !perspective_path.exists() {
                fs::write(&perspective_path, "# Factbase perspective\n")?;
            }
            let config = Config::load(None)?;
            let db_path = factbase_dir.join("factbase.db");
            let db = config.open_database(&db_path)?;
            let name = cwd
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "main".into());
            let repo = create_repository("main", &name, &cwd);
            db.upsert_repository(&repo)?;
            info!("Auto-initialized factbase at {}", cwd.display());
            (config, db, repo)
        }
    };
    let cached_embedding = setup_cached_embedding(&config, None).await;
    let llm = setup_llm_with_timeout(&config, None).await;
    run_stdio(&db, &cached_embedding, Some(llm.as_ref())).await
}
