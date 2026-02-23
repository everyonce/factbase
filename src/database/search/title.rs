//! Title search operations.
//!
//! SQL LIKE pattern matching for document titles.
//! Faster than semantic search as it doesn't require embeddings.

use super::{append_type_repo_filters, generate_snippet, push_type_repo_params, SEARCH_COLUMNS};
use crate::database::{decode_content_lossy, Database};
use crate::error::FactbaseError;
use crate::models::SearchResult;

impl Database {
    /// Searches documents by title using SQL LIKE pattern matching.
    ///
    /// Faster than semantic search as it doesn't require embeddings.
    pub fn search_by_title(
        &self,
        title_filter: &str,
        limit: usize,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, FactbaseError> {
        let conn = self.get_conn()?;

        let mut sql = format!(
            "SELECT {SEARCH_COLUMNS}
             FROM documents
             WHERE is_deleted = FALSE
             AND title LIKE ?1",
        );

        append_type_repo_filters(&mut sql, 2, doc_type, repo_id, "");
        sql.push_str(&format!(" ORDER BY title LIMIT {}", limit));

        let mut stmt = conn.prepare_cached(&sql)?;
        let pattern = format!("%{}%", title_filter);

        let mut results = Vec::with_capacity(limit);

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&pattern];
        push_type_repo_params(&mut params, &doc_type, &repo_id);

        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let stored_content: String = row.get(4)?;
            let content = decode_content_lossy(stored_content);
            let snippet = generate_snippet(&content);

            results.push(SearchResult {
                id: row.get(0)?,
                title: row.get(1)?,
                doc_type: row.get(2).ok(),
                file_path: row.get(3)?,
                relevance_score: 1.0,
                snippet,
                highlighted_snippet: None,
                chunk_index: None,
                chunk_start: None,
                chunk_end: None,
            });
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{
        test_db, test_doc, test_doc_with_repo, test_repo, test_repo_with_id,
    };

    #[test]
    fn test_search_by_title() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Project Alpha"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Project Beta"))
            .expect("upsert doc2 should succeed");
        db.upsert_document(&test_doc("doc3", "Meeting Notes"))
            .expect("upsert doc3 should succeed");

        // Search for "Project" - should match 2
        let results = db
            .search_by_title("Project", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 2);

        // Search for "Alpha" - should match 1
        let results = db
            .search_by_title("Alpha", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Alpha");

        // Search for "xyz" - should match 0
        let results = db
            .search_by_title("xyz", 10, None, None)
            .expect("search_by_title should succeed");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_title_with_doc_type_filter() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        // Create docs with different types
        let mut doc1 = test_doc("doc1", "Project Alpha");
        doc1.doc_type = Some("project".to_string());
        db.upsert_document(&doc1).expect("upsert doc1");

        let mut doc2 = test_doc("doc2", "Project Beta");
        doc2.doc_type = Some("person".to_string());
        db.upsert_document(&doc2).expect("upsert doc2");

        // Filter by type "project" - should match 1
        let results = db
            .search_by_title("Project", 10, Some("project"), None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Alpha");

        // Filter by type "person" - should match 1
        let results = db
            .search_by_title("Project", 10, Some("person"), None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Beta");

        // Filter by non-existent type - should match 0
        let results = db
            .search_by_title("Project", 10, Some("concept"), None)
            .expect("search_by_title should succeed");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_title_with_repo_filter() {
        let (db, _tmp) = test_db();

        // Create two repos
        let repo1 = test_repo();
        db.upsert_repository(&repo1).expect("upsert repo1");

        let repo2 = test_repo_with_id("other-repo");
        db.upsert_repository(&repo2).expect("upsert repo2");

        // Create docs in different repos
        db.upsert_document(&test_doc_with_repo("doc1", "test-repo", "Project Alpha"))
            .expect("upsert doc1");
        db.upsert_document(&test_doc_with_repo("doc2", "other-repo", "Project Beta"))
            .expect("upsert doc2");

        // Filter by repo "test-repo" - should match 1
        let results = db
            .search_by_title("Project", 10, None, Some("test-repo"))
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Alpha");

        // Filter by repo "other-repo" - should match 1
        let results = db
            .search_by_title("Project", 10, None, Some("other-repo"))
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Beta");
    }

    #[test]
    fn test_search_by_title_with_combined_filters() {
        let (db, _tmp) = test_db();

        // Create two repos
        let repo1 = test_repo();
        db.upsert_repository(&repo1).expect("upsert repo1");

        let repo2 = test_repo_with_id("other-repo");
        db.upsert_repository(&repo2).expect("upsert repo2");

        // Create docs with different types in different repos
        let mut doc1 = test_doc_with_repo("doc1", "test-repo", "Project Alpha");
        doc1.doc_type = Some("project".to_string());
        db.upsert_document(&doc1).expect("upsert doc1");

        let mut doc2 = test_doc_with_repo("doc2", "test-repo", "Project Beta");
        doc2.doc_type = Some("person".to_string());
        db.upsert_document(&doc2).expect("upsert doc2");

        let mut doc3 = test_doc_with_repo("doc3", "other-repo", "Project Gamma");
        doc3.doc_type = Some("project".to_string());
        db.upsert_document(&doc3).expect("upsert doc3");

        // Filter by type "project" AND repo "test-repo" - should match 1
        let results = db
            .search_by_title("Project", 10, Some("project"), Some("test-repo"))
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Alpha");

        // Filter by type "project" AND repo "other-repo" - should match 1
        let results = db
            .search_by_title("Project", 10, Some("project"), Some("other-repo"))
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Project Gamma");
    }

    #[test]
    fn test_search_by_title_respects_limit() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        // Create 5 docs with "Project" in title
        for i in 1..=5 {
            db.upsert_document(&test_doc(&format!("doc{}", i), &format!("Project {}", i)))
                .expect("upsert doc");
        }

        // Limit to 2 results
        let results = db
            .search_by_title("Project", 2, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 2);

        // Limit to 10 (more than available)
        let results = db
            .search_by_title("Project", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_by_title_case_insensitive() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        db.upsert_document(&test_doc("doc1", "Project Alpha"))
            .expect("upsert doc1");

        // Search with lowercase - SQLite LIKE is case-insensitive for ASCII
        let results = db
            .search_by_title("project", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);

        // Search with uppercase
        let results = db
            .search_by_title("PROJECT", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);

        // Search with mixed case
        let results = db
            .search_by_title("PrOjEcT", 10, None, None)
            .expect("search_by_title should succeed");
        assert_eq!(results.len(), 1);
    }
}
