//! Batch operations: cross-check hashes, backfill word counts.

use crate::error::FactbaseError;

use super::super::{decode_content, Database};

impl Database {
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
        use crate::link_detection::DetectedLink;
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
