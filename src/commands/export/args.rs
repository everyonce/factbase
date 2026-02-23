//! Export command arguments.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    about = "Export documents from a repository",
    after_help = "\
EXAMPLES:
    factbase export myrepo ./backup
    factbase export myrepo ./backup.json --format json
    factbase export myrepo ./backup.yaml --format yaml
    factbase export myrepo ./backup.tar.zst --compress
"
)]
pub struct ExportArgs {
    pub repo: String,
    pub output: PathBuf,
    #[arg(long)]
    pub with_metadata: bool,
    #[arg(long, default_value = "md")]
    pub format: String,
    #[arg(long)]
    pub compress: bool,
    /// Write output to stdout instead of file (only for json/md formats)
    #[arg(long)]
    pub stdout: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_args_defaults() {
        let args = ExportArgs::try_parse_from(["export", "myrepo", "./backup"]).unwrap();
        assert_eq!(args.repo, "myrepo");
        assert_eq!(args.output, PathBuf::from("./backup"));
        assert!(!args.with_metadata);
        assert_eq!(args.format, "md");
        assert!(!args.compress);
        assert!(!args.stdout);
    }

    #[test]
    fn test_export_args_format_json() {
        let args =
            ExportArgs::try_parse_from(["export", "myrepo", "./out.json", "--format", "json"])
                .unwrap();
        assert_eq!(args.format, "json");
    }

    #[test]
    fn test_export_args_format_yaml() {
        let args =
            ExportArgs::try_parse_from(["export", "myrepo", "./out.yaml", "--format", "yaml"])
                .unwrap();
        assert_eq!(args.format, "yaml");
    }

    #[test]
    fn test_export_args_with_metadata() {
        let args = ExportArgs::try_parse_from(["export", "myrepo", "./backup", "--with-metadata"])
            .unwrap();
        assert!(args.with_metadata);
    }

    #[test]
    fn test_export_args_compress() {
        let args =
            ExportArgs::try_parse_from(["export", "myrepo", "./backup.tar.zst", "--compress"])
                .unwrap();
        assert!(args.compress);
    }

    #[test]
    fn test_export_args_stdout() {
        let args = ExportArgs::try_parse_from(["export", "myrepo", "-", "--stdout"]).unwrap();
        assert!(args.stdout);
        assert_eq!(args.output, PathBuf::from("-"));
    }

    #[test]
    fn test_export_args_all_flags() {
        let args = ExportArgs::try_parse_from([
            "export",
            "myrepo",
            "./backup",
            "--with-metadata",
            "--format",
            "json",
            "--compress",
            "--stdout",
        ])
        .unwrap();
        assert!(args.with_metadata);
        assert_eq!(args.format, "json");
        assert!(args.compress);
        assert!(args.stdout);
    }

    #[test]
    fn test_export_args_missing_repo() {
        let result = ExportArgs::try_parse_from(["export"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_args_missing_output() {
        let result = ExportArgs::try_parse_from(["export", "myrepo"]);
        assert!(result.is_err());
    }
}
