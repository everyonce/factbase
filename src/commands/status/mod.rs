mod args;
mod detailed;
mod display;

pub use args::StatusArgs;

use super::{find_repo, parse_since_filter, print_output, OutputFormat};
use chrono::{DateTime, Utc};
use detailed::format_repo_status_json;
use display::print_repo_status_text;

/// Format a coverage percentage with appropriate precision
pub fn format_coverage(coverage: f32) -> String {
    if coverage == 100.0 {
        "100%".to_string()
    } else if coverage >= 10.0 {
        format!("{coverage:.1}%")
    } else {
        format!("{coverage:.2}%")
    }
}

pub fn cmd_status(args: StatusArgs) -> anyhow::Result<()> {
    let (db, _) = find_repo(None)?;
    let format = OutputFormat::resolve(args.json, args.format);

    // Parse --since filter if provided
    let since: Option<DateTime<Utc>> = parse_since_filter(&args.since)?;

    if let Some(repo_id) = args.repo.as_deref() {
        let repos = db.list_repositories()?;
        let repo = repos
            .iter()
            .find(|r| r.id == repo_id)
            .ok_or_else(|| factbase::repo_not_found(repo_id))?;

        // Use filtered or unfiltered stats based on --since
        let stats = db.get_stats(&repo.id, since.as_ref())?;

        let detailed = if args.detailed {
            Some(db.get_detailed_stats(&repo.id, since.as_ref())?)
        } else {
            None
        };

        let pool_stats = if args.detailed {
            Some(db.pool_stats())
        } else {
            None
        };

        // Temporal and source stats don't support --since filter yet
        let temporal_stats = if args.detailed && since.is_none() {
            Some(db.compute_temporal_stats(&repo.id)?)
        } else {
            None
        };
        let source_stats = if args.detailed && since.is_none() {
            Some(db.compute_source_stats(&repo.id)?)
        } else {
            None
        };

        let json_data = format_repo_status_json(
            repo,
            &stats,
            detailed.as_ref(),
            pool_stats.as_ref(),
            temporal_stats.as_ref(),
            source_stats.as_ref(),
            since.as_ref(),
        );
        print_output(format, &json_data, || {
            print_repo_status_text(
                repo,
                &stats,
                detailed.as_ref(),
                pool_stats.as_ref(),
                temporal_stats.as_ref(),
                source_stats.as_ref(),
                since.as_ref(),
            )
        })?;
    } else {
        let repos = db.list_repositories_with_stats()?;
        let quiet = args.quiet;

        let output: Vec<_> = repos
            .iter()
            .map(|(repo, count)| {
                serde_json::json!({"id": repo.id, "name": repo.name, "path": repo.path, "documents": count})
            })
            .collect();

        print_output(format, &output, || {
            if repos.is_empty() {
                if !quiet {
                    println!("No repositories registered");
                }
                return;
            }
            let total_docs: usize = repos.iter().map(|(_, c)| c).sum();
            if !quiet {
                println!("Factbase Status\n===============");
                println!(
                    "Repositories: {}\nTotal documents: {}\n",
                    repos.len(),
                    total_docs
                );
            }
            println!("{:<12} {:<20} {:<8} PATH", "ID", "NAME", "DOCS");
            println!("{}", "-".repeat(60));
            for (repo, count) in &repos {
                println!(
                    "{:<12} {:<20} {:<8} {}",
                    repo.id,
                    repo.name,
                    count,
                    repo.path.display()
                );
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_coverage_full() {
        assert_eq!(format_coverage(100.0), "100%");
    }

    #[test]
    fn test_format_coverage_high() {
        assert_eq!(format_coverage(85.5), "85.5%");
        assert_eq!(format_coverage(10.0), "10.0%");
    }

    #[test]
    fn test_format_coverage_low() {
        assert_eq!(format_coverage(5.5), "5.50%");
        assert_eq!(format_coverage(0.5), "0.50%");
    }
}
