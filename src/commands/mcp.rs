use super::{
    auto_init_repo, find_repo_with_config, setup_cached_embedding, setup_llm_with_timeout,
};
use factbase::mcp::run_stdio;

/// Run the MCP stdio transport (reads JSON-RPC from stdin, writes to stdout).
pub async fn cmd_mcp() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().unwrap_or_default();
    eprintln!(
        "factbase v{} (MCP stdio) cwd={}",
        env!("CARGO_PKG_VERSION"),
        cwd.display()
    );
    let result = find_repo_with_config(None);
    let (config, db, _) = match result {
        Ok(tuple) => tuple,
        Err(_) => auto_init_repo(&std::env::current_dir()?)?,
    };
    let cached_embedding = setup_cached_embedding(&config, None, &db).await;
    let llm = setup_llm_with_timeout(&config, None).await;
    run_stdio(&db, &cached_embedding, Some(llm.as_ref())).await
}
