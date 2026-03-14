//! Scan command arguments.
//!
//! Contains the `ScanArgs` struct with clap attributes for CLI parsing.

use clap::Parser;

#[derive(Parser)]
#[command(
    about = "Index documents in repositories",
    after_help = "\
EXAMPLES:
    factbase scan --repo myrepo
    factbase scan --dry-run --stats
    factbase scan --since 1d --no-links
    factbase scan --verify --fix -y
    factbase scan --progress | less -R
    factbase scan --no-progress
"
)]
pub struct ScanArgs {
    #[arg(long, help = "Show per-file processing details")]
    pub detailed: bool,
    #[arg(long, short = 'q', help = "Suppress output except errors")]
    pub quiet: bool,
    #[arg(long, short = 'j', help = "Output as JSON")]
    pub json: bool,
    #[arg(
        long,
        help = "Preview changes without modifying database or calling Ollama"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        short = 'w',
        help = "Watch for file changes and rescan automatically"
    )]
    pub watch: bool,
    #[arg(
        long,
        help = "Check for duplicate or near-duplicate documents (similarity > 0.95)"
    )]
    pub check_duplicates: bool,
    #[arg(
        long,
        visible_alias = "profile",
        help = "Show timing statistics for each scan phase"
    )]
    pub stats: bool,
    #[arg(
        long,
        help = "Only process files modified since date (ISO 8601 or relative: 1h, 1d, 1w)"
    )]
    pub since: Option<String>,
    #[arg(
        long,
        help = "Show quick statistics without modifying database or calling Ollama"
    )]
    pub stats_only: bool,
    #[arg(
        long,
        help = "Verify document integrity without re-indexing (check file exists, hash matches, ID header present)"
    )]
    pub verify: bool,
    #[arg(
        long,
        help = "Auto-fix integrity issues found by --verify (re-inject headers, update database)"
    )]
    pub fix: bool,
    #[arg(long, short = 'y', help = "Skip confirmation prompts when using --fix")]
    pub yes: bool,
    #[arg(
        long,
        help = "Remove orphaned database entries for deleted files (soft delete by default)"
    )]
    pub prune: bool,
    #[arg(
        long,
        help = "Permanently remove orphaned entries instead of soft delete (use with --prune)"
    )]
    pub hard: bool,
    #[arg(
        long,
        help = "Force re-generation of all embeddings (document and fact-level) even if content unchanged"
    )]
    pub reindex: bool,
    #[arg(
        long,
        help = "Batch size for embedding generation (default: 10, from config)"
    )]
    pub batch_size: Option<usize>,
    #[arg(
        long,
        help = "Timeout in seconds for Ollama API calls (default: from config, max: 300)"
    )]
    pub timeout: Option<u64>,
    #[arg(
        long,
        help = "Skip link detection phase for faster indexing (links can be detected in subsequent scan)"
    )]
    pub no_links: bool,
    #[arg(
        long,
        help = "Skip embedding generation (index documents into DB without calling embedding provider)"
    )]
    pub no_embed: bool,
    #[arg(
        long,
        help = "Force link detection on all documents (useful for migrated/copied KBs)"
    )]
    pub relink: bool,
    #[arg(
        long,
        help = "Validate index integrity for CI (check embeddings exist and dimensions match)"
    )]
    pub check: bool,
    #[arg(
        long,
        help = "Force progress bars even without TTY (useful with less -R)"
    )]
    pub progress: bool,
    #[arg(long, help = "Disable progress bars but keep other output")]
    pub no_progress: bool,
    #[arg(
        long,
        help = "Assess existing files without modifying anything (onboarding report)"
    )]
    pub assess: bool,
    #[arg(
        long,
        help = "Force full re-sync of review questions from all documents (migration/repair)"
    )]
    pub reindex_reviews: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_scan_args_default() {
        let args = ScanArgs::try_parse_from(["scan"]).unwrap();
        assert!(!args.detailed);
        assert!(!args.quiet);
        assert!(!args.json);
        assert!(!args.dry_run);
        assert!(!args.watch);
        assert!(!args.verify);
        assert!(!args.fix);
        assert!(!args.prune);
        assert!(!args.hard);
    }

    #[test]
    fn test_scan_args_quiet_short() {
        let args = ScanArgs::try_parse_from(["scan", "-q"]).unwrap();
        assert!(args.quiet);
    }

    #[test]
    fn test_scan_args_json_short() {
        let args = ScanArgs::try_parse_from(["scan", "-j"]).unwrap();
        assert!(args.json);
    }

    #[test]
    fn test_scan_args_watch_short() {
        let args = ScanArgs::try_parse_from(["scan", "-w"]).unwrap();
        assert!(args.watch);
    }

    #[test]
    fn test_scan_args_verify_fix_yes() {
        let args = ScanArgs::try_parse_from(["scan", "--verify", "--fix", "-y"]).unwrap();
        assert!(args.verify);
        assert!(args.fix);
        assert!(args.yes);
    }

    #[test]
    fn test_scan_args_prune_hard() {
        let args = ScanArgs::try_parse_from(["scan", "--prune", "--hard"]).unwrap();
        assert!(args.prune);
        assert!(args.hard);
    }

    #[test]
    fn test_scan_args_since() {
        let args = ScanArgs::try_parse_from(["scan", "--since", "1d"]).unwrap();
        assert_eq!(args.since, Some("1d".to_string()));
    }

    #[test]
    fn test_scan_args_batch_size() {
        let args = ScanArgs::try_parse_from(["scan", "--batch-size", "20"]).unwrap();
        assert_eq!(args.batch_size, Some(20));
    }

    #[test]
    fn test_scan_args_timeout() {
        let args = ScanArgs::try_parse_from(["scan", "--timeout", "60"]).unwrap();
        assert_eq!(args.timeout, Some(60));
    }

    #[test]
    fn test_scan_args_progress_flags() {
        let args = ScanArgs::try_parse_from(["scan", "--progress"]).unwrap();
        assert!(args.progress);
        assert!(!args.no_progress);

        let args = ScanArgs::try_parse_from(["scan", "--no-progress"]).unwrap();
        assert!(!args.progress);
        assert!(args.no_progress);
    }

    #[test]
    fn test_scan_args_stats_alias() {
        // --profile is a visible alias for --stats
        let args = ScanArgs::try_parse_from(["scan", "--profile"]).unwrap();
        assert!(args.stats);
    }

    #[test]
    fn test_scan_args_invalid_batch_size() {
        let result = ScanArgs::try_parse_from(["scan", "--batch-size", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_args_invalid_timeout() {
        let result = ScanArgs::try_parse_from(["scan", "--timeout", "not_a_number"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_args_relink() {
        let args = ScanArgs::try_parse_from(["scan", "--relink"]).unwrap();
        assert!(args.relink);
    }

    #[test]
    fn test_scan_args_relink_default_false() {
        let args = ScanArgs::try_parse_from(["scan"]).unwrap();
        assert!(!args.relink);
    }

    #[test]
    fn test_scan_args_no_embed() {
        let args = ScanArgs::try_parse_from(["scan", "--no-embed"]).unwrap();
        assert!(args.no_embed);
    }

    #[test]
    fn test_scan_args_no_embed_default_false() {
        let args = ScanArgs::try_parse_from(["scan"]).unwrap();
        assert!(!args.no_embed);
    }

    #[test]
    fn test_scan_args_reindex_reviews() {
        let args = ScanArgs::try_parse_from(["scan", "--reindex-reviews"]).unwrap();
        assert!(args.reindex_reviews);
    }

    #[test]
    fn test_scan_args_reindex_reviews_default_false() {
        let args = ScanArgs::try_parse_from(["scan"]).unwrap();
        assert!(!args.reindex_reviews);
    }
}
