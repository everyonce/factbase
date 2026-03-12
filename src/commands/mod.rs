//! CLI command implementations

pub mod completions;
pub mod db;
pub mod doctor;
pub mod embeddings;
pub mod errors;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod repair;
pub mod scan;
#[cfg(feature = "mcp")]
pub mod serve;
pub mod setup;
pub mod status;
pub mod utils;
pub mod version;

/// Output format for commands that support multiple formats
#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Yaml,
}

impl OutputFormat {
    /// Resolve the effective format, with `--json` flag taking priority.
    pub fn resolve(json_flag: bool, format: OutputFormat) -> OutputFormat {
        if json_flag {
            OutputFormat::Json
        } else {
            format
        }
    }
}

// Re-export command functions for active CLI commands
pub use completions::cmd_completions;
pub use db::cmd_db_vacuum;
pub use doctor::cmd_doctor;
pub use embeddings::cmd_embeddings;
#[cfg(feature = "mcp")]
pub use mcp::cmd_mcp;
pub use repair::cmd_repair;
pub use scan::cmd_scan;
#[cfg(feature = "mcp")]
pub use serve::cmd_serve;
pub use status::cmd_status;
pub use version::cmd_version;

// Re-export setup helpers
#[cfg(feature = "mcp")]
pub use setup::setup_embedding;
pub use setup::{auto_init_repo, setup_cached_embedding, setup_embedding_with_timeout};

// Re-export error helpers
pub use errors::repo_path_not_found_error;

// Re-export utils
pub use utils::{confirm_prompt, create_repository, parse_since, parse_since_filter, print_output};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_resolve_json_flag_overrides() {
        assert!(matches!(
            OutputFormat::resolve(true, OutputFormat::Table),
            OutputFormat::Json
        ));
    }

    #[test]
    fn test_output_format_resolve_no_flag_preserves_format() {
        assert!(matches!(
            OutputFormat::resolve(false, OutputFormat::Yaml),
            OutputFormat::Yaml
        ));
    }
}
