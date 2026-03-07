//! Review answer application logic.

use super::super::{
    parse_since_filter, setup_db_and_resolve_repos,
};
use super::args::ReviewArgs;
use factbase::{
    apply_all_review_answers, config::validate_timeout, extract_inbox_blocks, ApplyConfig,
    ApplyStatus, ProgressReporter,
};
use std::fs;
use std::path::Path;
use tracing::error;

#[tracing::instrument(
    name = "cmd_review_apply",
    skip(args),
    fields(repo = ?args.repo, dry_run = args.dry_run, detailed = args.detailed)
)]
pub async fn cmd_review_apply(args: &ReviewArgs) -> anyhow::Result<()> {
    let (db, repos_to_process) = setup_db_and_resolve_repos(args.repo.as_deref())?;

    let since_filter = parse_since_filter(&args.since)?;

    // Validate timeout if provided
    if let Some(timeout) = args.timeout {
        validate_timeout(timeout)?;
    }

    let progress = ProgressReporter::Cli { quiet: args.quiet };

    // Build repo filter: if single repo, pass it; otherwise None (all repos)
    let repo_filter = if repos_to_process.len() == 1 {
        Some(repos_to_process[0].id.as_str())
    } else {
        None
    };

    let apply_config = ApplyConfig {
        doc_id_filter: None,
        repo_filter,
        dry_run: args.dry_run,
        since: since_filter,
        deadline: None,
        acquire_write_guard: false,
    };

    if args.dry_run && !args.quiet {
        println!("\n--dry-run: showing proposed changes without modifying files\n");
    }

    let result = apply_all_review_answers(&db, &apply_config, &progress).await?;

    if result.filtered_count > 0 && !args.quiet {
        println!(
            "(Filtered {} document(s) by --since)",
            result.filtered_count
        );
    }

    // Print per-document details if --detailed
    if args.detailed && !args.quiet {
        for d in &result.documents {
            match d.status {
                ApplyStatus::Applied => {
                    println!(
                        "  ✓ {} [{}]: {} change(s)",
                        d.doc_title, d.doc_id, d.questions_applied
                    );
                }
                ApplyStatus::DryRun => {
                    println!(
                        "  ~ {} [{}]: {} change(s) (dry run)",
                        d.doc_title, d.doc_id, d.questions_applied
                    );
                }
                ApplyStatus::Error => {
                    println!(
                        "  ✗ {} [{}]: {}",
                        d.doc_title,
                        d.doc_id,
                        d.error.as_deref().unwrap_or("unknown error")
                    );
                }
            }
        }
    }

    if !args.quiet {
        println!("\nSummary:");
        println!("  Processed: {} question(s)", result.total_applied);
        if result.total_errors > 0 {
            println!("  Errors: {}", result.total_errors);
        }
    }

    // --- Inbox block processing ---
    let inbox_docs = collect_inbox_documents(&db, &repos_to_process, since_filter.as_ref())?;

    if !inbox_docs.is_empty() {
        if !args.quiet {
            println!(
                "\nFound {} document(s) with inbox blocks.",
                inbox_docs.len()
            );
        }
        if args.dry_run && !args.quiet {
            println!("--dry-run: showing inbox content without integrating\n");
        }

        let mut inbox_processed = 0;
        let mut inbox_errors = 0;

        for inbox_doc in &inbox_docs {
            if !args.quiet {
                println!(
                    "\nIntegrating inbox: {} [{}]...",
                    inbox_doc.doc_title, inbox_doc.doc_id
                );
            }

            let abs_path = Path::new(&inbox_doc.repo_path).join(&inbox_doc.file_path);
            let content = match fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(e) => {
                    inbox_errors += 1;
                    if !args.quiet {
                        println!("  ✗ Error reading file: {e}");
                    }
                    continue;
                }
            };

            let blocks = extract_inbox_blocks(&content);
            if blocks.is_empty() {
                continue;
            }

            if args.dry_run {
                for (i, block) in blocks.iter().enumerate() {
                    println!(
                        "  Inbox block {} (lines {}-{}):",
                        i + 1,
                        block.start_line + 1,
                        block.end_line + 1
                    );
                    for line in block.content.lines().take(5) {
                        println!("    {line}");
                    }
                    if block.content.lines().count() > 5 {
                        println!("    ...");
                    }
                }
                inbox_processed += 1;
                continue;
            }

            if args.detailed {
                for block in &blocks {
                    println!("  Inbox content:");
                    for line in block.content.lines() {
                        println!("    | {line}");
                    }
                }
            }

            match factbase::apply_inbox_integration(&content, &blocks).await {
                Ok(new_content) => {
                    if let Err(e) = write_file_safely(&abs_path, &new_content) {
                        inbox_errors += 1;
                        if !args.quiet {
                            println!("  ✗ Error writing file: {e}");
                        }
                    } else {
                        inbox_processed += 1;
                        if !args.quiet {
                            println!("  ✓ Inbox integrated and stripped");
                        }
                    }
                }
                Err(e) => {
                    inbox_errors += 1;
                    error!(file = %inbox_doc.file_path, error = %e, "Failed to integrate inbox");
                    if !args.quiet {
                        println!("  ✗ Error: {e}");
                    }
                }
            }
        }

        if !args.quiet {
            println!("\nInbox summary:");
            println!("  Integrated: {inbox_processed} document(s)");
            if inbox_errors > 0 {
                println!("  Errors: {inbox_errors}");
            }
        }
    }

    Ok(())
}

/// Write file atomically using temp file + rename
fn write_file_safely(path: &Path, content: &str) -> anyhow::Result<()> {
    let temp_path = path.with_extension("md.tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Document with inbox blocks
#[derive(Debug, Clone)]
struct InboxDocument {
    doc_id: String,
    doc_title: String,
    file_path: String,
    repo_path: String,
}

/// Collect documents that contain inbox blocks.
fn collect_inbox_documents(
    db: &factbase::Database,
    repos: &[factbase::Repository],
    since_filter: Option<&chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<Vec<InboxDocument>> {
    use super::status::file_modified_since;

    let mut result = Vec::new();

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;

        for doc in docs.values() {
            if doc.is_deleted {
                continue;
            }

            if let Some(since) = since_filter {
                let abs_path = repo.path.join(&doc.file_path);
                if !file_modified_since(&abs_path, since) {
                    continue;
                }
            }

            let blocks = extract_inbox_blocks(&doc.content);
            if !blocks.is_empty() {
                result.push(InboxDocument {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    file_path: doc.file_path.clone(),
                    repo_path: repo.path.to_string_lossy().to_string(),
                });
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_file_safely_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        write_file_safely(&file_path, "Hello, world!").unwrap();

        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "Hello, world!");
    }

    #[test]
    fn test_write_file_safely_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        fs::write(&file_path, "Original content").unwrap();
        write_file_safely(&file_path, "New content").unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "New content");
    }

    #[test]
    fn test_write_file_safely_no_temp_file_remains() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let temp_path = temp_dir.path().join("test.md.tmp");

        write_file_safely(&file_path, "Content").unwrap();

        assert!(
            !temp_path.exists(),
            "Temp file should be renamed, not left behind"
        );
    }

    #[test]
    fn test_write_file_safely_invalid_path() {
        let result = write_file_safely(Path::new("/nonexistent/dir/file.md"), "Content");
        assert!(result.is_err());
    }
}
