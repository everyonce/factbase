use super::{find_repo_with_config, setup_cached_embedding, setup_llm_with_timeout};
use factbase::mcp::run_stdio;

/// Run the MCP stdio transport (reads JSON-RPC from stdin, writes to stdout).
pub async fn cmd_mcp() -> anyhow::Result<()> {
    let (config, db, _) = find_repo_with_config(None)?;
    let cached_embedding = setup_cached_embedding(&config, None).await;
    let llm = setup_llm_with_timeout(&config, None).await;
    run_stdio(&db, &cached_embedding, Some(llm.as_ref())).await
}
