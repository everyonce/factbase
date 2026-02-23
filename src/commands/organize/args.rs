//! Organize command arguments.

use super::super::OutputFormat;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    about = "Reorganize knowledge base (merge, split, move, retype)",
    after_help = "\
EXAMPLES:
    # Analyze all repositories for reorganization opportunities
    factbase organize analyze

    # Merge two similar documents
    factbase organize merge abc123 def456

    # Split a multi-topic document
    factbase organize split abc123

    # Move a document to a different folder
    factbase organize move abc123 --to people/

    # Change document type without moving
    factbase organize retype abc123 --type person

    # Process answered organization suggestions
    factbase organize apply
"
)]
pub struct OrganizeArgs {
    #[command(subcommand)]
    pub command: OrganizeCommands,
}

#[derive(Subcommand)]
pub enum OrganizeCommands {
    /// Detect reorganization opportunities (merge, split, misplaced)
    Analyze(AnalyzeArgs),
    /// Merge similar documents
    Merge(MergeArgs),
    /// Split multi-topic document into separate documents
    Split(SplitArgs),
    /// Move document to a different folder
    Move(MoveArgs),
    /// Change document type without moving file
    Retype(RetypeArgs),
    /// Process answered organization suggestions from _orphans.md
    Apply(ApplyArgs),
}

#[derive(Parser)]
#[command(
    about = "Detect reorganization opportunities",
    after_help = "\
EXAMPLES:
    factbase organize analyze
    factbase organize analyze --repo myrepo
    factbase organize analyze --json
"
)]
pub struct AnalyzeArgs {
    /// Filter to specific repository
    #[arg(short, long)]
    pub repo: Option<String>,

    /// Output as JSON (shorthand for --format json)
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Minimum similarity threshold for merge candidates (default: 0.95)
    #[arg(long, default_value = "0.95")]
    pub merge_threshold: f32,

    /// Maximum similarity threshold for split candidates (default: 0.5)
    #[arg(long, default_value = "0.5")]
    pub split_threshold: f32,

    /// Timeout in seconds for Ollama API calls (default: from config)
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Parser)]
#[command(
    about = "Merge similar documents",
    after_help = "\
EXAMPLES:
    factbase organize merge abc123 def456
    factbase organize merge abc123 def456 --into abc123
    factbase organize merge abc123 def456 --dry-run
"
)]
pub struct MergeArgs {
    /// First document ID to merge
    pub doc1: String,

    /// Second document ID to merge
    pub doc2: String,

    /// Document ID to keep (default: auto-select based on content/links)
    #[arg(long)]
    pub into: Option<String>,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Timeout in seconds for Ollama API calls (default: from config)
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Parser)]
#[command(
    about = "Split multi-topic document",
    after_help = "\
EXAMPLES:
    factbase organize split abc123
    factbase organize split abc123 --at \"Section Title\"
    factbase organize split abc123 --dry-run
"
)]
pub struct SplitArgs {
    /// Document ID to split
    pub doc_id: String,

    /// Split at specific section title (auto-detect if not specified)
    #[arg(long)]
    pub at: Option<String>,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Timeout in seconds for Ollama API calls (default: from config)
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Parser)]
#[command(
    about = "Move document to different folder",
    after_help = "\
EXAMPLES:
    factbase organize move abc123 --to people/
    factbase organize move abc123 --to projects/active/
    factbase organize move abc123 --to people/ --dry-run
"
)]
pub struct MoveArgs {
    /// Document ID to move
    pub doc_id: String,

    /// Destination folder (relative to repository root)
    #[arg(long)]
    pub to: String,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Parser)]
#[command(
    about = "Change document type without moving",
    after_help = "\
EXAMPLES:
    factbase organize retype abc123 --type person
    factbase organize retype abc123 --type project --persist
    factbase organize retype abc123 --type concept --dry-run
"
)]
pub struct RetypeArgs {
    /// Document ID to retype
    pub doc_id: String,

    /// New document type
    #[arg(long, short = 't')]
    pub r#type: String,

    /// Persist type override to file (survives re-scans)
    #[arg(long)]
    pub persist: bool,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Parser)]
#[command(
    about = "Process answered organization suggestions",
    after_help = "\
EXAMPLES:
    factbase organize apply
    factbase organize apply --repo myrepo
    factbase organize apply --dry-run
"
)]
pub struct ApplyArgs {
    /// Filter to specific repository
    #[arg(short, long)]
    pub repo: Option<String>,

    /// Preview changes without modifying files
    #[arg(long)]
    pub dry_run: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Show detailed progress
    #[arg(long)]
    pub detailed: bool,
}
