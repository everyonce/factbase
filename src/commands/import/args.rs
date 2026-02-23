//! Import command arguments.

use clap::Parser;
use std::path::PathBuf;

/// Import documents into a repository
#[derive(Parser)]
#[command(
    about = "Import documents into a repository",
    after_help = "\
EXAMPLES:
    factbase import myrepo ./backup
    factbase import myrepo ./backup.tar.zst --overwrite
    factbase import myrepo ./backup --include \"people/*\" --validate
"
)]
pub struct ImportArgs {
    pub repo: String,
    pub input: PathBuf,
    #[arg(long)]
    pub overwrite: bool,
    #[arg(long)]
    pub include: Option<String>,
    /// Validate documents before importing (check ID format, temporal tags, source footnotes)
    #[arg(long)]
    pub validate: bool,
    /// Preview import without writing files (use with --validate for validation-only mode)
    #[arg(long)]
    pub dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_import_args_defaults() {
        let args = ImportArgs::try_parse_from(["import", "myrepo", "./backup"]).unwrap();
        assert_eq!(args.repo, "myrepo");
        assert_eq!(args.input, PathBuf::from("./backup"));
        assert!(!args.overwrite);
        assert!(args.include.is_none());
        assert!(!args.validate);
        assert!(!args.dry_run);
    }

    #[test]
    fn test_import_args_overwrite() {
        let args =
            ImportArgs::try_parse_from(["import", "myrepo", "./backup", "--overwrite"]).unwrap();
        assert!(args.overwrite);
    }

    #[test]
    fn test_import_args_include() {
        let args =
            ImportArgs::try_parse_from(["import", "myrepo", "./backup", "--include", "people/*"])
                .unwrap();
        assert_eq!(args.include, Some("people/*".to_string()));
    }

    #[test]
    fn test_import_args_validate() {
        let args =
            ImportArgs::try_parse_from(["import", "myrepo", "./backup", "--validate"]).unwrap();
        assert!(args.validate);
    }

    #[test]
    fn test_import_args_dry_run() {
        let args =
            ImportArgs::try_parse_from(["import", "myrepo", "./backup", "--dry-run"]).unwrap();
        assert!(args.dry_run);
    }

    #[test]
    fn test_import_args_all_flags() {
        let args = ImportArgs::try_parse_from([
            "import",
            "myrepo",
            "./backup.tar.zst",
            "--overwrite",
            "--include",
            "*.md",
            "--validate",
            "--dry-run",
        ])
        .unwrap();
        assert_eq!(args.repo, "myrepo");
        assert_eq!(args.input, PathBuf::from("./backup.tar.zst"));
        assert!(args.overwrite);
        assert_eq!(args.include, Some("*.md".to_string()));
        assert!(args.validate);
        assert!(args.dry_run);
    }

    #[test]
    fn test_import_args_missing_repo() {
        let result = ImportArgs::try_parse_from(["import"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_args_missing_input() {
        let result = ImportArgs::try_parse_from(["import", "myrepo"]);
        assert!(result.is_err());
    }
}
