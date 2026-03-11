use super::{
    auto_init_repo, setup_cached_embedding,
};
use crate::commands::setup::Setup;
use factbase::mcp::run_stdio;

/// Run the MCP stdio transport (reads JSON-RPC from stdin, writes to stdout).
pub async fn cmd_mcp() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().unwrap_or_default();
    eprintln!(
        "factbase v{} (MCP stdio) cwd={}",
        env!("CARGO_PKG_VERSION"),
        cwd.display()
    );
    let result = Setup::new().require_repo(None).build();
    let (config, db, _) = match result {
        Ok(ctx) => ctx.take_repo(),
        Err(_) => auto_init_repo(&std::env::current_dir()?)?,
    };
    let cached_embedding = setup_cached_embedding(&config, None, &db).await;
    run_stdio(&db, &cached_embedding).await
}
