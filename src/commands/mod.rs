//! CLI command implementations

pub mod completions;
pub mod db;
pub mod doctor;
pub mod embeddings;
pub mod export;
pub mod filters;
pub mod grep;
pub mod import;
pub mod init;
pub mod links;
pub mod check;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod organize;
pub mod repair;
pub mod repo;
pub mod review;
pub mod scan;
pub mod search;
#[cfg(feature = "mcp")]
pub mod serve;
pub mod setup;
pub mod show;
pub mod stats;
pub mod status;
pub mod utils;
pub mod version;
pub mod watch_helper;

#[cfg(test)]
pub(crate) mod test_helpers;

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

// Re-export command functions
pub use completions::cmd_completions;
pub use db::cmd_db_vacuum;
pub use doctor::cmd_doctor;
pub use embeddings::cmd_embeddings;
pub use export::cmd_export;
pub use grep::cmd_grep;
pub use import::cmd_import;
pub use init::cmd_init;
pub use links::cmd_links;
pub use check::cmd_check;
#[cfg(feature = "mcp")]
pub use mcp::cmd_mcp;
pub use organize::cmd_organize;
pub use repair::cmd_repair;
pub use repo::{cmd_repo_add, cmd_repo_list, cmd_repo_remove};
pub use review::cmd_review;
pub use scan::cmd_scan;
pub use search::cmd_search;
#[cfg(feature = "mcp")]
pub use serve::cmd_serve;
pub use show::cmd_show;
pub use stats::{cmd_stats, StatsArgs};
pub use status::cmd_status;
pub use version::cmd_version;

// Re-export helper functions from submodules for internal use via `super::`
pub use setup::{
    auto_init_repo, clean_canonicalize, find_repo, find_repo_with_config, setup_cached_embedding,
    setup_database, setup_database_checked, setup_database_only, setup_embedding_with_timeout,
    setup_llm_with_timeout, setup_review_llm_with_timeout,
};
#[cfg(feature = "mcp")]
pub use setup::setup_embedding;
pub use utils::{
    confirm_prompt, create_repository, filter_by_excluded_types,
    parse_since, parse_since_filter, print_output, repo_path_not_found_error, resolve_repos,
    setup_db_and_resolve_repos, validate_directory_path, validate_file_path,
};

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
