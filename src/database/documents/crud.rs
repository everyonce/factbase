//! Core CRUD operations: upsert, get, update, delete.

use crate::error::FactbaseError;
use crate::models::Document;
use base64::Engine;

use super::super::{compress_content, doc_not_found, Database};
use super::{repo_id_for_doc, DOCUMENT_COLUMNS};

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
        let compressed;
        let content_to_store: &str = if self.compression {
            compressed = super::super::B64.encode(compress_content(&doc.content));
            &compressed
        } else {
            &doc.content
        };
        // Calculate word count for efficient stats queries
        let word_count = crate::models::word_count(&doc.content) as i64;
        let has_review_queue = crate::patterns::has_review_section(&doc.content);

        // Remove any stale document at the same path with a different ID.
        // This handles the case where a file's factbase ID was regenerated.
        conn.execute(
            "DELETE FROM document_links WHERE source_id IN (SELECT id FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3)
             OR target_id IN (SELECT id FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3)",
            rusqlite::params![doc.repo_id, doc.file_path, doc.id],
        )?;
        // Clean up embeddings: get chunk IDs first, then delete from vec0 and chunks table
        let stale_chunk_ids: Vec<String> = conn
            .prepare_cached("SELECT ec.id FROM embedding_chunks ec JOIN documents d ON ec.document_id = d.id WHERE d.repo_id = ?1 AND d.file_path = ?2 AND d.id != ?3")?
            .query_map(rusqlite::params![doc.repo_id, doc.file_path, doc.id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();
        for chunk_id in &stale_chunk_ids {
            let _ = conn.execute("DELETE FROM document_embeddings WHERE id = ?1", [chunk_id]);
        }
        conn.execute(
            "DELETE FROM embedding_chunks WHERE document_id IN (SELECT id FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3)",
            rusqlite::params![doc.repo_id, doc.file_path, doc.id],
        )?;
        conn.execute(
            "DELETE FROM document_content_fts WHERE doc_id IN (SELECT id FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3)",
            rusqlite::params![doc.repo_id, doc.file_path, doc.id],
        )?;
        // Clean up fact-level embeddings and metadata (foreign key on document_id)
        let stale_fact_ids: Vec<String> = conn
            .prepare_cached("SELECT fm.id FROM fact_metadata fm JOIN documents d ON fm.document_id = d.id WHERE d.repo_id = ?1 AND d.file_path = ?2 AND d.id != ?3")?
            .query_map(rusqlite::params![doc.repo_id, doc.file_path, doc.id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();
        for fact_id in &stale_fact_ids {
            let _ = conn.execute("DELETE FROM fact_embeddings WHERE id = ?1", [fact_id]);
        }
        conn.execute(
            "DELETE FROM fact_metadata WHERE document_id IN (SELECT id FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3)",
            rusqlite::params![doc.repo_id, doc.file_path, doc.id],
        )?;
        conn.execute(
            "DELETE FROM documents WHERE repo_id = ?1 AND file_path = ?2 AND id != ?3",
            rusqlite::params![doc.repo_id, doc.file_path, doc.id],
        )?;

        conn.execute(
            "INSERT INTO documents (id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted, word_count, has_review_queue)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, FALSE, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                repo_id = excluded.repo_id,
                file_path = excluded.file_path,
                file_hash = excluded.file_hash,
                title = excluded.title,
                doc_type = excluded.doc_type,
                content = excluded.content,
                file_modified_at = excluded.file_modified_at,
                indexed_at = excluded.indexed_at,
                is_deleted = FALSE,
                word_count = excluded.word_count,
                has_review_queue = excluded.has_review_queue",
            rusqlite::params![
                doc.id, doc.repo_id, doc.file_path, doc.file_hash, doc.title, doc.doc_type, content_to_store,
                doc.file_modified_at.map(|t| t.to_rfc3339()), doc.indexed_at.to_rfc3339(), word_count, has_review_queue
            ],
        )?;
        // Keep FTS5 index in sync
        conn.execute(
            "DELETE FROM document_content_fts WHERE doc_id = ?1",
            [&doc.id],
        )?;
        conn.execute(
            "INSERT INTO document_content_fts (doc_id, content) VALUES (?1, ?2)",
            rusqlite::params![doc.id, doc.content],
        )?;
        self.invalidate_stats_cache(&doc.repo_id);
        Ok(())
    }

    /// Update document content and hash in the database (used after lint writes review questions).
    ///
    /// Runs all three SQL statements (UPDATE + FTS5 sync) inside a single
    /// transaction so a partial failure cannot leave the index inconsistent.
    pub fn update_document_content(
        &self,
        id: &str,
        content: &str,
        new_hash: &str,
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute_batch("BEGIN")?;
        let result = self.update_document_content_on_conn(&conn, id, content, new_hash);
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Update document content on an existing connection (no transaction management).
    ///
    /// Use this inside [`Database::with_transaction`] to batch multiple content
    /// updates atomically.
    pub fn update_document_content_on_conn(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
        content: &str,
        new_hash: &str,
    ) -> Result<(), FactbaseError> {
        let compressed;
        let content_to_store: &str = if self.compression {
            compressed = super::super::B64.encode(super::super::compress_content(content));
            &compressed
        } else {
            content
        };
        let word_count = crate::models::word_count(content) as i64;
        let has_review_queue = crate::patterns::has_review_section(content);
        conn.execute(
            "UPDATE documents SET content = ?1, file_hash = ?2, word_count = ?3, has_review_queue = ?4 WHERE id = ?5 AND is_deleted = FALSE",
            rusqlite::params![content_to_store, new_hash, word_count, has_review_queue, id],
        )?;
        // Keep FTS5 index in sync
        conn.execute("DELETE FROM document_content_fts WHERE doc_id = ?1", [id])?;
        conn.execute(
            "INSERT INTO document_content_fts (doc_id, content) VALUES (?1, ?2)",
            rusqlite::params![id, content],
        )?;
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

    /// Update only the document title.
    pub fn update_document_title(&self, id: &str, title: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "UPDATE documents SET title = ?1 WHERE id = ?2 AND is_deleted = FALSE",
            rusqlite::params![title, id],
        )?;
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
        conn.execute("DELETE FROM document_content_fts WHERE doc_id = ?1", [id])?;
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
        conn.execute("DELETE FROM document_content_fts WHERE doc_id = ?1", [id])?;
        // Clean up fact-level embeddings and metadata (foreign key on document_id)
        let fact_ids: Vec<String> = conn
            .prepare_cached("SELECT id FROM fact_metadata WHERE document_id = ?1")?
            .query_map([id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();
        for fact_id in &fact_ids {
            let _ = conn.execute("DELETE FROM fact_embeddings WHERE id = ?1", [fact_id]);
        }
        conn.execute("DELETE FROM fact_metadata WHERE document_id = ?1", [id])?;
        conn.execute("DELETE FROM documents WHERE id = ?1", [id])?;
        if let Some(rid) = repo_id {
            self.invalidate_stats_cache(&rid);
        }
        Ok(())
    }

    /// Purge all soft-deleted documents for a repository, removing them and their
    /// associated embeddings, links, fact metadata, and review questions.
    ///
    /// Call this after marking documents deleted during a scan to keep the DB lean.
    pub fn purge_deleted_documents(&self, repo_id: &str) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        // Collect IDs to purge
        let ids: Vec<String> = conn
            .prepare_cached("SELECT id FROM documents WHERE repo_id = ?1 AND is_deleted = TRUE")?
            .query_map([repo_id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        conn.execute_batch("BEGIN")?;
        // Delete embeddings via subquery through embedding_chunks (avoids LIKE per-row)
        conn.execute(
            &format!("DELETE FROM document_embeddings WHERE id IN (SELECT id FROM embedding_chunks WHERE document_id IN ({placeholders}))"),
            params.as_slice(),
        )?;
        conn.execute(
            &format!("DELETE FROM embedding_chunks WHERE document_id IN ({placeholders})"),
            params.as_slice(),
        )?;
        conn.execute(
            &format!("DELETE FROM document_links WHERE source_id IN ({placeholders}) OR target_id IN ({placeholders})"),
            // params used twice for source_id and target_id
            {
                let mut p: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                p.extend(ids.iter().map(|s| s as &dyn rusqlite::ToSql));
                p
            }.as_slice(),
        )?;
        conn.execute(
            &format!("DELETE FROM review_questions WHERE doc_id IN ({placeholders})"),
            params.as_slice(),
        )?;
        // fact_embeddings via subquery through fact_metadata
        conn.execute(
            &format!("DELETE FROM fact_embeddings WHERE id IN (SELECT id FROM fact_metadata WHERE document_id IN ({placeholders}))"),
            params.as_slice(),
        )?;
        conn.execute(
            &format!("DELETE FROM fact_metadata WHERE document_id IN ({placeholders})"),
            params.as_slice(),
        )?;
        conn.execute(
            &format!("DELETE FROM documents WHERE id IN ({placeholders})"),
            params.as_slice(),
        )?;
        conn.execute_batch("COMMIT")?;
        self.invalidate_stats_cache(repo_id);
        Ok(ids.len())
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

        // FTS5 entry should be removed
        let conn = db.pool.get().expect("Failed to get conn");
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_content_fts WHERE doc_id = ?1",
                ["abc123"],
                |row| row.get(0),
            )
            .expect("Failed to query FTS5");
        assert_eq!(fts_count, 0, "FTS5 entry should be removed on mark_deleted");
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

        // FTS5 entry should be removed
        let conn = db.pool.get().expect("Failed to get conn");
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_content_fts WHERE doc_id = ?1",
                ["abc123"],
                |row| row.get(0),
            )
            .expect("Failed to query FTS5");
        assert_eq!(
            fts_count, 0,
            "FTS5 entry should be removed on hard_delete_document"
        );
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
    fn test_upsert_cleans_fact_metadata_for_stale_doc() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");

        // Create a document at a specific path
        let mut doc = test_doc_with_repo("old_id", "repo1", "Test Doc");
        doc.file_path = "test/doc.md".to_string();
        db.upsert_document(&doc).expect("upsert old doc");

        // Add fact_metadata referencing the old document (id, doc_id, line, text, hash, embedding)
        db.upsert_fact_embedding("fact1", "old_id", 1, "some fact", "hash1", &[0.1; 1024])
            .expect("insert fact");

        // Now upsert a new document at the same path with a different ID
        let mut new_doc = test_doc_with_repo("new_id", "repo1", "Test Doc Updated");
        new_doc.file_path = "test/doc.md".to_string();
        // This should not fail with FK constraint
        db.upsert_document(&new_doc)
            .expect("upsert new doc at same path should clean up stale fact_metadata");

        // Old doc should be gone
        assert!(db.get_document("old_id").expect("query").is_none());
        // New doc should exist
        assert!(db.get_document("new_id").expect("query").is_some());
    }

    #[test]
    fn test_hard_delete_cleans_fact_metadata() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Test Doc");
        db.upsert_document(&doc).expect("upsert");

        // Add fact_metadata referencing the document (id, doc_id, line, text, hash, embedding)
        db.upsert_fact_embedding("fact1", "abc123", 1, "some fact", "hash1", &[0.1; 1024])
            .expect("insert fact");

        // Hard delete should not fail with FK constraint
        db.hard_delete_document("abc123")
            .expect("hard delete should clean up fact_metadata");
    }

    #[test]
    fn test_purge_deleted_documents() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc1 = test_doc_with_repo("abc123", "repo1", "Active");
        let doc2 = test_doc_with_repo("def456", "repo1", "Deleted");
        db.upsert_document(&doc1).expect("Failed to upsert");
        db.upsert_document(&doc2).expect("Failed to upsert");
        db.mark_deleted("def456").expect("Failed to mark deleted");

        let purged = db.purge_deleted_documents("repo1").expect("Failed to purge");
        assert_eq!(purged, 1);

        // Active doc still present
        assert!(db.get_document("abc123").unwrap().is_some());
        // Deleted doc gone
        assert!(db.get_document("def456").unwrap().is_none());
    }

    #[test]
    fn test_purge_deleted_documents_idempotent() {
        let (db, _temp) = test_db();
        let repo = test_repo_with_id("repo1");
        db.upsert_repository(&repo).expect("Failed to create repo");

        let doc = test_doc_with_repo("abc123", "repo1", "Active");
        db.upsert_document(&doc).expect("Failed to upsert");

        // No deleted docs — purge returns 0
        let purged = db.purge_deleted_documents("repo1").expect("Failed to purge");
        assert_eq!(purged, 0);
        assert!(db.get_document("abc123").unwrap().is_some());
    }
}
