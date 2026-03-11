//! Import command for importing documents into a repository.
//!
//! # Module Organization
//!
//! - `args` - Command arguments (`ImportArgs`)
//! - `validate` - Document validation (internal)
//! - `formats` - Format handlers (JSON, tar.zst, directory)
//!
//! # Public API
//!
//! - [`cmd_import`] - Main command entry point
//! - [`ImportArgs`] - Command arguments

mod args;
mod formats;
mod validate;

pub use args::ImportArgs;

use super::{validate_file_path};
use crate::commands::setup::Setup;
use factbase::ProgressReporter;
use formats::{import_directory, import_json};

#[cfg(feature = "compression")]
use formats::{import_json_zst, import_md_zst, import_tar_zst};

/// Import documents into a repository.
pub fn cmd_import(args: ImportArgs) -> anyhow::Result<()> {
    let db = Setup::new().build()?.db;

    let repo = db.require_repository(&args.repo)?;

    validate_file_path(&args.input)?;

    let progress = ProgressReporter::Cli { quiet: false };
    let input_str = args.input.to_string_lossy();

    #[cfg(feature = "compression")]
    {
        if input_str.ends_with(".tar.zst") {
            return import_tar_zst(&args, &repo, &progress);
        }
        if input_str.ends_with(".json.zst") {
            return import_json_zst(&args, &repo, &progress);
        }
        if input_str.ends_with(".md.zst") {
            return import_md_zst(&args, &repo, &progress);
        }
    }
    #[cfg(not(feature = "compression"))]
    {
        if input_str.ends_with(".tar.zst")
            || input_str.ends_with(".json.zst")
            || input_str.ends_with(".md.zst")
        {
            anyhow::bail!(
                "Compressed import requires the 'compression' feature. Build with: cargo build --features compression"
            );
        }
    }
    if input_str.ends_with(".json") {
        return import_json(&args, &repo, &progress);
    }

    import_directory(&args, &repo, &progress)
}
