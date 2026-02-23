//! Detailed statistics computation.
//!
//! Provides `compute_detailed_stats` for extended repository statistics with
//! optional `since` filter for modification-date filtering.

use super::{decode_content_lossy, Database};
use crate::error::FactbaseError;
use crate::models::DetailedStats;
use chrono::{DateTime, Utc};

impl Database {
    /// Compute detailed statistics for a repository.
    ///
    /// When `since` is provided, only documents modified on or after that date are included.
    /// Uses `prepare_cached()` for query reuse across repeated calls.
    pub(crate) fn compute_detailed_stats(
        &self,
        repo_id: &str,
        since: Option<&DateTime<Utc>>,
    ) -> Result<DetailedStats, FactbaseError> {
        let conn = self.get_conn()?;
        let since_str = since.map(|s| s.to_rfc3339());
        let since_clause = if since.is_some() {
            " AND d.file_modified_at >= ?2"
        } else {
            ""
        };
        let since_clause_no_alias = if since.is_some() {
            " AND file_modified_at >= ?2"
        } else {
            ""
        };

        let params: Vec<&dyn rusqlite::types::ToSql> = if let Some(ref s) = since_str {
            vec![&repo_id, s]
        } else {
            vec![&repo_id]
        };

        // Most linked documents (by incoming links)
        let sql = format!(
            "SELECT d.id, d.title, COUNT(l.source_id) as cnt
             FROM documents d
             LEFT JOIN document_links l ON d.id = l.target_id
             WHERE d.repo_id = ?1 AND d.is_deleted = FALSE{since_clause}
             GROUP BY d.id
             ORDER BY cnt DESC
             LIMIT 5"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(&*params)?;
        let mut most_linked = Vec::with_capacity(5);
        while let Some(row) = rows.next()? {
            let cnt: usize = row.get(2)?;
            if cnt > 0 {
                most_linked.push((row.get(0)?, row.get(1)?, cnt));
            }
        }

        // Orphan documents (no incoming or outgoing links)
        let sql = format!(
            "SELECT d.id, d.title
             FROM documents d
             WHERE d.repo_id = ?1 AND d.is_deleted = FALSE{since_clause}
               AND NOT EXISTS (SELECT 1 FROM document_links WHERE source_id = d.id)
               AND NOT EXISTS (SELECT 1 FROM document_links WHERE target_id = d.id)"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(&*params)?;
        let mut orphans = Vec::with_capacity(16);
        while let Some(row) = rows.next()? {
            orphans.push((row.get(0)?, row.get(1)?));
        }

        // Average document size
        let sql = format!(
            "SELECT COALESCE(AVG(LENGTH(content)), 0) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause_no_alias}"
        );
        let avg_doc_size: usize = conn
            .prepare_cached(&sql)?
            .query_row(&*params, |r| r.get(0))
            .unwrap_or(0);

        // Total words and average words per document
        let (total_words, avg_words_per_doc) =
            self.compute_word_stats(&conn, &params, since_clause_no_alias)?;

        // Compression stats (only for unfiltered view)
        let compression_stats = if since.is_none() && self.compression {
            self.compute_compression_stats(&conn, repo_id)?
        } else {
            None
        };

        // Oldest and newest documents
        let oldest_doc = self.query_boundary_doc(&conn, &params, since_clause_no_alias, "ASC")?;
        let newest_doc = self.query_boundary_doc(&conn, &params, since_clause_no_alias, "DESC")?;

        Ok(DetailedStats {
            most_linked,
            orphans,
            avg_doc_size,
            total_words,
            avg_words_per_doc,
            compression_stats,
            oldest_doc,
            newest_doc,
            temporal_stats: None,
            source_stats: None,
        })
    }

    /// Compute total and average word counts.
    fn compute_word_stats(
        &self,
        conn: &rusqlite::Connection,
        params: &[&dyn rusqlite::types::ToSql],
        since_clause: &str,
    ) -> Result<(usize, usize), FactbaseError> {
        let sql = format!(
            "SELECT COALESCE(SUM(word_count), 0), COUNT(*), COALESCE(SUM(CASE WHEN word_count IS NULL THEN 1 ELSE 0 END), 0)
             FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause}"
        );
        let (sum, count, null_count): (i64, i64, i64) = conn
            .prepare_cached(&sql)?
            .query_row(params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;

        if null_count == 0 {
            let total = sum as usize;
            let avg = if count > 0 { total / count as usize } else { 0 };
            return Ok((total, avg));
        }

        // Fallback: some documents missing word_count
        let content_sql = if since_clause.is_empty() {
            super::CONTENT_ONLY_QUERY.to_string()
        } else {
            format!(
                "SELECT content FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause}"
            )
        };
        let mut stmt = conn.prepare_cached(&content_sql)?;
        let mut rows = stmt.query(params)?;
        let mut total = 0usize;
        let mut doc_count = 0usize;
        while let Some(row) = rows.next()? {
            let content: String = row.get(0)?;
            let decoded = decode_content_lossy(content);
            total += crate::models::word_count(&decoded);
            doc_count += 1;
        }
        let avg = if doc_count > 0 { total / doc_count } else { 0 };
        Ok((total, avg))
    }

    /// Query oldest or newest document by modification date.
    fn query_boundary_doc(
        &self,
        conn: &rusqlite::Connection,
        params: &[&dyn rusqlite::types::ToSql],
        since_clause: &str,
        order: &str,
    ) -> Result<Option<(String, String, DateTime<Utc>)>, FactbaseError> {
        let sql = format!(
            "SELECT id, title, COALESCE(file_modified_at, indexed_at) as mod_date
             FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause}
             ORDER BY mod_date {order} LIMIT 1"
        );
        Ok(conn
            .prepare_cached(&sql)?
            .query_row(params, |row| {
                let date_str: String = row.get(2)?;
                let date = crate::database::parse_rfc3339_utc(&date_str);
                Ok((row.get(0)?, row.get(1)?, date))
            })
            .ok())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_compute_detailed_stats() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2");
        db.upsert_document(&test_doc("doc3", "Doc 3"))
            .expect("upsert doc3");
        db.upsert_document(&test_doc("doc4", "Doc 4"))
            .expect("upsert doc4");

        db.update_links(
            "doc2",
            &[crate::llm::DetectedLink {
                target_id: "doc1".into(),
                target_title: "Doc 1".into(),
                mention_text: "Doc 1".into(),
                context: "".into(),
            }],
        )
        .expect("update_links doc2");
        db.update_links(
            "doc3",
            &[crate::llm::DetectedLink {
                target_id: "doc1".into(),
                target_title: "Doc 1".into(),
                mention_text: "Doc 1".into(),
                context: "".into(),
            }],
        )
        .expect("update_links doc3");

        let detailed = db
            .compute_detailed_stats("test-repo", None)
            .expect("compute_detailed_stats");
        assert_eq!(detailed.most_linked.len(), 1);
        assert_eq!(detailed.most_linked[0].0, "doc1");
        assert_eq!(detailed.most_linked[0].2, 2);
        assert_eq!(detailed.orphans.len(), 1);
        assert_eq!(detailed.orphans[0].0, "doc4");
    }

    #[test]
    fn test_compute_detailed_stats_since_filters_by_date() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc1 = test_doc("abc123", "Old Doc");
        doc1.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::days(10));
        db.upsert_document(&doc1).expect("upsert doc1");

        let mut doc2 = test_doc("def456", "New Doc");
        doc2.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        db.upsert_document(&doc2).expect("upsert doc2");

        let since = chrono::Utc::now() - chrono::Duration::days(1);
        let filtered_stats = db
            .compute_detailed_stats(&repo.id, Some(&since))
            .expect("compute_detailed_stats with since");

        assert_eq!(filtered_stats.orphans.len(), 1);
        assert_eq!(filtered_stats.orphans[0].0, "def456");
    }

    #[test]
    fn test_compute_detailed_stats_uses_word_count_column() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        let mut doc1 = test_doc("doc1", "Doc 1");
        doc1.content = "Test content".to_string();
        db.upsert_document(&doc1).expect("upsert doc1");

        let mut doc2 = test_doc("doc2", "Doc 2");
        doc2.content = "More test content here".to_string();
        db.upsert_document(&doc2).expect("upsert doc2");

        let detailed = db
            .compute_detailed_stats("test-repo", None)
            .expect("compute_detailed_stats");

        assert_eq!(detailed.total_words, 6);
        assert_eq!(detailed.avg_words_per_doc, 3);
    }
}
