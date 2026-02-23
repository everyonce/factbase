//! Repair command for auto-fixing document corruption.

use clap::Args;

#[derive(Args)]
pub struct RepairArgs {
    /// Only repair documents in this repository
    #[arg(long)]
    pub repo: Option<String>,
    /// Repair a single document by ID
    #[arg(long)]
    pub doc: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Suppress non-essential output
    #[arg(short, long)]
    pub quiet: bool,
}
