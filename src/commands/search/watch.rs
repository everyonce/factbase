//! Watch mode logic for live-updating search results.

use super::args::SearchArgs;
use super::run_single_search;
use crate::commands::setup_database;
use crate::commands::utils::resolve_repos;
use crate::commands::watch_helper::{run_async_watch_loop, WatchContext};

/// Run search in watch mode - re-run search when files change
pub async fn run_search_watch_mode(args: SearchArgs) -> anyhow::Result<()> {
    let (config, db) = setup_database()?;
    let repos = resolve_repos(db.list_repositories()?, args.repo.as_deref())?;
    let mut ctx = WatchContext::new(&config, repos)?;

    let query = args.query.clone();
    run_async_watch_loop(
        &mut ctx,
        || {
            println!("Searching for: \"{query}\"");
            println!("{}", "=".repeat(50));
        },
        || run_single_search(&args),
    )
    .await
}
