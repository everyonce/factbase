//! Apply command - process answered organization suggestions from _orphans.md.

use super::args::ApplyArgs;
use crate::commands::{print_output, OutputFormat};
use crate::commands::setup::Setup;
use crate::commands::utils::resolve_repos;
use factbase::database::Database;
use factbase::models::Repository;
use factbase::organize::{has_orphans, load_orphan_entries, process_orphan_answers};
use serde::Serialize;
use std::path::PathBuf;

/// Output for apply command.
#[derive(Debug, Serialize)]
pub struct ApplyOutput {
    /// Results per repository
    pub repositories: Vec<RepoApplyResult>,
    /// Total orphans assigned
    pub total_assigned: usize,
    /// Total orphans dismissed
    pub total_dismissed: usize,
    /// Total orphans remaining
    pub total_remaining: usize,
    /// Whether this was a dry run
    pub dry_run: bool,
}

/// Result for a single repository.
#[derive(Debug, Serialize)]
pub struct RepoApplyResult {
    /// Repository ID
    pub repo_id: String,
    /// Repository path
    pub repo_path: PathBuf,
    /// Number of orphans assigned to documents
    pub assigned_count: usize,
    /// Number of orphans dismissed
    pub dismissed_count: usize,
    /// Number of orphans remaining (unanswered)
    pub remaining_count: usize,
    /// Documents that were modified
    pub modified_docs: Vec<String>,
}

/// Preview of changes for dry-run mode.
#[derive(Debug, Serialize)]
pub struct ApplyPreview {
    /// Previews per repository
    pub repositories: Vec<RepoPreview>,
    /// Total answered orphans that would be processed
    pub total_answered: usize,
    /// Total unanswered orphans
    pub total_unanswered: usize,
}

/// Preview for a single repository.
#[derive(Debug, Serialize)]
pub struct RepoPreview {
    /// Repository ID
    pub repo_id: String,
    /// Repository path
    pub repo_path: PathBuf,
    /// Answered orphans (ready to process)
    pub answered: usize,
    /// Unanswered orphans
    pub unanswered: usize,
    /// Pending changes
    pub pending_changes: Vec<PendingChange>,
}

/// A pending change from an answered orphan.
#[derive(Debug, Serialize)]
pub struct PendingChange {
    /// The fact content
    pub content: String,
    /// Action to take
    pub action: String,
    /// Target document ID (if assigning)
    pub target_doc: Option<String>,
}

pub fn run(args: ApplyArgs) -> anyhow::Result<()> {
    let format = OutputFormat::resolve(args.json, args.format);

    let ctx = Setup::new().build()?;
    let (_, db) = (ctx.config, ctx.db);
    let repos = resolve_repos(db.list_repositories()?, args.repo.as_deref())?;

    if args.dry_run {
        run_dry_run(&repos, format, args.detailed)
    } else {
        run_apply(&db, &repos, format, args.detailed)
    }
}

fn run_dry_run(repos: &[Repository], format: OutputFormat, verbose: bool) -> anyhow::Result<()> {
    let mut repo_previews = Vec::new();
    let mut total_answered = 0;
    let mut total_unanswered = 0;

    for repo in repos {
        let repo_path = PathBuf::from(&repo.path);

        let entries = load_orphan_entries(&repo_path)?;
        if entries.is_empty() {
            continue;
        }

        let answered: Vec<_> = entries.iter().filter(|e| e.answered).collect();
        let unanswered = entries.len() - answered.len();

        total_answered += answered.len();
        total_unanswered += unanswered;

        let pending_changes: Vec<PendingChange> = answered
            .iter()
            .map(|e| {
                let answer = e.answer.as_ref().expect("filtered to answered entries");
                let (action, target) =
                    if answer.to_lowercase() == "dismiss" || answer.to_lowercase() == "ignore" {
                        ("dismiss".to_string(), None)
                    } else {
                        ("assign".to_string(), Some(answer.clone()))
                    };
                PendingChange {
                    content: e.content.clone(),
                    action,
                    target_doc: target,
                }
            })
            .collect();

        repo_previews.push(RepoPreview {
            repo_id: repo.id.clone(),
            repo_path: repo_path.clone(),
            answered: answered.len(),
            unanswered,
            pending_changes,
        });
    }

    let preview = ApplyPreview {
        repositories: repo_previews,
        total_answered,
        total_unanswered,
    };

    print_output(format, &preview, || {
        print_preview_table(&preview, verbose);
    })?;

    Ok(())
}

fn run_apply(
    db: &Database,
    repos: &[Repository],
    format: OutputFormat,
    verbose: bool,
) -> anyhow::Result<()> {
    let mut repo_results = Vec::new();
    let mut total_assigned = 0;
    let mut total_dismissed = 0;
    let mut total_remaining = 0;

    for repo in repos {
        let repo_path = PathBuf::from(&repo.path);

        if !has_orphans(&repo_path) {
            continue;
        }

        let result = process_orphan_answers(&repo_path, db)?;

        total_assigned += result.assigned_count;
        total_dismissed += result.dismissed_count;
        total_remaining += result.remaining_count;

        repo_results.push(RepoApplyResult {
            repo_id: repo.id.clone(),
            repo_path: repo_path.clone(),
            assigned_count: result.assigned_count,
            dismissed_count: result.dismissed_count,
            remaining_count: result.remaining_count,
            modified_docs: result.modified_docs,
        });
    }

    let output = ApplyOutput {
        repositories: repo_results,
        total_assigned,
        total_dismissed,
        total_remaining,
        dry_run: false,
    };

    print_output(format, &output, || {
        print_result_table(&output, verbose);
    })?;

    Ok(())
}

fn print_preview_table(preview: &ApplyPreview, verbose: bool) {
    if preview.repositories.is_empty() {
        println!("No orphan files found.");
        return;
    }

    println!("Dry Run - Pending Changes");
    println!("==========================\n");

    for repo in &preview.repositories {
        println!(
            "Repository: {} ({})",
            repo.repo_id,
            repo.repo_path.display()
        );
        println!("  Answered: {}", repo.answered);
        println!("  Unanswered: {}", repo.unanswered);

        if verbose && !repo.pending_changes.is_empty() {
            println!("  Pending changes:");
            for change in &repo.pending_changes {
                let target = change.target_doc.as_deref().unwrap_or("-");
                let content_preview = if change.content.len() > 50 {
                    let mut end = 50;
                    while end > 0 && !change.content.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...", &change.content[..end])
                } else {
                    change.content.clone()
                };
                println!("    {} → {} ({})", content_preview, change.action, target);
            }
        }
        println!();
    }

    println!("Summary");
    println!("-------");
    println!("Total answered (would process): {}", preview.total_answered);
    println!(
        "Total unanswered (would skip): {}",
        preview.total_unanswered
    );
}

fn print_result_table(output: &ApplyOutput, verbose: bool) {
    if output.repositories.is_empty() {
        println!("No orphan files found.");
        return;
    }

    println!("Organization Apply Results");
    println!("==========================\n");

    for repo in &output.repositories {
        println!(
            "Repository: {} ({})",
            repo.repo_id,
            repo.repo_path.display()
        );
        println!("  Assigned: {}", repo.assigned_count);
        println!("  Dismissed: {}", repo.dismissed_count);
        println!("  Remaining: {}", repo.remaining_count);

        if verbose && !repo.modified_docs.is_empty() {
            println!("  Modified documents: {}", repo.modified_docs.join(", "));
        }
        println!();
    }

    println!("Summary");
    println!("-------");
    println!("Total assigned: {}", output.total_assigned);
    println!("Total dismissed: {}", output.total_dismissed);
    println!("Total remaining: {}", output.total_remaining);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_output_struct() {
        let output = ApplyOutput {
            repositories: vec![],
            total_assigned: 5,
            total_dismissed: 2,
            total_remaining: 3,
            dry_run: false,
        };
        assert_eq!(output.total_assigned, 5);
        assert_eq!(output.total_dismissed, 2);
        assert_eq!(output.total_remaining, 3);
        assert!(!output.dry_run);
    }

    #[test]
    fn test_repo_apply_result_struct() {
        let result = RepoApplyResult {
            repo_id: "main".to_string(),
            repo_path: PathBuf::from("/repo"),
            assigned_count: 3,
            dismissed_count: 1,
            remaining_count: 2,
            modified_docs: vec!["abc123".to_string()],
        };
        assert_eq!(result.repo_id, "main");
        assert_eq!(result.assigned_count, 3);
        assert_eq!(result.modified_docs.len(), 1);
    }

    #[test]
    fn test_pending_change_struct() {
        let change = PendingChange {
            content: "Some fact".to_string(),
            action: "assign".to_string(),
            target_doc: Some("abc123".to_string()),
        };
        assert_eq!(change.action, "assign");
        assert_eq!(change.target_doc, Some("abc123".to_string()));
    }

    #[test]
    fn test_preview_struct() {
        let preview = ApplyPreview {
            repositories: vec![],
            total_answered: 10,
            total_unanswered: 5,
        };
        assert_eq!(preview.total_answered, 10);
        assert_eq!(preview.total_unanswered, 5);
    }
}
