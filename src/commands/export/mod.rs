//! Export command implementation.
//!
//! # Module Organization
//!
//! - `args` - Command arguments ([`ExportArgs`])
//! - `json` - JSON and YAML export handlers
//! - `markdown` - Markdown export handlers (single file, directory)
//! - `archive` - Compressed tar.zst archive export (requires `compression` feature)
//!
//! # Public API
//!
//! - [`ExportArgs`] - Command arguments struct
//! - [`cmd_export`] - Main export command function
//! - [`is_single_file_output`] - Check if output path is a single file

#[cfg(feature = "compression")]
mod archive;
mod args;
mod json;
mod markdown;

pub use args::ExportArgs;

use super::setup::Setup;
use super::paths::ends_with_ext;
use factbase::ProgressReporter;

/// Determine if the output path represents a single file (vs directory)
pub fn is_single_file_output(path: &str) -> bool {
    ends_with_ext(path, ".md")
        || ends_with_ext(path, ".json")
        || ends_with_ext(path, ".yaml")
        || ends_with_ext(path, ".md.zst")
        || ends_with_ext(path, ".json.zst")
        || ends_with_ext(path, ".tar.zst")
}

/// Determine the effective export format from path extension
/// Used in tests to verify format detection logic
#[cfg(test)]
pub fn detect_format_from_path(path: &str) -> Option<&'static str> {
    if ends_with_ext(path, ".json") || ends_with_ext(path, ".json.zst") {
        Some("json")
    } else if ends_with_ext(path, ".yaml") {
        Some("yaml")
    } else if ends_with_ext(path, ".md")
        || ends_with_ext(path, ".md.zst")
        || ends_with_ext(path, ".tar.zst")
    {
        Some("md")
    } else {
        None
    }
}

/// Check if the path indicates compression should be used
/// Used in tests to verify compression detection logic
#[cfg(test)]
pub fn should_compress_from_path(path: &str) -> bool {
    path.ends_with(".zst")
}

pub fn cmd_export(args: ExportArgs) -> anyhow::Result<()> {
    #[cfg(not(feature = "compression"))]
    if args.compress {
        anyhow::bail!(
            "Compression requires the 'compression' feature. Build with: cargo build --features compression"
        );
    }

    // Validate --stdout usage
    if args.stdout {
        if args.compress {
            anyhow::bail!("--stdout cannot be used with --compress");
        }
        if args.format != "json" && args.format != "md" && args.format != "yaml" {
            anyhow::bail!("--stdout only works with --format json, md, or yaml");
        }
    }

    let db = Setup::new().build()?.db;

    let repo = db.require_repository(&args.repo)?;

    let docs = db.list_documents(None, Some(&args.repo), None, usize::MAX)?;

    if docs.is_empty() {
        println!("No documents to export");
        return Ok(());
    }

    let progress = ProgressReporter::Cli { quiet: false };
    let output_str = args.output.to_string_lossy();
    let is_single_file = is_single_file_output(&output_str);

    match args.format.as_str() {
        "json" => {
            json::export_json(
                &docs,
                &db,
                &args.output,
                args.compress,
                args.stdout,
                &progress,
            )?;
        }
        "yaml" => {
            json::export_yaml(&docs, &db, &args.output, args.stdout, &progress)?;
        }
        _ => {
            // Markdown format (default)
            if args.stdout {
                markdown::export_markdown_stdout(&docs, &db, args.with_metadata, &progress)?;
            } else if is_single_file
                && (output_str.ends_with(".md") || output_str.ends_with(".md.zst"))
            {
                markdown::export_markdown_single_file(
                    &docs,
                    &db,
                    &args.output,
                    args.with_metadata,
                    args.compress,
                    &progress,
                )?;
            } else if args.compress {
                #[cfg(feature = "compression")]
                {
                    archive::export_archive(
                        &docs,
                        &db,
                        &args.output,
                        &repo.path,
                        args.with_metadata,
                        &progress,
                    )?;
                }
                #[cfg(not(feature = "compression"))]
                unreachable!("compression check at start should have caught this");
            } else {
                markdown::export_markdown_directory(
                    &docs,
                    &db,
                    &args.output,
                    &repo.path,
                    args.with_metadata,
                    &progress,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_single_file_output_md() {
        assert!(is_single_file_output("output.md"));
        assert!(is_single_file_output("/path/to/output.md"));
    }

    #[test]
    fn test_is_single_file_output_json() {
        assert!(is_single_file_output("output.json"));
        assert!(is_single_file_output("/path/to/output.json"));
    }

    #[test]
    fn test_is_single_file_output_yaml() {
        assert!(is_single_file_output("output.yaml"));
        assert!(is_single_file_output("/path/to/output.yaml"));
    }

    #[test]
    fn test_is_single_file_output_compressed() {
        assert!(is_single_file_output("output.md.zst"));
        assert!(is_single_file_output("output.json.zst"));
        assert!(is_single_file_output("output.tar.zst"));
    }

    #[test]
    fn test_is_single_file_output_directory() {
        assert!(!is_single_file_output("output"));
        assert!(!is_single_file_output("/path/to/output"));
        assert!(!is_single_file_output("backup_dir"));
    }

    #[test]
    fn test_detect_format_from_path_json() {
        assert_eq!(detect_format_from_path("output.json"), Some("json"));
        assert_eq!(detect_format_from_path("output.json.zst"), Some("json"));
    }

    #[test]
    fn test_detect_format_from_path_md() {
        assert_eq!(detect_format_from_path("output.md"), Some("md"));
        assert_eq!(detect_format_from_path("output.md.zst"), Some("md"));
        assert_eq!(detect_format_from_path("output.tar.zst"), Some("md"));
    }

    #[test]
    fn test_detect_format_from_path_yaml() {
        assert_eq!(detect_format_from_path("output.yaml"), Some("yaml"));
    }

    #[test]
    fn test_detect_format_from_path_unknown() {
        assert_eq!(detect_format_from_path("output"), None);
        assert_eq!(detect_format_from_path("backup_dir"), None);
    }

    #[test]
    fn test_should_compress_from_path() {
        assert!(should_compress_from_path("output.md.zst"));
        assert!(should_compress_from_path("output.json.zst"));
        assert!(should_compress_from_path("output.tar.zst"));
        assert!(!should_compress_from_path("output.md"));
        assert!(!should_compress_from_path("output.json"));
        assert!(!should_compress_from_path("backup_dir"));
    }
}
