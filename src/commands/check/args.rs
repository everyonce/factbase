//! Check command argument parsing.

use super::OutputFormat;
use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "Check knowledge base quality and generate review questions",
    after_help = "\
EXAMPLES:
    factbase check
    factbase check --repo myrepo
    factbase check --dry-run
    factbase check --cross-check
    factbase check --check-duplicates --min-similarity 0.9
    factbase check --max-age 365 --fix
"
)]
pub struct CheckArgs {
    #[arg(long, short = 'r')]
    pub repo: Option<String>,
    #[arg(long, default_value = "100")]
    pub min_length: usize,
    #[arg(long, help = "Warn about documents not modified in N days")]
    pub max_age: Option<i64>,
    #[arg(long, help = "Check for duplicate or near-duplicate documents")]
    pub check_duplicates: bool,
    #[arg(
        long,
        default_value = "0.95",
        help = "Minimum similarity threshold for duplicates (0.0-1.0)"
    )]
    pub min_similarity: f32,
    #[arg(long, help = "Auto-fix broken links by removing them")]
    pub fix: bool,
    #[arg(
        long,
        help = "Validate temporal tags for format correctness and consistency"
    )]
    pub check_temporal: bool,
    #[arg(
        long,
        help = "Validate source footnotes for orphan references and definitions"
    )]
    pub check_sources: bool,
    #[arg(
        long,
        short = 'j',
        help = "Output as JSON (shorthand for --format json)"
    )]
    pub json: bool,
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
    #[arg(
        long,
        help = "Preview changes without modifying files"
    )]
    pub dry_run: bool,
    #[arg(long, short = 'q', help = "Suppress progress output")]
    pub quiet: bool,
    #[arg(
        long,
        short = 'w',
        help = "Watch for file changes and re-check automatically"
    )]
    pub watch: bool,
    #[arg(
        long,
        short = 'p',
        help = "Process documents in parallel for faster checking"
    )]
    pub parallel: bool,
    #[arg(
        long,
        help = "Only check files modified since date (ISO 8601 or relative: 1h, 1d, 1w)"
    )]
    pub since: Option<String>,
    #[arg(
        long,
        default_value = "0",
        help = "Process documents in batches of N to limit memory usage (0 = no batching)"
    )]
    pub batch_size: usize,
    #[arg(
        long,
        help = "Export generated questions to file instead of appending to documents"
    )]
    pub export_questions: Option<String>,
    #[arg(
        long,
        short = 'a',
        help = "Run all validation checks (equivalent to --check-temporal --check-sources --check-duplicates)"
    )]
    pub check_all: bool,
    #[arg(
        long,
        help = "Only check documents modified since last check (tracks timestamp per repository)"
    )]
    pub incremental: bool,
    #[arg(
        long,
        help = "Cross-check facts against other documents for conflicts and staleness (requires inference backend)"
    )]
    pub cross_check: bool,
    #[arg(
        long,
        help = "Timeout in seconds for API calls (default: from config, max: 300)"
    )]
    pub timeout: Option<u64>,
}
