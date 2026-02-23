//! Incremental check tracking.
//!
//! Handles filtering documents by modification time and tracking last lint timestamps.

use chrono::{DateTime, Utc};
use factbase::{Database, Document, Repository};
use std::fs;
use std::path::Path;
use tracing::info;

/// Determine the effective since filter for incremental checking.
///
/// Priority:
/// 1. Explicit --since flag takes precedence
/// 2. --incremental uses repo.last_check_at
/// 3. Otherwise, no filtering (returns None)
pub fn get_effective_since(
    explicit_since: Option<DateTime<Utc>>,
    incremental: bool,
    repo: &Repository,
    is_table_format: bool,
) -> Option<DateTime<Utc>> {
    if let Some(since_dt) = explicit_since {
        Some(since_dt)
    } else if incremental {
        if let Some(last_check) = repo.last_check_at {
            if is_table_format {
                println!(
                    "  Incremental mode: checking files modified since {}",
                    last_check.format("%Y-%m-%d %H:%M:%S")
                );
            }
            Some(last_check)
        } else {
            if is_table_format {
                println!("  Incremental mode: no previous check, checking all files");
            }
            None
        }
    } else {
        None
    }
}

/// Filter documents by modification time.
///
/// Returns documents modified since the given timestamp.
/// Documents where modification time cannot be determined are included.
pub fn filter_documents_by_time(
    docs: Vec<Document>,
    since: DateTime<Utc>,
    repo_path: &Path,
) -> Vec<Document> {
    docs.into_iter()
        .filter(|doc| {
            // Check file modification time
            // Construct absolute path from repo path + relative file path
            let abs_path = repo_path.join(&doc.file_path);
            if let Ok(metadata) = fs::metadata(&abs_path) {
                if let Ok(modified) = metadata.modified() {
                    let modified_dt: DateTime<Utc> = modified.into();
                    return modified_dt >= since;
                }
            }
            // Include document if we can't check modification time
            true
        })
        .collect()
}

/// Update last_check_at timestamp for repositories after successful lint.
pub fn update_check_timestamps(
    db: &Database,
    repos: &[Repository],
    lint_time: DateTime<Utc>,
    is_table_format: bool,
) -> anyhow::Result<()> {
    for repo in repos {
        db.update_last_check_at(&repo.id, lint_time)?;
    }
    if is_table_format {
        info!(
            "Updated last_check_at to {} for {} repository(ies)",
            lint_time.format("%Y-%m-%d %H:%M:%S"),
            repos.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::{make_test_doc, make_test_repo};
    use chrono::TimeZone;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_repo(last_check_at: Option<DateTime<Utc>>) -> Repository {
        Repository {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: PathBuf::from("/tmp"),
            last_check_at,
            ..make_test_repo()
        }
    }

    fn test_doc(id: &str, file_path: &str) -> Document {
        Document {
            repo_id: "test".to_string(),
            doc_type: Some("note".to_string()),
            file_path: file_path.to_string(),
            ..make_test_doc(id)
        }
    }

    #[test]
    fn test_get_effective_since_explicit_takes_precedence() {
        let explicit = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let repo = test_repo(Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()));

        let result = get_effective_since(Some(explicit), true, &repo, false);
        assert_eq!(result, Some(explicit));
    }

    #[test]
    fn test_get_effective_since_incremental_uses_last_check() {
        let last_check = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
        let repo = test_repo(Some(last_check));

        let result = get_effective_since(None, true, &repo, false);
        assert_eq!(result, Some(last_check));
    }

    #[test]
    fn test_get_effective_since_no_filter() {
        let repo = test_repo(None);

        let result = get_effective_since(None, false, &repo, false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_effective_since_incremental_no_last_check() {
        let repo = test_repo(None);

        let result = get_effective_since(None, true, &repo, false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_filter_documents_by_time_includes_recent() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("recent.md");
        fs::write(&file_path, "content").unwrap();

        let doc = test_doc("abc123", "recent.md");
        let since = Utc::now() - chrono::Duration::hours(1);

        let result = filter_documents_by_time(vec![doc], since, temp.path());
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_documents_by_time_excludes_old() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("old.md");
        fs::write(&file_path, "content").unwrap();

        let doc = test_doc("abc123", "old.md");
        // Use a future timestamp so the file appears old
        let since = Utc::now() + chrono::Duration::hours(1);

        let result = filter_documents_by_time(vec![doc], since, temp.path());
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_filter_documents_by_time_includes_missing_file() {
        let temp = TempDir::new().unwrap();
        // Don't create the file - it doesn't exist
        let doc = test_doc("abc123", "nonexistent.md");
        let since = Utc::now() - chrono::Duration::hours(1);

        let result = filter_documents_by_time(vec![doc], since, temp.path());
        // Should include doc when file metadata unavailable
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_documents_by_time_empty_input() {
        let temp = TempDir::new().unwrap();
        let since = Utc::now();

        let result = filter_documents_by_time(vec![], since, temp.path());
        assert!(result.is_empty());
    }
}
