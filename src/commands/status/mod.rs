mod args;
mod detailed;
mod display;

pub use args::StatusArgs;

use super::{parse_since_filter, print_output, OutputFormat};
use crate::commands::setup::Setup;
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
    let ctx = Setup::new().require_repo(None).build()?;
    let db = &ctx.db;
    let format = OutputFormat::resolve(args.json, args.format);
    let since: Option<DateTime<Utc>> = parse_since_filter(&args.since)?;

    let repos = db.list_repositories()?;
    let repo = repos
        .first()
        .ok_or_else(|| anyhow::anyhow!("No repository found"))?;

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

    // Include KB health status in JSON output
    let health = if since.is_none() {
        factbase::services::kb_status(db, Some(&repo.id)).ok()
    } else {
        None
    };

    let mut json_data = format_repo_status_json(
        repo,
        &stats,
        detailed.as_ref(),
        pool_stats.as_ref(),
        temporal_stats.as_ref(),
        source_stats.as_ref(),
        since.as_ref(),
    );

    // Merge health fields into JSON output
    if let Some(ref h) = health {
        if let (Some(obj), Some(h_obj)) = (json_data.as_object_mut(), h.as_object()) {
            for (k, v) in h_obj {
                obj.entry(k).or_insert_with(|| v.clone());
            }
        }
    }

    print_output(format, &json_data, || {
        print_repo_status_text(
            repo,
            &stats,
            detailed.as_ref(),
            pool_stats.as_ref(),
            temporal_stats.as_ref(),
            source_stats.as_ref(),
            since.as_ref(),
        );
        // Print health summary in text mode
        if let Some(ref h) = health {
            if let Some(summary) = h.get("summary").and_then(|v| v.as_str()) {
                println!();
                println!("{summary}");
            }
        }
    })?;

    // Show aggregate stats (merged from old `stats` command)
    if !args.detailed && !args.quiet {
        let db_path = ctx.db_path;
        let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        if db_size > 0 {
            println!(
                "\nDatabase: {} ({})",
                db_path.display(),
                factbase::output::format_bytes(db_size)
            );
        }
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
