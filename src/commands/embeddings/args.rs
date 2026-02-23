//! Embedding command arguments.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Manage vector embeddings")]
pub struct EmbeddingsArgs {
    #[command(subcommand)]
    pub command: EmbeddingsCommands,
}

#[derive(Subcommand)]
pub enum EmbeddingsCommands {
    /// Export embeddings to a JSONL file
    Export(ExportArgs),
    /// Import embeddings from a JSONL file
    Import(ImportArgs),
    /// Show embedding status
    Status(StatusArgs),
}

#[derive(Parser)]
pub struct ExportArgs {
    /// Output file path
    #[arg(short, long)]
    pub output: PathBuf,
    /// Filter by repository ID
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Parser)]
pub struct ImportArgs {
    /// Input file path
    #[arg(short, long)]
    pub input: PathBuf,
    /// Force import even if dimension mismatches
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser)]
pub struct StatusArgs {
    /// Filter by repository ID
    #[arg(long)]
    pub repo: Option<String>,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_args_required_output() {
        let result = ExportArgs::try_parse_from(["export"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_args_required_input() {
        let result = ImportArgs::try_parse_from(["import"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_args_force_default_false() {
        let args = ImportArgs::try_parse_from(["import", "--input", "file.jsonl"]).unwrap();
        assert!(!args.force);
    }

    #[test]
    fn test_status_args_defaults() {
        let args = StatusArgs::try_parse_from(["status"]).unwrap();
        assert!(args.repo.is_none());
        assert!(!args.json);
    }
}
