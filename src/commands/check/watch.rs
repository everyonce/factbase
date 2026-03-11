//! Watch mode for check command.
//!
//! Monitors file changes and re-lints affected repositories.

use crate::commands::watch_helper::WatchContext;
use factbase::database::Database;
use factbase::models::Repository;
use factbase::patterns::MANUAL_LINK_REGEX;
use factbase::watcher::find_repo_for_path;
use std::collections::HashSet;
use std::time::Duration as StdDuration;

/// Run lint in watch mode, monitoring for file changes.
///
/// This function blocks indefinitely, re-linting repositories when files change.
/// Press Ctrl+C to stop.
///
/// Note: Uses custom inline lint logic for performance rather than re-running
/// the full check command. Only checks broken links, stubs, and orphans.
pub fn run_check_watch_mode(
    ctx: &mut WatchContext,
    db: &Database,
    min_length: usize,
    quiet: bool,
) -> anyhow::Result<()> {
    loop {
        if let Some(changed_paths) = ctx.check_changes() {
            // Find which repo the changed file belongs to
            if let Some(path) = changed_paths.first() {
                if let Some(repo) = find_repo_for_path(path, &ctx.repos) {
                    run_quick_check(repo, db, min_length, quiet)?;
                }
            }
        }

        std::thread::sleep(StdDuration::from_millis(100));
    }
}

/// Run a quick lint check on a single repository.
fn run_quick_check(
    repo: &Repository,
    db: &Database,
    min_length: usize,
    quiet: bool,
) -> anyhow::Result<()> {
    if !quiet {
        println!("\n--- Re-checking {} ({}) ---", repo.name, repo.id);
    }

    let docs = db.list_documents(None, Some(&repo.id), None, 10000)?;
    let doc_ids: HashSet<_> = docs.iter().map(|d| d.id.as_str()).collect();

    // Build mapping from document ID to filename stem for readable link suggestions
    let doc_id_to_stem: std::collections::HashMap<&str, &str> = docs
        .iter()
        .filter_map(|d| {
            std::path::Path::new(&d.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|stem| (d.id.as_str(), stem))
        })
        .collect();

    // Batch fetch links for all documents (2 queries instead of 2*N)
    let doc_id_refs: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let all_links = db.get_links_for_documents(&doc_id_refs)?;

    let mut watch_errors = 0;
    let mut watch_warnings = 0;

    for doc in &docs {
        // Check for broken links and hex-ID link style
        for cap in MANUAL_LINK_REGEX.captures_iter(&doc.content) {
            let link_id = &cap[1];
            if !doc_ids.contains(link_id) {
                if !quiet {
                    println!(
                        "  ERROR: Broken link [[{}]] in {} [{}]",
                        link_id, doc.title, doc.id
                    );
                }
                watch_errors += 1;
            } else if let Some(stem) = doc_id_to_stem.get(link_id) {
                if !quiet {
                    println!(
                        "  WARN: Prefer [[{}]] over [[{}]] in {} [{}]",
                        stem, link_id, doc.title, doc.id
                    );
                }
                watch_warnings += 1;
            }
        }

        // Check stub documents
        if doc.content.len() < min_length {
            if !quiet {
                println!(
                    "  WARN: Stub document ({} chars): {} [{}]",
                    doc.content.len(),
                    doc.title,
                    doc.id
                );
            }
            watch_warnings += 1;
        }

        // Check orphan documents using pre-fetched links
        let (links_from, links_to) = all_links
            .get(&doc.id)
            .map_or((&[][..], &[][..]), |(out, inc)| {
                (out.as_slice(), inc.as_slice())
            });
        if links_from.is_empty() && links_to.is_empty() {
            if !quiet {
                println!(
                    "  WARN: Orphan document (no links): {} [{}]",
                    doc.title, doc.id
                );
            }
            watch_warnings += 1;
        }
    }

    if !quiet {
        if watch_errors == 0 && watch_warnings == 0 {
            println!("✓ No issues found");
        } else {
            println!("Found {watch_errors} error(s), {watch_warnings} warning(s)");
        }
    }

    Ok(())
}
