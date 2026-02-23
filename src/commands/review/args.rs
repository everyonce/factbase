//! Review command arguments.

use super::super::OutputFormat;
use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "Process review questions",
    after_help = "\
EXAMPLES:
    factbase review --status
    factbase review --apply --dry-run
    factbase review --apply -r myrepo
    factbase review --import-questions questions.json
"
)]
pub struct ReviewArgs {
    /// Process answered questions and update documents
    #[arg(long)]
    pub apply: bool,

    /// Show summary of pending review questions
    #[arg(long)]
    pub status: bool,

    /// Import review questions from JSON/YAML file
    #[arg(long)]
    pub import_questions: Option<String>,

    /// Filter to specific repository
    #[arg(short, long)]
    pub repo: Option<String>,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Output as JSON (shorthand for --format json)
    #[arg(short, long)]
    pub json: bool,

    /// Suppress non-essential output (useful for scripting)
    #[arg(short, long)]
    pub quiet: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Show detailed progress during --apply
    #[arg(long)]
    pub detailed: bool,

    /// Only process files modified since date (ISO 8601 or relative: 1h, 1d, 1w)
    #[arg(long)]
    pub since: Option<String>,

    /// Timeout in seconds for Ollama API calls (default: from config, max: 300)
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Remove all unanswered review questions from documents.
    /// Keeps answered questions (ready for --apply). Use --type to clear only specific types.
    #[arg(long)]
    pub clear: bool,

    /// Filter --clear to a specific question type (temporal, conflict, missing, ambiguous, stale, duplicate)
    #[arg(long, value_name = "TYPE")]
    pub r#type: Option<String>,
}
