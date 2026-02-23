//! Orphan pruning logic.
//!
//! Contains `cmd_scan_prune` for removing orphaned database entries.

use factbase::{format_json, Database, Repository};
use serde::Serialize;
use std::path::Path;

use crate::commands::confirm_prompt;

/// Remove orphaned database entries for deleted files
pub(super) fn cmd_scan_prune(
    db: &Database,
    repos: &[Repository],
    json_output: bool,
    quiet: bool,
    hard_delete: bool,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    #[derive(Serialize)]
    struct OrphanedEntry {
        doc_id: String,
        title: String,
        file_path: String,
        repo_id: String,
    }

    #[derive(Serialize)]
    struct PruneResult {
        total_documents: usize,
        orphaned_count: usize,
        pruned_count: usize,
        orphaned: Vec<OrphanedEntry>,
        dry_run: bool,
        hard_delete: bool,
    }

    let mut total_documents = 0;
    let mut orphaned: Vec<OrphanedEntry> = Vec::with_capacity(16); // Typical prune finds few orphans

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;
        let repo_path = Path::new(&repo.path);

        for (_, doc) in docs {
            // Skip already deleted documents
            if doc.is_deleted {
                continue;
            }
            total_documents += 1;

            // Build full file path
            let full_path = repo_path.join(&doc.file_path);

            // Check if file exists
            if !full_path.exists() {
                orphaned.push(OrphanedEntry {
                    doc_id: doc.id.clone(),
                    title: doc.title.clone(),
                    file_path: doc.file_path.clone(),
                    repo_id: repo.id.clone(),
                });
            }
        }
    }

    let mut pruned_count = 0;

    if !orphaned.is_empty() && !dry_run {
        // Prompt for confirmation unless --yes
        let proceed = if yes {
            true
        } else if !quiet {
            let action = if hard_delete {
                "permanently delete"
            } else {
                "soft delete"
            };
            println!("\nFound {} orphaned document(s):", orphaned.len());
            for entry in &orphaned {
                println!(
                    "  - {} [{}] ({})",
                    entry.title, entry.doc_id, entry.file_path
                );
            }
            let prompt = format!("{} these entries?", action);
            confirm_prompt(&prompt)?
        } else {
            false
        };

        if proceed {
            for entry in &orphaned {
                let delete_result = if hard_delete {
                    db.hard_delete_document(&entry.doc_id)
                } else {
                    db.mark_deleted(&entry.doc_id)
                };

                match delete_result {
                    Ok(_) => {
                        pruned_count += 1;
                        if !quiet && !json_output {
                            let action = if hard_delete {
                                "Deleted"
                            } else {
                                "Soft deleted"
                            };
                            println!("✓ {} {} [{}]", action, entry.title, entry.doc_id);
                        }
                    }
                    Err(e) => {
                        if !quiet && !json_output {
                            println!(
                                "✗ Failed to prune {} [{}]: {}",
                                entry.title, entry.doc_id, e
                            );
                        }
                    }
                }
            }
        }
    }

    // Capture counts before moving orphaned
    let orphaned_count = orphaned.len();

    let result = PruneResult {
        total_documents,
        orphaned_count,
        pruned_count,
        orphaned,
        dry_run,
        hard_delete,
    };

    if json_output {
        println!("{}", format_json(&result)?);
    } else if !quiet {
        if dry_run {
            println!("Prune Preview (dry run)");
            println!("=======================");
        } else {
            println!("\nPrune Summary");
            println!("=============");
        }
        println!("Total documents: {}", total_documents);
        println!("Orphaned entries: {}", result.orphaned.len());

        if dry_run && !result.orphaned.is_empty() {
            println!("\nWould prune:");
            for entry in &result.orphaned {
                println!(
                    "  - {} [{}] ({})",
                    entry.title, entry.doc_id, entry.file_path
                );
            }
            let action = if hard_delete {
                "permanently delete"
            } else {
                "soft delete"
            };
            println!("\nRun without --dry-run to {} these entries.", action);
        } else if !dry_run {
            println!("Pruned: {}", pruned_count);
        }
    }

    Ok(())
}
