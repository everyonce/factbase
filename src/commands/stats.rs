use crate::commands::setup::Setup;
use clap::Parser;
use factbase::output::{format_bytes, format_json};
use serde::Serialize;
use std::fs;

#[derive(Parser)]
#[command(
    about = "Show quick aggregate statistics",
    after_help = "\
EXAMPLES:
    # Show full statistics
    factbase stats

    # Single-line output for scripting
    factbase stats --short
    # Output: 2 repos, 45 docs, 128 KB

    # JSON output for CI/scripting
    factbase stats --json
"
)]
pub struct StatsArgs {
    /// Single-line output for scripting
    #[arg(short, long)]
    pub short: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
}

/// JSON output structure for stats command
#[derive(Serialize)]
struct StatsOutput {
    repos_count: usize,
    docs_count: usize,
    db_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_scan: Option<String>,
}

pub fn cmd_stats(args: StatsArgs) -> anyhow::Result<()> {
    let ctx = Setup::new().build()?;
    let (config, db) = (ctx.config, ctx.db);

    let repos = db.list_repositories_with_stats()?;

    let total_repos = repos.len();
    let total_docs: usize = repos.iter().map(|(_, c)| c).sum();

    let db_path = shellexpand::tilde(&config.database.path).to_string();
    let db_size = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let last_scan = repos.iter().filter_map(|(r, _)| r.last_indexed_at).max();

    if args.json {
        let output = StatsOutput {
            repos_count: total_repos,
            docs_count: total_docs,
            db_size_bytes: db_size,
            last_scan: last_scan.map(|ts| ts.to_rfc3339()),
        };
        println!("{}", format_json(&output)?);
    } else if repos.is_empty() {
        if args.short {
            println!("0 repos, 0 docs, 0 B");
        } else {
            println!("No repositories registered");
        }
    } else if args.short {
        println!(
            "{} repos, {} docs, {}",
            total_repos,
            total_docs,
            format_bytes(db_size)
        );
    } else {
        println!("Factbase Stats");
        println!("==============");
        println!("Repositories: {total_repos}");
        println!("Documents:    {total_docs}");
        println!("Database:     {}", format_bytes(db_size));
        if let Some(ts) = last_scan {
            println!("Last scan:    {}", ts.format("%Y-%m-%d %H:%M:%S"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_args_default() {
        let args = StatsArgs {
            short: false,
            json: false,
        };
        assert!(!args.short);
        assert!(!args.json);
    }

    #[test]
    fn test_stats_args_short() {
        let args = StatsArgs {
            short: true,
            json: false,
        };
        assert!(args.short);
    }

    #[test]
    fn test_stats_args_json() {
        let args = StatsArgs {
            short: false,
            json: true,
        };
        assert!(args.json);
    }

    #[test]
    fn test_stats_output_serialization() {
        let output = StatsOutput {
            repos_count: 2,
            docs_count: 45,
            db_size_bytes: 131072,
            last_scan: Some("2024-01-25T12:00:00+00:00".to_string()),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"repos_count\":2"));
        assert!(json.contains("\"docs_count\":45"));
        assert!(json.contains("\"db_size_bytes\":131072"));
        assert!(json.contains("\"last_scan\":"));
    }

    #[test]
    fn test_stats_output_no_last_scan() {
        let output = StatsOutput {
            repos_count: 0,
            docs_count: 0,
            db_size_bytes: 0,
            last_scan: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("last_scan"));
    }
}
