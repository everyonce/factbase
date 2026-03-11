use super::{setup::Setup, utils::print_output, OutputFormat};
use clap::Parser;
use factbase::format_bytes;
use serde::Serialize;
use std::fs;

#[derive(Parser)]
#[command(
    about = "Optimize database storage",
    after_help = "\
EXAMPLES:
    # Reclaim space from deleted documents
    factbase db vacuum
    # Output: Before: 1 MB, After: 512 KB, Reclaimed: 512 KB (50.0%)
"
)]
pub struct DbVacuumArgs;

#[derive(Parser)]
#[command(
    about = "Show database statistics",
    after_help = "\
EXAMPLES:
    # Show database stats
    factbase db stats

    # Output as JSON
    factbase db stats --format json
"
)]
pub struct DbStatsArgs {
    /// Output format
    #[arg(short, long, value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Parser)]
#[command(
    about = "Backfill word counts for existing documents",
    after_help = "\
EXAMPLES:
    # Backfill word counts for documents missing them
    factbase db backfill-word-counts
    # Output: Updated 42 documents with word counts
"
)]
pub struct DbBackfillWordCountsArgs;

/// Format vacuum results as a human-readable string
pub fn format_vacuum_result(before: u64, after: u64) -> String {
    let reclaimed = before.saturating_sub(after);
    let percent = if before > 0 {
        (reclaimed as f64 / before as f64) * 100.0
    } else {
        0.0
    };
    format!(
        "Before: {}, After: {}, Reclaimed: {} ({:.1}%)",
        format_bytes(before),
        format_bytes(after),
        format_bytes(reclaimed),
        percent
    )
}

pub fn cmd_db_vacuum() -> anyhow::Result<()> {
    let ctx = Setup::new().check_exists().build()?;
    let (_config, db, _db_path) = (ctx.config, ctx.db, ctx.db_path);
    let (before, after) = db.vacuum()?;

    println!("Database optimized:");
    println!("{}", format_vacuum_result(before, after));

    Ok(())
}

/// Output structure for database stats
#[derive(Serialize)]
struct DbStatsOutput {
    pool: PoolStats,
    database: DatabaseInfo,
    counts: Counts,
}

#[derive(Serialize)]
struct PoolStats {
    connections: u32,
    idle_connections: u32,
    max_size: u32,
}

#[derive(Serialize)]
struct DatabaseInfo {
    path: String,
    size_bytes: u64,
    compression: bool,
}

#[derive(Serialize)]
struct Counts {
    repositories: usize,
    documents: usize,
}

pub fn cmd_db_stats(args: DbStatsArgs) -> anyhow::Result<()> {
    let ctx = Setup::new().check_exists().build()?;
    let (config, db, db_path) = (ctx.config, ctx.db, ctx.db_path);
    let compression = config.database.is_compression_enabled();

    // Get pool stats
    let pool_stats = db.pool_stats();

    // Get database file size
    let db_size = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    // Get document and repo counts
    let repos = db.list_repositories()?;
    let total_docs: usize = repos
        .iter()
        .map(|r| db.get_stats(&r.id, None).map(|s| s.total).unwrap_or(0))
        .sum();

    let output = DbStatsOutput {
        pool: PoolStats {
            connections: pool_stats.connections,
            idle_connections: pool_stats.idle_connections,
            max_size: pool_stats.max_size,
        },
        database: DatabaseInfo {
            path: db_path.display().to_string(),
            size_bytes: db_size,
            compression,
        },
        counts: Counts {
            repositories: repos.len(),
            documents: total_docs,
        },
    };

    print_output(args.format, &output, || {
        println!("Database Stats");
        println!("==============\n");
        println!("Connection Pool:");
        println!("  Active connections: {}", output.pool.connections);
        println!("  Idle connections:   {}", output.pool.idle_connections);
        println!("  Max pool size:      {}", output.pool.max_size);
        println!();
        println!("Database:");
        println!("  Path:        {}", output.database.path);
        println!(
            "  Size:        {}",
            format_bytes(output.database.size_bytes)
        );
        println!(
            "  Compression: {}",
            if output.database.compression {
                "zstd"
            } else {
                "none"
            }
        );
        println!();
        println!("Counts:");
        println!("  Repositories: {}", output.counts.repositories);
        println!("  Documents:    {}", output.counts.documents);
    })
}

pub fn cmd_db_backfill_word_counts() -> anyhow::Result<()> {
    let ctx = Setup::new().check_exists().build()?;
    let (_config, db, _db_path) = (ctx.config, ctx.db, ctx.db_path);
    let updated = db.backfill_word_counts()?;

    if updated == 0 {
        println!("All documents already have word counts");
    } else {
        println!("Updated {updated} document(s) with word counts");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_vacuum_result_no_change() {
        let result = format_vacuum_result(1000, 1000);
        assert!(result.contains("Reclaimed: 0 B"));
        assert!(result.contains("0.0%"));
    }

    #[test]
    fn test_format_vacuum_result_with_savings() {
        let result = format_vacuum_result(1000, 500);
        assert!(result.contains("Before: 1000 B"));
        assert!(result.contains("After: 500 B"));
        assert!(result.contains("Reclaimed: 500 B"));
        assert!(result.contains("50.0%"));
    }

    #[test]
    fn test_format_vacuum_result_empty_db() {
        let result = format_vacuum_result(0, 0);
        assert!(result.contains("Reclaimed: 0 B"));
    }
}
