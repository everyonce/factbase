mod args;
mod execute;
mod output;

pub use args::GrepArgs;

use super::watch_helper;
use super::{
    filter_by_excluded_types, parse_since_filter, print_output, OutputFormat,
};
use crate::commands::setup::Setup;
use execute::{run_grep_watch_mode, run_single_grep};
use regex::RegexBuilder;

pub fn cmd_grep(args: GrepArgs) -> anyhow::Result<()> {
    // Validate regex pattern first
    if let Err(e) = RegexBuilder::new(&args.pattern)
        .case_insensitive(true)
        .build()
    {
        anyhow::bail!("Invalid regex pattern: {e}");
    }

    // Handle watch mode
    if args.watch {
        return run_grep_watch_mode(&args);
    }

    // Handle dry-run mode
    if args.dry_run {
        let db = Setup::new().build()?.db;
        let repos = db.list_repositories_with_stats()?;
        let (repo_count, doc_count) = match &args.repo {
            Some(repo_id) => {
                let filtered: Vec<_> = repos.iter().filter(|(r, _)| r.id == *repo_id).collect();
                (
                    filtered.len(),
                    filtered.iter().map(|(_, c)| *c).sum::<usize>(),
                )
            }
            None => (repos.len(), repos.iter().map(|(_, c)| *c).sum::<usize>()),
        };
        let since_msg = if let Some(ref s) = args.since {
            format!(" (modified since {s})")
        } else {
            String::new()
        };
        println!("Would search {doc_count} document(s) in {repo_count} repository(ies){since_msg}");
        return Ok(());
    }

    run_single_grep(&args)
}

#[cfg(test)]
mod tests {
    use super::output::highlight_matches;
    use factbase::ansi;
    use regex::RegexBuilder;

    #[test]
    fn test_highlight_matches_single() {
        let re = RegexBuilder::new("test")
            .case_insensitive(true)
            .build()
            .unwrap();
        let result = highlight_matches("this is a test string", &re);
        assert!(result.contains(&format!("{}test{}", ansi::BOLD_RED, ansi::RESET)));
    }

    #[test]
    fn test_highlight_matches_multiple() {
        let re = RegexBuilder::new("test")
            .case_insensitive(true)
            .build()
            .unwrap();
        let result = highlight_matches("test one test two", &re);
        assert_eq!(
            result.matches(ansi::BOLD_RED).count(),
            2,
            "Should have 2 highlight starts"
        );
    }

    #[test]
    fn test_highlight_matches_case_insensitive() {
        let re = RegexBuilder::new("test")
            .case_insensitive(true)
            .build()
            .unwrap();
        let result = highlight_matches("TEST and Test and test", &re);
        assert_eq!(
            result.matches(ansi::BOLD_RED).count(),
            3,
            "Should highlight all case variants"
        );
    }

    #[test]
    fn test_highlight_matches_no_match() {
        let re = RegexBuilder::new("xyz")
            .case_insensitive(true)
            .build()
            .unwrap();
        let result = highlight_matches("this is a test string", &re);
        assert_eq!(result, "this is a test string");
        assert!(!result.contains("\x1b["));
    }

    #[test]
    fn test_highlight_matches_special_chars() {
        // Test that special regex characters in the pattern are escaped
        let re = RegexBuilder::new(r"\[test\]")
            .case_insensitive(true)
            .build()
            .unwrap();
        let result = highlight_matches("this is [test] string", &re);
        assert!(result.contains(&format!("{}[test]{}", ansi::BOLD_RED, ansi::RESET)));
    }

    #[test]
    fn test_regex_validation_valid() {
        // Valid regex patterns should compile
        let valid_patterns = ["TODO", "test.*pattern", r"\d+", "foo|bar", "[a-z]+"];
        for pattern in valid_patterns {
            let result = RegexBuilder::new(pattern).case_insensitive(true).build();
            assert!(result.is_ok(), "Pattern '{}' should be valid", pattern);
        }
    }

    #[test]
    fn test_regex_validation_invalid() {
        // Invalid regex patterns should fail
        let invalid_patterns = ["[unclosed", "(unmatched", "*invalid"];
        for pattern in invalid_patterns {
            let result = RegexBuilder::new(pattern).case_insensitive(true).build();
            assert!(result.is_err(), "Pattern '{}' should be invalid", pattern);
        }
    }

    #[test]
    fn test_grep_args_watch_flag() {
        use super::GrepArgs;
        use clap::Parser;
        let args = GrepArgs::try_parse_from(["grep", "-w", "TODO"]).unwrap();
        assert!(args.watch);
        assert_eq!(args.pattern, "TODO");
    }

    #[test]
    fn test_grep_args_watch_long_flag() {
        use super::GrepArgs;
        use clap::Parser;
        let args = GrepArgs::try_parse_from(["grep", "--watch", "pattern"]).unwrap();
        assert!(args.watch);
        assert_eq!(args.pattern, "pattern");
    }

    #[test]
    fn test_grep_args_watch_with_other_flags() {
        use super::GrepArgs;
        use clap::Parser;
        let args =
            GrepArgs::try_parse_from(["grep", "-w", "-r", "myrepo", "-l", "20", "TODO"]).unwrap();
        assert!(args.watch);
        assert_eq!(args.repo, Some("myrepo".to_string()));
        assert_eq!(args.limit, 20);
        assert_eq!(args.pattern, "TODO");
    }
}
