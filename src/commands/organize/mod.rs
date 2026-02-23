//! Organize command implementation.
//!
//! Reorganize knowledge base with fact-level accounting to prevent data loss.
//!
//! # Subcommands
//!
//! - `analyze` - Detect reorganization opportunities
//! - `merge` - Merge similar documents
//! - `split` - Split multi-topic documents
//! - `move` - Move document to different folder
//! - `retype` - Change document type
//! - `apply` - Process answered organization suggestions

mod analyze;
mod apply;
mod args;
mod merge;
mod r#move;
mod split;

use std::path::Path;

use factbase::{cleanup, create_snapshot, rollback, Database, VerificationResult};

pub use args::{
    AnalyzeArgs, MergeArgs, MoveArgs, OrganizeArgs, OrganizeCommands, RetypeArgs, SplitArgs,
};

/// Execute an organize operation with snapshot-based rollback on failure.
///
/// Creates a snapshot of the affected documents, runs the execute function,
/// verifies the result, and either cleans up on success or rolls back on failure.
pub fn execute_with_snapshot<R>(
    doc_ids: &[&str],
    db: &Database,
    repo_path: &Path,
    operation_name: &str,
    execute_fn: impl FnOnce() -> Result<R, factbase::FactbaseError>,
    verify_fn: impl FnOnce(&R) -> Result<VerificationResult, factbase::FactbaseError>,
) -> anyhow::Result<R> {
    let snapshot = create_snapshot(doc_ids, db, repo_path)?;

    let result = match execute_fn() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{operation_name} failed, rolling back...");
            rollback(&snapshot, db)?;
            return Err(e.into());
        }
    };

    let verification = verify_fn(&result)?;
    if !verification.passed {
        eprintln!(
            "Verification failed: {}",
            verification
                .mismatch_details
                .as_deref()
                .unwrap_or("unknown")
        );
        eprintln!("Rolling back...");
        rollback(&snapshot, db)?;
        anyhow::bail!("{operation_name} verification failed - changes rolled back");
    }

    cleanup(&snapshot)?;
    Ok(result)
}

/// Main entry point for the organize command.
pub async fn cmd_organize(args: OrganizeArgs) -> anyhow::Result<()> {
    match args.command {
        OrganizeCommands::Analyze(args) => analyze::run(args).await,
        OrganizeCommands::Merge(args) => merge::run(args).await,
        OrganizeCommands::Split(args) => split::run(args).await,
        OrganizeCommands::Move(args) => r#move::run(args),
        OrganizeCommands::Retype(args) => cmd_retype(args),
        OrganizeCommands::Apply(args) => apply::run(args),
    }
}

fn cmd_retype(_args: RetypeArgs) -> anyhow::Result<()> {
    anyhow::bail!("Not yet implemented: organize retype")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: TestCommands,
    }

    #[derive(clap::Subcommand)]
    enum TestCommands {
        Organize(OrganizeArgs),
    }

    #[test]
    fn test_organize_analyze_parses() {
        let cli = TestCli::parse_from(["test", "organize", "analyze"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Analyze(a) => {
                    assert!(a.repo.is_none());
                    assert!(!a.json);
                }
                _ => panic!("Expected Analyze"),
            },
        }
    }

    #[test]
    fn test_organize_analyze_with_repo() {
        let cli = TestCli::parse_from(["test", "organize", "analyze", "--repo", "myrepo"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Analyze(a) => {
                    assert_eq!(a.repo, Some("myrepo".to_string()));
                }
                _ => panic!("Expected Analyze"),
            },
        }
    }

    #[test]
    fn test_organize_merge_parses() {
        let cli = TestCli::parse_from(["test", "organize", "merge", "abc123", "def456"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Merge(m) => {
                    assert_eq!(m.doc1, "abc123");
                    assert_eq!(m.doc2, "def456");
                    assert!(m.into.is_none());
                    assert!(!m.dry_run);
                }
                _ => panic!("Expected Merge"),
            },
        }
    }

    #[test]
    fn test_organize_merge_with_into() {
        let cli = TestCli::parse_from([
            "test", "organize", "merge", "abc123", "def456", "--into", "abc123",
        ]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Merge(m) => {
                    assert_eq!(m.into, Some("abc123".to_string()));
                }
                _ => panic!("Expected Merge"),
            },
        }
    }

    #[test]
    fn test_organize_split_parses() {
        let cli = TestCli::parse_from(["test", "organize", "split", "abc123"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Split(s) => {
                    assert_eq!(s.doc_id, "abc123");
                    assert!(s.at.is_none());
                }
                _ => panic!("Expected Split"),
            },
        }
    }

    #[test]
    fn test_organize_split_with_at() {
        let cli = TestCli::parse_from([
            "test",
            "organize",
            "split",
            "abc123",
            "--at",
            "Section Title",
        ]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Split(s) => {
                    assert_eq!(s.at, Some("Section Title".to_string()));
                }
                _ => panic!("Expected Split"),
            },
        }
    }

    #[test]
    fn test_organize_move_parses() {
        let cli = TestCli::parse_from(["test", "organize", "move", "abc123", "--to", "people/"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Move(m) => {
                    assert_eq!(m.doc_id, "abc123");
                    assert_eq!(m.to, "people/");
                }
                _ => panic!("Expected Move"),
            },
        }
    }

    #[test]
    fn test_organize_retype_parses() {
        let cli = TestCli::parse_from(["test", "organize", "retype", "abc123", "--type", "person"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Retype(r) => {
                    assert_eq!(r.doc_id, "abc123");
                    assert_eq!(r.r#type, "person");
                    assert!(!r.persist);
                }
                _ => panic!("Expected Retype"),
            },
        }
    }

    #[test]
    fn test_organize_retype_with_persist() {
        let cli = TestCli::parse_from([
            "test",
            "organize",
            "retype",
            "abc123",
            "--type",
            "person",
            "--persist",
        ]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Retype(r) => {
                    assert!(r.persist);
                }
                _ => panic!("Expected Retype"),
            },
        }
    }

    #[test]
    fn test_organize_apply_parses() {
        let cli = TestCli::parse_from(["test", "organize", "apply"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Apply(a) => {
                    assert!(a.repo.is_none());
                    assert!(!a.dry_run);
                }
                _ => panic!("Expected Apply"),
            },
        }
    }

    #[test]
    fn test_organize_apply_with_dry_run() {
        let cli = TestCli::parse_from(["test", "organize", "apply", "--dry-run"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Apply(a) => {
                    assert!(a.dry_run);
                }
                _ => panic!("Expected Apply"),
            },
        }
    }

    #[test]
    fn test_common_flags_dry_run() {
        // Test --dry-run on merge
        let cli =
            TestCli::parse_from(["test", "organize", "merge", "a", "b", "--dry-run", "--yes"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Merge(m) => {
                    assert!(m.dry_run);
                    assert!(m.yes);
                }
                _ => panic!("Expected Merge"),
            },
        }
    }

    #[test]
    fn test_common_flags_json() {
        // Test --json on analyze
        let cli = TestCli::parse_from(["test", "organize", "analyze", "--json"]);
        match cli.command {
            TestCommands::Organize(args) => match args.command {
                OrganizeCommands::Analyze(a) => {
                    assert!(a.json);
                }
                _ => panic!("Expected Analyze"),
            },
        }
    }
}
