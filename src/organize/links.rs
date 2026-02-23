//! Link redirection for document reorganization.
//!
//! Updates both database links and manual `[[id]]` references in file content.

use std::path::Path;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::{read_file, write_file};
use crate::patterns::MANUAL_LINK_REGEX;

/// Redirect all links from one document to another.
///
/// Updates both:
/// 1. The `document_links` table in the database
/// 2. Manual `[[from_id]]` references in file content
///
/// # Arguments
/// * `db` - Database connection
/// * `from_id` - Source document ID being merged/deleted
/// * `to_id` - Target document ID to redirect links to
/// * `repo_path` - Repository root path for file operations
///
/// # Returns
/// Total count of redirected links (database + file content).
pub fn redirect_links(
    db: &Database,
    from_id: &str,
    to_id: &str,
    repo_path: &Path,
) -> Result<usize, FactbaseError> {
    let db_count = redirect_database_links(db, from_id, to_id)?;
    let file_count = redirect_file_links(db, from_id, to_id, repo_path)?;
    Ok(db_count + file_count)
}

/// Redirect links in the database only.
///
/// Updates the `document_links` table to point to the new target.
pub fn redirect_database_links(
    db: &Database,
    from_id: &str,
    to_id: &str,
) -> Result<usize, FactbaseError> {
    let incoming = db.get_links_to(from_id)?;
    let count = incoming.len();

    for link in &incoming {
        let mut source_links = db.get_links_from(&link.source_id)?;

        for l in &mut source_links {
            if l.target_id == from_id {
                l.target_id = to_id.to_string();
            }
        }

        let detected: Vec<crate::llm::DetectedLink> = source_links
            .into_iter()
            .map(|l| crate::llm::DetectedLink {
                target_id: l.target_id,
                target_title: String::new(),
                mention_text: String::new(),
                context: l.context.unwrap_or_default(),
            })
            .collect();

        db.update_links(&link.source_id, &detected)?;
    }

    Ok(count)
}

/// Redirect manual `[[id]]` links in file content.
///
/// Scans all documents in the repository for `[[from_id]]` references
/// and rewrites them to `[[to_id]]`.
pub fn redirect_file_links(
    db: &Database,
    from_id: &str,
    to_id: &str,
    repo_path: &Path,
) -> Result<usize, FactbaseError> {
    let pattern = format!("[[{from_id}]]");
    let replacement = format!("[[{to_id}]]");
    let mut count = 0;

    // Get all documents that might contain manual links
    let incoming = db.get_links_to(from_id)?;

    for link in &incoming {
        if let Some(doc) = db.get_document(&link.source_id)? {
            let file_path = repo_path.join(&doc.file_path);
            if file_path.exists() {
                let content = read_file(&file_path)?;

                if content.contains(&pattern) {
                    let new_content = content.replace(&pattern, &replacement);
                    let replacements = content.matches(&pattern).count();
                    count += replacements;

                    write_file(&file_path, &new_content)?;
                }
            }
        }
    }

    // Also scan for manual links that might not be in the database
    // (e.g., newly added links not yet indexed)
    let all_docs = db.list_documents(None, None, None, 100_000)?;
    for doc in all_docs {
        // Skip if already processed via incoming links
        if incoming.iter().any(|l| l.source_id == doc.id) {
            continue;
        }

        let file_path = repo_path.join(&doc.file_path);
        if file_path.exists() {
            let content = read_file(&file_path)?;

            if MANUAL_LINK_REGEX.is_match(&content) && content.contains(&pattern) {
                let new_content = content.replace(&pattern, &replacement);
                let replacements = content.matches(&pattern).count();
                count += replacements;

                write_file(&file_path, &new_content)?;
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::insert_test_doc as test_doc;
    use std::fs;

    #[test]
    fn test_redirect_database_links_no_incoming() {
        let (db, _temp) = test_db();
        let count = redirect_database_links(&db, "nonexistent", "target").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_redirect_database_links_with_incoming() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        test_doc(&db, "doc1", "repo1", "Doc 1", "content", "doc1.md");
        test_doc(&db, "doc2", "repo1", "Doc 2", "content", "doc2.md");
        test_doc(&db, "doc3", "repo1", "Doc 3", "links to doc2", "doc3.md");

        // Create link from doc3 to doc2
        db.update_links(
            "doc3",
            &[crate::llm::DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "references".to_string(),
            }],
        )
        .unwrap();

        let count = redirect_database_links(&db, "doc2", "doc1").unwrap();
        assert_eq!(count, 1);

        // Verify link now points to doc1
        let links = db.get_links_from("doc3").unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "doc1");
    }

    #[test]
    fn test_redirect_file_links_no_manual_links() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create file without manual links
        let doc_path = repo_path.join("doc.md");
        fs::write(&doc_path, "# Doc\nNo links here").unwrap();
        test_doc(&db, "doc1", "repo1", "Doc", "No links here", "doc.md");

        let count = redirect_file_links(&db, "abc123", "def456", repo_path).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_redirect_file_links_with_manual_links() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create file with manual link
        let doc_path = repo_path.join("doc.md");
        fs::write(&doc_path, "# Doc\nSee [[abc123]] for details").unwrap();
        test_doc(
            &db,
            "doc1",
            "repo1",
            "Doc",
            "See [[abc123]] for details",
            "doc.md",
        );

        let count = redirect_file_links(&db, "abc123", "def456", repo_path).unwrap();
        assert_eq!(count, 1);

        // Verify file was updated
        let content = fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("[[def456]]"));
        assert!(!content.contains("[[abc123]]"));
    }

    #[test]
    fn test_redirect_file_links_multiple_occurrences() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create file with multiple manual links to same target
        let doc_path = repo_path.join("doc.md");
        fs::write(&doc_path, "# Doc\nSee [[abc123]] and also [[abc123]] again").unwrap();
        test_doc(
            &db,
            "doc1",
            "repo1",
            "Doc",
            "See [[abc123]] and also [[abc123]] again",
            "doc.md",
        );

        let count = redirect_file_links(&db, "abc123", "def456", repo_path).unwrap();
        assert_eq!(count, 2);

        let content = fs::read_to_string(&doc_path).unwrap();
        assert_eq!(content.matches("[[def456]]").count(), 2);
        assert!(!content.contains("[[abc123]]"));
    }

    #[test]
    fn test_redirect_links_combined() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create files
        let doc1_path = repo_path.join("doc1.md");
        let doc2_path = repo_path.join("doc2.md");
        let doc3_path = repo_path.join("doc3.md");
        fs::write(&doc1_path, "# Doc 1\nContent").unwrap();
        fs::write(&doc2_path, "# Doc 2\nContent").unwrap();
        fs::write(&doc3_path, "# Doc 3\nSee [[abc123]] for info").unwrap();

        test_doc(&db, "abc123", "repo1", "Doc 1", "Content", "doc1.md");
        test_doc(&db, "def456", "repo1", "Doc 2", "Content", "doc2.md");
        test_doc(
            &db,
            "ghi789",
            "repo1",
            "Doc 3",
            "See [[abc123]] for info",
            "doc3.md",
        );

        // Create database link
        db.update_links(
            "ghi789",
            &[crate::llm::DetectedLink {
                target_id: "abc123".to_string(),
                target_title: "Doc 1".to_string(),
                mention_text: "Doc 1".to_string(),
                context: "references".to_string(),
            }],
        )
        .unwrap();

        let count = redirect_links(&db, "abc123", "def456", repo_path).unwrap();
        // 1 database link + 1 file link
        assert_eq!(count, 2);

        // Verify database link redirected
        let links = db.get_links_from("ghi789").unwrap();
        assert_eq!(links[0].target_id, "def456");

        // Verify file link redirected
        let content = fs::read_to_string(&doc3_path).unwrap();
        assert!(content.contains("[[def456]]"));
    }
}
