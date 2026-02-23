//! Lint command argument parsing.
//!
//! Contains the `LintArgs` struct with clap attributes for CLI argument parsing.

use super::OutputFormat;
use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "Check knowledge base quality",
    after_help = "\
EXAMPLES:
    factbase lint --repo myrepo
    factbase lint --check-duplicates --min-similarity 0.9
    factbase lint --review --dry-run
    factbase lint --max-age 365 --fix
"
)]
pub struct LintArgs {
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
        help = "Generate review questions for documents using LLM analysis"
    )]
    pub review: bool,
    #[arg(
        long,
        help = "Preview changes without modifying files (use with --fix or --review)"
    )]
    pub dry_run: bool,
    #[arg(long, short = 'q', help = "Suppress progress output")]
    pub quiet: bool,
    #[arg(
        long,
        short = 'w',
        help = "Watch for file changes and re-lint automatically"
    )]
    pub watch: bool,
    #[arg(
        long,
        short = 'p',
        help = "Process documents in parallel for faster linting"
    )]
    pub parallel: bool,
    #[arg(
        long,
        help = "Only lint files modified since date (ISO 8601 or relative: 1h, 1d, 1w)"
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
        help = "Export generated questions to file instead of appending to documents (use with --review)"
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
        help = "Only lint documents modified since last lint (tracks timestamp per repository)"
    )]
    pub incremental: bool,
    #[arg(
        long,
        help = "Cross-check facts against other documents for conflicts and staleness (expensive, opt-in)"
    )]
    pub cross_check: bool,
    #[arg(
        long,
        help = "Timeout in seconds for Ollama API calls (default: from config, max: 300)"
    )]
    pub timeout: Option<u64>,
}
