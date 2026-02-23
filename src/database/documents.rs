//! Document CRUD operations.
//!
//! This module handles:
//! - Document insertion and updates (upsert_document)
//! - Document retrieval (get_document, get_document_by_path)
//! - Document listing (get_documents_for_repo, list_documents)
//! - Document deletion (mark_deleted, hard_delete)
//! - Content hash management (update_document_hash, needs_update)
//!
//! # Content Compression
//!
//! When the `compression` feature is enabled, document content
//! is compressed with zstd before storage and decompressed on retrieval.

use std::collections::HashMap;

use crate::error::FactbaseError;
use crate::models::Document;
use base64::Engine;

use super::{compress_content, decode_content, doc_not_found, Database};

/// Column list for SELECT queries that map to `row_to_document()`.
const DOCUMENT_COLUMNS: &str =
    "id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted";

/// Look up the repo_id for a document (for cache invalidation).
fn repo_id_for_doc(conn: &super::DbConn, id: &str) -> Option<String> {
    conn.query_row("SELECT repo_id FROM documents WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .ok()
}

impl Database {
    /// Inserts or updates a document in the database.
    ///
    /// If a document with the same ID exists, it is replaced.
    /// Content is compressed if the `compression` feature is enabled.
    /// Word count is calculated and stored for efficient stats queries.
    /// Invalidates the stats cache for the document's repository.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn upsert_document(&self, doc: &Document) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let content_to_store = if self.compression {
            let compressed = compress_content(&doc.content);
            super::B64.encode(&compressed)
        } else {
            doc.content.clone()
        };
        // Calculate word count for efficient stats queries
        let word_count = crate::models::word_count(&doc.content) as i64;
        conn.execute(
            "INSERT OR REPLACE INTO documents (id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, FALSE, ?10)",
            rusqlite::params![
                doc.id, doc.repo_id, doc.file_path, doc.file_hash, doc.title, doc.doc_type, content_to_store,
                doc.file_modified_at.map(|t| t.to_rfc3339()), doc.indexed_at.to_rfc3339(), word_count
            ],
        )?;
        self.invalidate_stats_cache(&doc.repo_id);
        Ok(())
    }

    /// Update only the file hash for a document (used by scan --verify --fix)
    pub fn update_document_hash(&self, id: &str, new_hash: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute(
            "UPDATE documents SET file_hash = ?1 WHERE id = ?2 AND is_deleted = FALSE",
            rusqlite::params![new_hash, id],
        )?;
        if rows_affected == 0 {
            return Err(doc_not_found(id));
        }
        Ok(())
    }

    /// Update only the document type (used by organize retype)
    pub fn update_document_type(&self, id: &str, new_type: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let repo_id = repo_id_for_doc(&conn, id);
        let rows_affected = conn.execute(
            "UPDATE documents SET doc_type = ?1 WHERE id = ?2 AND is_deleted = FALSE",
            rusqlite::params![new_type, id],
        )?;
        if rows_affected == 0 {
            return Err(doc_not_found(id));
        }
        if let Some(repo) = repo_id {
            self.invalidate_stats_cache(&repo);
        }
        Ok(())
    }

    /// Check if document needs update by comparing hash.
    /// Uses prepared statement caching for performance on repeated calls.
    pub fn needs_update(&self, id: &str, hash: &str) -> Result<bool, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT file_hash FROM documents WHERE id = ?1 AND is_deleted = FALSE",
        )?;
        let existing: Option<String> = stmt.query_row([id], |row| row.get(0)).ok();
        Ok(existing.as_deref() != Some(hash))
    }

    /// Retrieves a document by its unique 6-character hex ID.
    ///
    /// Only returns non-deleted documents.
    /// Uses prepared statement caching for performance on repeated calls.
    ///
    /// # Returns
    /// `Ok(Some(doc))` if found, `Ok(None)` if not found or deleted.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_document(&self, id: &str) -> Result<Option<Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE id = ?1 AND is_deleted = FALSE"
        ))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves a document by ID, returning an error if not found.
    ///
    /// Convenience wrapper around [`get_document`](Self::get_document) that converts
    /// `None` into a [`FactbaseError::NotFound`] error.
    ///
    /// # Errors
    /// Returns `FactbaseError::NotFound` if the document doesn't exist,
    /// or `FactbaseError::Database` on SQL errors.
    pub fn require_document(&self, id: &str) -> Result<Document, FactbaseError> {
        self.get_document(id)?.ok_or_else(|| doc_not_found(id))
    }

    /// Retrieves a document by its file path within a repository.
    ///
    /// Includes both active and deleted documents (for move detection).
    /// Uses prepared statement caching for performance on repeated calls.
    ///
    /// # Returns
    /// `Ok(Some(doc))` if found, `Ok(None)` if not found.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_document_by_path(
        &self,
        repo_id: &str,
        path: &str,
    ) -> Result<Option<Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE repo_id = ?1 AND file_path = ?2"
        ))?;
        let mut rows = stmt.query([repo_id, path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves all documents for a repository as a map keyed by document ID.
    ///
    /// Includes both active and deleted documents (for scan reconciliation).
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_documents_for_repo(
        &self,
        repo_id: &str,
    ) -> Result<HashMap<String, Document>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE repo_id = ?1"
        ))?;
        let mut docs = HashMap::new();
        let mut rows = stmt.query([repo_id])?;
        while let Some(row) = rows.next()? {
            let doc = Self::row_to_document(row)?;
            docs.insert(doc.id.clone(), doc);
        }
        Ok(docs)
    }

    /// Converts a database row to a Document struct.
    ///
    /// Handles content decompression automatically.
    pub(crate) fn row_to_document(row: &rusqlite::Row) -> Result<Document, FactbaseError> {
        let file_modified_str: Option<String> = row.get(7)?;
        let indexed_str: String = row.get(8)?;
        let stored_content: String = row.get(6)?;

        // Auto-detect and decompress content
        let content = decode_content(&stored_content)?;

        Ok(Document {
            id: row.get(0)?,
            repo_id: row.get(1)?,
            file_path: row.get(2)?,
            file_hash: row.get(3)?,
            title: row.get(4)?,
            doc_type: row.get(5)?,
            content,
            file_modified_at: file_modified_str.and_then(|s| super::parse_rfc3339_utc_opt(&s)),
            indexed_at: super::parse_rfc3339_utc(&indexed_str),
            is_deleted: row.get(9)?,
        })
    }

    /// Marks a document as deleted (soft delete).
    ///
    /// The document remains in the database but is excluded from queries.
    /// Use `hard_delete_document` to permanently remove.
    /// Invalidates the stats cache for the document's repository.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn mark_deleted(&self, id: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let repo_id = repo_id_for_doc(&conn, id);
        conn.execute("UPDATE documents SET is_deleted = TRUE WHERE id = ?1", [id])?;
        if let Some(rid) = repo_id {
            self.invalidate_stats_cache(&rid);
        }
        Ok(())
    }

    /// Permanently delete a document from the database (hard delete)
    pub fn hard_delete_document(&self, id: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let repo_id = repo_id_for_doc(&conn, id);
        // Delete embedding first (foreign key constraint)
        conn.execute(
            "DELETE FROM document_embeddings WHERE id LIKE ?1 || '%'",
            [id],
        )?;
        conn.execute("DELETE FROM embedding_chunks WHERE document_id = ?1", [id])?;
        conn.execute(
            "DELETE FROM document_links WHERE source_id = ?1 OR target_id = ?1",
            [id],
        )?;
        conn.execute("DELETE FROM documents WHERE id = ?1", [id])?;
        if let Some(rid) = repo_id {
            self.invalidate_stats_cache(&rid);
        }
        Ok(())
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
            sql.push_str(&format!(" AND doc_type = ?{}", param_idx));
            param_idx += 1;
        }
        if repo_id.is_some() {
            sql.push_str(&format!(" AND repo_id = ?{}", param_idx));
            param_idx += 1;
        }
        if title_filter.is_some() {
            sql.push_str(&format!(" AND title LIKE ?{}", param_idx));
        }

        sql.push_str(&format!(" ORDER BY title LIMIT {}", limit));

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

    /// Check if a document needs cross-validation by comparing its current
    /// file_hash against the stored cross_check_hash.
    ///
    /// Returns `true` if the document has no cross_check_hash or it differs
    /// from the current file_hash (meaning content changed since last check).
    pub fn needs_cross_check(&self, id: &str) -> Result<bool, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT file_hash, cross_check_hash FROM documents WHERE id = ?1 AND is_deleted = FALSE",
        )?;
        let result: Option<(String, Option<String>)> = stmt
            .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok();
        match result {
            Some((file_hash, Some(cc_hash))) => Ok(file_hash != cc_hash),
            _ => Ok(true), // No document or no cross_check_hash → needs check
        }
    }

    /// Store the current file_hash as cross_check_hash after successful cross-validation.
    pub fn set_cross_check_hash(&self, id: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "UPDATE documents SET cross_check_hash = file_hash WHERE id = ?1 AND is_deleted = FALSE",
            [id],
        )?;
        Ok(())
    }

    /// Clear cross_check_hash for a list of document IDs.
    ///
    /// Used when a document changes to invalidate cross-check status
    /// of documents that link to it.
    pub fn clear_cross_check_hashes(&self, ids: &[&str]) -> Result<(), FactbaseError> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = self.get_conn()?;
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "UPDATE documents SET cross_check_hash = NULL WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        conn.execute(&sql, params.as_slice())?;
        Ok(())
    }

    /// Backfill word_count for documents with NULL values.
    ///
    /// Returns the number of documents updated.
    /// Used by `factbase db backfill-word-counts` command.
    pub fn backfill_word_counts(&self) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;

        // Find documents with NULL word_count
        let mut stmt = conn.prepare_cached(
            "SELECT id, content FROM documents WHERE word_count IS NULL AND is_deleted = FALSE",
        )?;
        let mut rows = stmt.query([])?;

        let mut updates: Vec<(String, i64)> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let stored_content: String = row.get(1)?;
            let content = decode_content(&stored_content)?;
            let word_count = crate::models::word_count(&content) as i64;
            updates.push((id, word_count));
        }
        drop(rows);
        drop(stmt);

        // Update each document
        let mut update_stmt =
            conn.prepare_cached("UPDATE documents SET word_count = ?1 WHERE id = ?2")?;
        for (id, word_count) in &updates {
            update_stmt.execute(rusqlite::params![word_count, id])?;
        }

        Ok(updates.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc_with_repo, test_repo_with_id};

    #[test]
    fn test_upsert_and_get_document() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("Failed to upsert");

        let retrieved = db.get_document("abc123").expect("Failed to get");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "abc123");
        assert_eq!(retrieved.title, "Test Doc");
    }

    #[test]
    fn test_get_document_not_found() {
        let (db, _temp) = test_db();
        let result = db.get_document("nonexistent").expect("Failed to query");
        assert!(result.is_none());
    }

    #[test]
    fn test_mark_deleted() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("Failed to upsert");
        db.mark_deleted("abc123").expect("Failed to mark deleted");

        // Should not be found via get_document (excludes deleted)
        let result = db.get_document("abc123").expect("Failed to query");
        assert!(result.is_none());
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
    fn test_needs_update() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("Failed to upsert");

        // Same hash - no update needed
        assert!(!db
            .needs_update("abc123", "abc123")
            .expect("Failed to check"));

        // Different hash - update needed
        assert!(db
            .needs_update("abc123", "different")
            .expect("Failed to check"));
    }

    #[test]
    fn test_hard_delete_document() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("Failed to upsert");
        db.hard_delete_document("abc123")
            .expect("Failed to hard delete");

        // Should not be found even via get_document_by_path
        let result = db
            .get_document_by_path("repo1", "abc123.md")
            .expect("Failed to query");
        assert!(result.is_none());
    }

    #[test]
    fn test_update_document_hash() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("Failed to upsert");
        db.update_document_hash("abc123", "newhash")
            .expect("Failed to update hash");

        // Verify hash was updated
        assert!(!db
            .needs_update("abc123", "newhash")
            .expect("Failed to check"));
    }

    #[test]
    fn test_word_count_populated() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        // Create doc with known word count
        let mut doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        doc.content = "one two three four five".to_string(); // 5 words
        db.upsert_document(&doc).expect("Failed to upsert");

        // Query word_count directly from database
        let conn = db.get_conn().expect("get connection");
        let word_count: i64 = conn
            .query_row(
                "SELECT word_count FROM documents WHERE id = ?1",
                ["abc123"],
                |row| row.get(0),
            )
            .expect("query word_count");

        assert_eq!(word_count, 5);
    }

    #[test]
    fn test_backfill_word_counts() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        // Insert doc with word_count via upsert
        let mut doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        doc.content = "one two three".to_string();
        db.upsert_document(&doc).expect("Failed to upsert");

        // Manually set word_count to NULL to simulate pre-migration data
        let conn = db.get_conn().expect("get connection");
        conn.execute(
            "UPDATE documents SET word_count = NULL WHERE id = ?1",
            ["abc123"],
        )
        .expect("set NULL");

        // Verify it's NULL
        let wc: Option<i64> = conn
            .query_row(
                "SELECT word_count FROM documents WHERE id = ?1",
                ["abc123"],
                |row| row.get(0),
            )
            .expect("query");
        assert!(wc.is_none());

        // Run backfill
        let updated = db.backfill_word_counts().expect("backfill");
        assert_eq!(updated, 1);

        // Verify word_count is now populated
        let wc: i64 = conn
            .query_row(
                "SELECT word_count FROM documents WHERE id = ?1",
                ["abc123"],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(wc, 3);
    }

    #[test]
    fn test_backfill_word_counts_skips_populated() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        // Insert doc with word_count via upsert (already populated)
        let mut doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        doc.content = "one two three".to_string();
        db.upsert_document(&doc).expect("Failed to upsert");

        // Run backfill - should update 0 since word_count already set
        let updated = db.backfill_word_counts().expect("backfill");
        assert_eq!(updated, 0);
    }

    #[test]
    fn test_needs_cross_check_no_hash() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");
        let doc = test_doc_with_repo("abc123", "repo1", "Test");
        db.upsert_document(&doc).expect("upsert");

        // No cross_check_hash set → needs check
        assert!(db.needs_cross_check("abc123").expect("check"));
    }

    #[test]
    fn test_needs_cross_check_after_set() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");
        let doc = test_doc_with_repo("abc123", "repo1", "Test");
        db.upsert_document(&doc).expect("upsert");

        db.set_cross_check_hash("abc123").expect("set hash");
        // Hash matches → no check needed
        assert!(!db.needs_cross_check("abc123").expect("check"));
    }

    #[test]
    fn test_needs_cross_check_after_content_change() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");
        let mut doc = test_doc_with_repo("abc123", "repo1", "Test");
        db.upsert_document(&doc).expect("upsert");
        db.set_cross_check_hash("abc123").expect("set hash");

        // Simulate content change by updating file_hash
        doc.file_hash = "newhash".to_string();
        db.upsert_document(&doc).expect("upsert changed");

        // Hash differs → needs check
        assert!(db.needs_cross_check("abc123").expect("check"));
    }

    #[test]
    fn test_clear_cross_check_hashes() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");
        let doc1 = test_doc_with_repo("aaa111", "repo1", "Doc1");
        let doc2 = test_doc_with_repo("bbb222", "repo1", "Doc2");
        db.upsert_document(&doc1).expect("upsert");
        db.upsert_document(&doc2).expect("upsert");
        db.set_cross_check_hash("aaa111").expect("set");
        db.set_cross_check_hash("bbb222").expect("set");

        assert!(!db.needs_cross_check("aaa111").expect("check"));
        assert!(!db.needs_cross_check("bbb222").expect("check"));

        db.clear_cross_check_hashes(&["aaa111", "bbb222"])
            .expect("clear");

        assert!(db.needs_cross_check("aaa111").expect("check"));
        assert!(db.needs_cross_check("bbb222").expect("check"));
    }

    #[test]
    fn test_clear_cross_check_hashes_empty() {
        let (db, _temp) = test_db();
        // Should not error on empty list
        db.clear_cross_check_hashes(&[]).expect("clear empty");
    }

    #[test]
    fn test_linked_doc_invalidation_on_change() {
        // Simulates Task 5.3: when doc changes, linked docs need re-cross-checking
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");

        let changed = test_doc_with_repo("changed1", "repo1", "Changed Doc");
        let linker = test_doc_with_repo("linker1", "repo1", "Linker Doc");
        db.upsert_document(&changed).expect("upsert");
        db.upsert_document(&linker).expect("upsert");

        // linker1 links TO changed1
        use crate::llm::DetectedLink;
        db.update_links(
            "linker1",
            &[DetectedLink {
                target_id: "changed1".to_string(),
                target_title: "Changed Doc".to_string(),
                mention_text: "Changed Doc".to_string(),
                context: "mentions".to_string(),
            }],
        )
        .expect("link");

        // Both have been cross-checked
        db.set_cross_check_hash("changed1").expect("set");
        db.set_cross_check_hash("linker1").expect("set");
        assert!(!db.needs_cross_check("linker1").expect("check"));

        // Simulate scan invalidation: changed1 changed, find docs linking to it
        let links = db.get_links_to("changed1").expect("links");
        let ids: Vec<&str> = links.iter().map(|l| l.source_id.as_str()).collect();
        db.clear_cross_check_hashes(&ids).expect("clear");

        // linker1 now needs re-cross-checking
        assert!(db.needs_cross_check("linker1").expect("check"));
    }
}
