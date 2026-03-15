//! Orphan pruning logic.
//!
//! Contains `cmd_scan_prune` for removing orphaned database entries.

use factbase::database::Database;
use factbase::models::Repository;
use factbase::output::format_json;
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
            let prompt = format!("{action} these entries?");
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
        println!("Total documents: {total_documents}");
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
            println!("\nRun without --dry-run to {action} these entries.");
        } else if !dry_run {
            println!("Pruned: {pruned_count}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use factbase::database::Database;
    use factbase::models::Document;
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db = Database::new(&tmp.path().join("test.db")).unwrap();
        (db, tmp)
    }

    fn make_repo(path: &std::path::Path) -> Repository {
        Repository {
            id: "test".into(),
            name: "test".into(),
            path: path.to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        }
    }

    fn insert_doc(db: &Database, id: &str, repo_id: &str, file_path: &str) {
        let doc = Document {
            id: id.into(),
            repo_id: repo_id.into(),
            file_path: file_path.into(),
            file_hash: "hash".into(),
            title: format!("Doc {id}"),
            doc_type: Some("document".into()),
            content: format!("# Doc {id}\n\nContent."),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();
    }

    #[test]
    fn test_prune_no_orphans() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        // Create file on disk matching the DB entry
        std::fs::write(repo_dir.path().join("doc.md"), "# Doc").unwrap();
        insert_doc(&db, "aaa111", "test", "doc.md");

        let result = cmd_scan_prune(&db, &[repo], true, true, false, true, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_prune_detects_orphans_dry_run() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        // Insert doc in DB but don't create file on disk
        insert_doc(&db, "aaa111", "test", "missing.md");

        let result = cmd_scan_prune(&db, &[repo], false, true, false, true, false);
        assert!(result.is_ok());

        // Document should still exist (dry_run)
        let doc = db.get_document("aaa111").unwrap();
        assert!(doc.is_some());
        assert!(!doc.unwrap().is_deleted);
    }

    #[test]
    fn test_prune_soft_deletes_orphans() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        insert_doc(&db, "aaa111", "test", "missing.md");

        // Not dry_run, not hard_delete, yes=true to skip prompt
        let result = cmd_scan_prune(&db, &[repo], false, true, false, false, true);
        assert!(result.is_ok());

        // get_document filters out soft-deleted docs, so it returns None
        let doc = db.get_document("aaa111").unwrap();
        assert!(
            doc.is_none(),
            "soft-deleted doc should not appear in get_document"
        );
    }

    #[test]
    fn test_prune_hard_deletes_orphans() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        insert_doc(&db, "aaa111", "test", "missing.md");

        // hard_delete=true, yes=true
        let result = cmd_scan_prune(&db, &[repo], false, true, true, false, true);
        assert!(result.is_ok());

        let doc = db.get_document("aaa111").unwrap();
        assert!(doc.is_none(), "hard delete should remove document entirely");
    }

    #[test]
    fn test_prune_skips_already_deleted() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        insert_doc(&db, "aaa111", "test", "missing.md");
        db.mark_deleted("aaa111").unwrap();

        // Already deleted doc should not be counted as orphan
        let result = cmd_scan_prune(&db, &[repo], true, true, false, true, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_prune_mixed_existing_and_orphaned() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        // One file exists, one doesn't
        std::fs::write(repo_dir.path().join("exists.md"), "# Exists").unwrap();
        insert_doc(&db, "aaa111", "test", "exists.md");
        insert_doc(&db, "bbb222", "test", "gone.md");

        let result = cmd_scan_prune(&db, &[repo], false, true, false, false, true);
        assert!(result.is_ok());

        // Existing doc should be untouched
        let doc1 = db.get_document("aaa111").unwrap();
        assert!(doc1.is_some(), "existing doc should still be accessible");

        // Missing doc should be soft-deleted (not visible via get_document)
        let doc2 = db.get_document("bbb222").unwrap();
        assert!(doc2.is_none(), "orphaned doc should be soft-deleted");
    }

    #[test]
    fn test_prune_json_output() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        insert_doc(&db, "aaa111", "test", "missing.md");

        // json_output=true, dry_run=true
        let result = cmd_scan_prune(&db, &[repo], true, true, false, true, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_prune_empty_repo() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        let result = cmd_scan_prune(&db, &[repo], false, true, false, true, false);
        assert!(result.is_ok());
    }
}
