//! Document listing and filtering operations.

use std::collections::HashMap;

use crate::error::FactbaseError;
use crate::models::Document;

use super::super::Database;
use super::{DocStub, DOCUMENT_COLUMNS};

impl Database {
    /// Retrieves lightweight stubs (id, title, file_path, is_deleted) for all active documents
    /// in a repository. Much cheaper than `get_documents_for_repo` — skips content decompression.
    ///
    /// Use this when only metadata is needed (e.g. link detection filtering).
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_document_stubs_for_repo(
        &self,
        repo_id: &str,
    ) -> Result<HashMap<String, DocStub>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, file_path, is_deleted FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
        )?;
        let mut stubs = HashMap::new();
        let mut rows = stmt.query([repo_id])?;
        while let Some(row) = rows.next()? {
            let stub = DocStub {
                id: row.get(0)?,
                title: row.get(1)?,
                file_path: row.get(2)?,
                is_deleted: row.get(3)?,
            };
            stubs.insert(stub.id.clone(), stub);
        }
        Ok(stubs)
    }

    /// Retrieves all active (non-deleted) documents for a repository as a map keyed by document ID.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_documents_for_repo(
        &self,
        repo_id: &str,
    ) -> Result<HashMap<String, Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE"
        ))?;
        let mut docs = HashMap::new();
        let mut rows = stmt.query([repo_id])?;
        while let Some(row) = rows.next()? {
            let doc = Self::row_to_document(row)?;
            docs.insert(doc.id.clone(), doc);
        }
        Ok(docs)
    }

    /// Get only documents that have a review queue section.
    ///
    /// Much faster than `get_documents_for_repo` when only review
    /// documents are needed, as it skips decompressing non-review content.
    pub fn get_documents_with_review_queue(
        &self,
        repo_id: Option<&str>,
    ) -> Result<Vec<Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(rid) =
            repo_id
        {
            (
                format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE has_review_queue = TRUE AND is_deleted = FALSE AND repo_id = ?1"),
                vec![Box::new(rid.to_string())],
            )
        } else {
            (
                format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE has_review_queue = TRUE AND is_deleted = FALSE"),
                vec![],
            )
        };
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut docs = Vec::new();
        let mut rows = stmt.query(rusqlite::params_from_iter(&params))?;
        while let Some(row) = rows.next()? {
            docs.push(Self::row_to_document(row)?);
        }
        Ok(docs)
    }

    /// Lists documents with optional filters.
    ///
    /// # Arguments
    /// * `doc_type` - Filter by document type
    /// * `repo_id` - Filter by repository
    /// * `title_filter` - Filter by title (LIKE pattern)
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// Vector of matching documents, ordered by title.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn list_documents(
        &self,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
        title_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut sql = format!(
            "SELECT {DOCUMENT_COLUMNS}
             FROM documents WHERE is_deleted = FALSE",
        );

        let mut param_idx = 1;
        if doc_type.is_some() {
            write_str!(sql, " AND doc_type = ?{}", param_idx);
            param_idx += 1;
        }
        if repo_id.is_some() {
            write_str!(sql, " AND repo_id = ?{}", param_idx);
            param_idx += 1;
        }
        if title_filter.is_some() {
            write_str!(sql, " AND title LIKE ?{}", param_idx);
        }

        write_str!(sql, " ORDER BY title LIMIT {}", limit);

        let mut stmt = conn.prepare_cached(&sql)?;
        let mut results = Vec::with_capacity(limit);

        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(3);
        if let Some(ref t) = doc_type {
            params.push(t);
        }
        if let Some(ref r) = repo_id {
            params.push(r);
        }
        if let Some(ref tf) = title_filter {
            params.push(tf);
        }

        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            results.push(Self::row_to_document(row)?);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc_with_repo, test_repo_with_id};

    #[test]
    fn test_get_document_stubs_for_repo() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Doc 1");
        let doc2 = test_doc_with_repo("def456", "repo1", "Doc 2");
        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let stubs = db.get_document_stubs_for_repo("repo1").expect("Failed to get stubs");
        assert_eq!(stubs.len(), 2);
        assert!(stubs.contains_key("abc123"));
        assert!(stubs.contains_key("def456"));
        assert_eq!(stubs["abc123"].title, "Doc 1");
        assert!(!stubs["abc123"].is_deleted);
    }

    #[test]
    fn test_get_document_stubs_excludes_deleted() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Active");
        let doc2 = test_doc_with_repo("def456", "repo1", "Deleted");
        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");
        db.mark_deleted("def456").expect("Failed to mark deleted");

        let stubs = db.get_document_stubs_for_repo("repo1").expect("Failed to get stubs");
        assert_eq!(stubs.len(), 1);
        assert!(stubs.contains_key("abc123"));
        assert!(!stubs.contains_key("def456"));
    }

    #[test]
    fn test_get_document_stubs_repo_isolation() {
        let (db, _temp) = test_db();
        let repo1 = test_repo_with_id("repo1");
        let repo2 = test_repo_with_id("repo2");
        db.upsert_repository(&repo1).expect("Failed to create repo1");
        db.upsert_repository(&repo2).expect("Failed to create repo2");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Doc 1");
        let doc2 = test_doc_with_repo("def456", "repo2", "Doc 2");
        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let stubs1 = db.get_document_stubs_for_repo("repo1").expect("Failed to get stubs");
        let stubs2 = db.get_document_stubs_for_repo("repo2").expect("Failed to get stubs");
        assert_eq!(stubs1.len(), 1);
        assert_eq!(stubs2.len(), 1);
        assert!(stubs1.contains_key("abc123"));
        assert!(stubs2.contains_key("def456"));
    }

    #[test]
    fn test_get_documents_for_repo() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Doc 1");
        let doc2 = test_doc_with_repo("def456", "repo1", "Doc 2");

        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let docs = db.get_documents_for_repo("repo1").expect("Failed to get");
        assert_eq!(docs.len(), 2);
        assert!(docs.contains_key("abc123"));
        assert!(docs.contains_key("def456"));
    }

    #[test]
    fn test_get_documents_for_repo_isolation() {
        let (db, _temp) = test_db();
        let repo1 = test_repo_with_id("repo1");
        let repo2 = test_repo_with_id("repo2");
        db.upsert_repository(&repo1)
            .expect("Failed to create repo1");
        db.upsert_repository(&repo2)
            .expect("Failed to create repo2");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Doc 1");
        let doc2 = test_doc_with_repo("def456", "repo2", "Doc 2");

        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let repo1_docs = db.get_documents_for_repo("repo1").expect("Failed to get");
        let repo2_docs = db.get_documents_for_repo("repo2").expect("Failed to get");

        assert_eq!(repo1_docs.len(), 1);
        assert_eq!(repo2_docs.len(), 1);
        assert!(repo1_docs.contains_key("abc123"));
        assert!(repo2_docs.contains_key("def456"));
    }

    #[test]
    fn test_list_documents_with_repo_filter() {
        let (db, _temp) = test_db();
        let repo1 = test_repo_with_id("repo1");
        let repo2 = test_repo_with_id("repo2");
        db.upsert_repository(&repo1)
            .expect("Failed to create repo1");
        db.upsert_repository(&repo2)
            .expect("Failed to create repo2");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Doc 1");
        let doc2 = test_doc_with_repo("def456", "repo2", "Doc 2");

        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let docs = db
            .list_documents(None, Some("repo1"), None, 100)
            .expect("Failed to list");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, "abc123");
    }

    #[test]
    fn test_list_documents_with_title_filter() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Alpha Doc");
        let doc2 = test_doc_with_repo("def456", "repo1", "Beta Doc");

        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");

        let docs = db
            .list_documents(None, None, Some("%Alpha%"), 100)
            .expect("Failed to list");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Alpha Doc");
    }

    #[test]
    fn test_get_documents_for_repo_excludes_deleted() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Active Doc");
        let doc2 = test_doc_with_repo("def456", "repo1", "Deleted Doc");

        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");
        db.mark_deleted("def456").expect("Failed to mark deleted");

        let docs = db.get_documents_for_repo("repo1").expect("Failed to get");
        assert_eq!(docs.len(), 1, "deleted doc should be excluded");
        assert!(docs.contains_key("abc123"));
        assert!(!docs.contains_key("def456"));
    }
}
