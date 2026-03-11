//! Shared watch mode helper for commands that monitor file changes.
//!
//! Provides common functionality for grep, search, and lint watch modes.

use chrono::Local;
use factbase::config::Config;
use factbase::models::Repository;
use factbase::watcher::{FileWatcher, find_repo_for_path};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

/// Context for watch mode operations.
pub struct WatchContext {
    pub watcher: FileWatcher,
    pub repos: Vec<Repository>,
}

impl WatchContext {
    /// Create a new watch context with FileWatcher configured from config.
    pub fn new(config: &Config, repos: Vec<Repository>) -> anyhow::Result<Self> {
        let mut watcher =
            FileWatcher::new(config.watcher.debounce_ms, &config.watcher.ignore_patterns)?;

        for repo in &repos {
            watcher.watch_directory(&repo.path)?;
        }

        Ok(Self { watcher, repos })
    }

    /// Check for file changes and return changed paths if any are in watched repos.
    pub fn check_changes(&mut self) -> Option<Vec<PathBuf>> {
        if let Some(changed_paths) = self.watcher.try_recv() {
            for path in &changed_paths {
                info!("File changed: {}", path.display());
            }

            // Check if any changed file is in a watched repository
            if let Some(path) = changed_paths.first() {
                if find_repo_for_path(path, &self.repos).is_some() {
                    return Some(changed_paths);
                }
            }
        }
        None
    }
}

/// Clear terminal screen and move cursor to top.
pub fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

/// Print the standard "watching" status message.
pub fn print_watching_status() {
    println!();
    println!("Watching for changes... (Press Ctrl+C to stop)");
    println!("Last update: {}", Local::now().format("%H:%M:%S"));
}

/// Run a synchronous watch loop.
///
/// Calls `run_fn` initially and on each file change.
/// The `header_fn` is called before each run to print context.
pub fn run_sync_watch_loop<H, R>(
    ctx: &mut WatchContext,
    header_fn: H,
    run_fn: R,
) -> anyhow::Result<()>
where
    H: Fn(),
    R: Fn() -> anyhow::Result<()>,
{
    // Initial run
    header_fn();
    run_fn()?;
    print_watching_status();

    loop {
        if ctx.check_changes().is_some() {
            clear_screen();
            header_fn();

            if let Err(e) = run_fn() {
                eprintln!(
                    "{}",
                    factbase::error::format_user_error(&format!("Command failed: {e}"), None)
                );
            }

            print_watching_status();
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Run an async watch loop.
///
/// Calls `run_fn` initially and on each file change.
/// The `header_fn` is called before each run to print context.
pub async fn run_async_watch_loop<H, R, Fut>(
    ctx: &mut WatchContext,
    header_fn: H,
    run_fn: R,
) -> anyhow::Result<()>
where
    H: Fn(),
    R: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    // Initial run
    header_fn();
    run_fn().await?;
    print_watching_status();

    loop {
        if ctx.check_changes().is_some() {
            clear_screen();
            header_fn();

            if let Err(e) = run_fn().await {
                eprintln!(
                    "{}",
                    factbase::error::format_user_error(&format!("Command failed: {e}"), None)
                );
            }

            print_watching_status();
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopping file watcher...");
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_screen_escape_sequence() {
        // Just verify the function doesn't panic
        // Actual screen clearing is terminal-dependent
        clear_screen();
    }
}
