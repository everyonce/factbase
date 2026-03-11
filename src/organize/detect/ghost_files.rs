//! Detect ghost files: two files in the same directory sharing a factbase ID or title.
//!
//! After organize/merge, one copy may persist on disk while the DB only tracks the other.
//! This module scans the filesystem to find such duplicates.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::types::GhostFile;
use crate::patterns::extract_heading_title;
use crate::processor::DocumentProcessor;
use crate::progress::ProgressReporter;
use crate::scanner::Scanner;
use std::collections::HashMap;
use std::path::Path;

/// Scan repository directories for files sharing a factbase ID or title within the same folder.
pub fn detect_ghost_files(
    db: &Database,
    repo_id: Option<&str>,
    progress: &ProgressReporter,
) -> Result<Vec<GhostFile>, FactbaseError> {
    let repos = match repo_id {
        Some(rid) => db
            .get_repository(rid)?
            .into_iter()
            .collect::<Vec<_>>(),
        None => db.list_repositories()?,
    };

    // Build lookup: relative_path -> doc_id for all tracked documents
    let mut tracked: HashMap<String, String> = HashMap::new();
    for repo in &repos {
        let docs = db.get_documents_for_repo(&repo.id)?;
        for doc in docs.values().filter(|d| !d.is_deleted) {
            tracked.insert(doc.file_path.clone(), doc.id.clone());
        }
    }

    let mut results = Vec::new();

    for repo in &repos {
        if !repo.path.exists() {
            continue;
        }

        let scanner = Scanner::new(&[]);
        let files = scanner.find_markdown_files(&repo.path);
        progress.report(0, files.len(), "Scanning for ghost files");

        // Group files by directory: dir -> Vec<(relative_path, doc_id, title, line_count)>
        let mut by_dir: HashMap<String, Vec<(String, Option<String>, Option<String>, usize)>> =
            HashMap::new();

        for (i, path) in files.iter().enumerate() {
            progress.report(i + 1, files.len(), "Reading file headers");

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let relative = path
                .strip_prefix(&repo.path)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let dir = Path::new(&relative)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let doc_id = DocumentProcessor::extract_id_static(&content);
            let title = extract_heading_title(&content);
            let line_count = content.lines().count();

            by_dir
                .entry(dir)
                .or_default()
                .push((relative, doc_id, title, line_count));
        }

        // Detect duplicates within each directory
        for entries in by_dir.values() {
            // Check for shared factbase IDs
            let mut by_id: HashMap<&str, Vec<&(String, Option<String>, Option<String>, usize)>> =
                HashMap::new();
            for entry in entries {
                if let Some(ref id) = entry.1 {
                    by_id.entry(id.as_str()).or_default().push(entry);
                }
            }
            for (doc_id, group) in &by_id {
                if group.len() < 2 {
                    continue;
                }
                // Find which file the DB tracks
                let (tracked_entry, ghosts): (Vec<_>, Vec<_>) = group
                    .iter()
                    .partition(|e| tracked.get(&e.0).is_some_and(|id| id == *doc_id));

                // Safe: group.len() >= 2 checked above, so group.first() always succeeds
                let tracked_entry = tracked_entry.first().or(group.first()).expect("group is non-empty (len >= 2)");
                let title = tracked_entry
                    .2
                    .clone()
                    .unwrap_or_else(|| doc_id.to_string());

                let ghost_iter = if ghosts.is_empty() {
                    // All files claim the same ID but none matches the DB path —
                    // treat all but the first as ghosts
                    group[1..].to_vec()
                } else {
                    ghosts
                };

                for ghost in ghost_iter {
                    results.push(GhostFile {
                        doc_id: doc_id.to_string(),
                        title: title.clone(),
                        tracked_path: tracked_entry.0.clone(),
                        ghost_path: ghost.0.clone(),
                        tracked_lines: tracked_entry.3,
                        ghost_lines: ghost.3,
                        reason: "same_id".to_string(),
                    });
                }
            }

            // Check for shared titles (only among files NOT already flagged by ID)
            let flagged_paths: std::collections::HashSet<&str> = results
                .iter()
                .flat_map(|g| [g.tracked_path.as_str(), g.ghost_path.as_str()])
                .collect();

            let mut by_title: HashMap<&str, Vec<&(String, Option<String>, Option<String>, usize)>> =
                HashMap::new();
            for entry in entries {
                if flagged_paths.contains(entry.0.as_str()) {
                    continue;
                }
                if let Some(ref title) = entry.2 {
                    by_title.entry(title.as_str()).or_default().push(entry);
                }
            }
            for (title, group) in &by_title {
                if group.len() < 2 {
                    continue;
                }
                // Prefer the DB-tracked file as the "tracked" one
                let (tracked_entries, others): (Vec<_>, Vec<_>) =
                    group.iter().partition(|e| tracked.contains_key(&e.0));

                // Safe: group.len() >= 2 checked above, so group.first() always succeeds
                let primary = tracked_entries.first().or(group.first()).expect("group is non-empty (len >= 2)");
                let doc_id = primary
                    .1
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());

                let ghost_iter = if others.is_empty() {
                    group[1..].to_vec()
                } else {
                    others
                };

                for ghost in ghost_iter {
                    results.push(GhostFile {
                        doc_id: doc_id.clone(),
                        title: title.to_string(),
                        tracked_path: primary.0.clone(),
                        ghost_path: ghost.0.clone(),
                        tracked_lines: primary.3,
                        ghost_lines: ghost.3,
                        reason: "same_title".to_string(),
                    });
                }
            }
        }
    }

    results.sort_by(|a, b| a.ghost_path.cmp(&b.ghost_path));
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db};
    use crate::models::Document;

    fn upsert_doc(db: &Database, id: &str, repo_id: &str, file_path: &str, content: &str) {
        let doc = Document {
            id: id.to_string(),
            repo_id: repo_id.to_string(),
            file_path: file_path.to_string(),
            content: content.to_string(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();
    }

    #[test]
    fn test_no_ghosts_when_single_file() {
        let (db, tmp) = test_db();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();
        test_repo_in_db(&db, "test", &repo_dir);

        std::fs::write(
            repo_dir.join("doc.md"),
            "<!-- factbase:aaa111 -->\n# Doc\nContent",
        )
        .unwrap();
        upsert_doc(&db, "aaa111", "test", "doc.md", "<!-- factbase:aaa111 -->\n# Doc\nContent");

        let results = detect_ghost_files(&db, Some("test"), &ProgressReporter::Silent).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_detects_same_id_ghost() {
        let (db, tmp) = test_db();
        let repo_dir = tmp.path().join("repo");
        let sub = repo_dir.join("entities");
        std::fs::create_dir_all(&sub).unwrap();
        test_repo_in_db(&db, "test", &repo_dir);

        let content_a = "<!-- factbase:aaa111 -->\n# Entity\nShort content";
        let content_b =
            "<!-- factbase:aaa111 -->\n# Entity\nMuch longer content\nwith more lines\nand details";

        std::fs::write(sub.join("overview.md"), content_a).unwrap();
        std::fs::write(sub.join("entity-name.md"), content_b).unwrap();

        upsert_doc(&db, "aaa111", "test", "entities/entity-name.md", content_b);

        let results = detect_ghost_files(&db, Some("test"), &ProgressReporter::Silent).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "aaa111");
        assert_eq!(results[0].tracked_path, "entities/entity-name.md");
        assert_eq!(results[0].ghost_path, "entities/overview.md");
        assert_eq!(results[0].reason, "same_id");
    }

    #[test]
    fn test_detects_same_title_ghost() {
        let (db, tmp) = test_db();
        let repo_dir = tmp.path().join("repo");
        let sub = repo_dir.join("items");
        std::fs::create_dir_all(&sub).unwrap();
        test_repo_in_db(&db, "test", &repo_dir);

        let content_a = "<!-- factbase:aaa111 -->\n# Same Title\nContent A";
        let content_b = "<!-- factbase:bbb222 -->\n# Same Title\nContent B with more";

        std::fs::write(sub.join("overview.md"), content_a).unwrap();
        std::fs::write(sub.join("item-name.md"), content_b).unwrap();

        upsert_doc(&db, "aaa111", "test", "items/overview.md", content_a);
        upsert_doc(&db, "bbb222", "test", "items/item-name.md", content_b);

        let results = detect_ghost_files(&db, Some("test"), &ProgressReporter::Silent).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].reason, "same_title");
    }

    #[test]
    fn test_different_dirs_not_flagged() {
        let (db, tmp) = test_db();
        let repo_dir = tmp.path().join("repo");
        let dir_a = repo_dir.join("a");
        let dir_b = repo_dir.join("b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        test_repo_in_db(&db, "test", &repo_dir);

        let content = "<!-- factbase:aaa111 -->\n# Entity\nContent";
        std::fs::write(dir_a.join("doc.md"), content).unwrap();
        std::fs::write(dir_b.join("doc.md"), content).unwrap();

        upsert_doc(&db, "aaa111", "test", "a/doc.md", content);

        let results = detect_ghost_files(&db, Some("test"), &ProgressReporter::Silent).unwrap();
        assert!(results.is_empty(), "Files in different dirs should not be flagged");
    }

    #[test]
    fn test_no_repo_path_skipped() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "gone", Path::new("/nonexistent/path/xyz"));

        let results = detect_ghost_files(&db, Some("gone"), &ProgressReporter::Silent).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_line_counts_reported() {
        let (db, tmp) = test_db();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();
        test_repo_in_db(&db, "test", &repo_dir);

        let content_short = "<!-- factbase:aaa111 -->\n# Entity\nLine 1";
        let content_long =
            "<!-- factbase:aaa111 -->\n# Entity\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5";

        std::fs::write(repo_dir.join("overview.md"), content_short).unwrap();
        std::fs::write(repo_dir.join("entity.md"), content_long).unwrap();

        upsert_doc(&db, "aaa111", "test", "entity.md", content_long);

        let results = detect_ghost_files(&db, Some("test"), &ProgressReporter::Silent).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tracked_lines, 7);
        assert_eq!(results[0].ghost_lines, 3);
    }
}
