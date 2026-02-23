//! Basic statistics computation.
//!
//! Provides `compute_stats` for basic repository statistics with optional
//! `since` filter for modification-date filtering.

use super::Database;
use crate::error::FactbaseError;
use crate::models::RepoStats;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

impl Database {
    /// Compute basic statistics for a repository.
    ///
    /// When `since` is provided, only documents modified on or after that date are counted.
    /// Uses `prepare_cached()` for query reuse across repeated calls.
    pub(crate) fn compute_stats(
        &self,
        repo_id: &str,
        since: Option<&DateTime<Utc>>,
    ) -> Result<RepoStats, FactbaseError> {
        let conn = self.get_conn()?;
        let since_str = since.map(DateTime::to_rfc3339);
        let since_clause = if since.is_some() {
            " AND file_modified_at >= ?2"
        } else {
            ""
        };

        let params: Vec<&dyn rusqlite::types::ToSql> = if let Some(ref s) = since_str {
            vec![&repo_id, s]
        } else {
            vec![&repo_id]
        };

        let (total, active, deleted) = if since.is_some() {
            let sql = format!(
                "SELECT COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause}"
            );
            let active: usize = conn
                .prepare_cached(&sql)?
                .query_row(&*params, |r| r.get(0))?;
            (active, active, 0)
        } else {
            let total: usize = conn
                .prepare_cached("SELECT COUNT(*) FROM documents WHERE repo_id = ?1")?
                .query_row([repo_id], |r| r.get(0))?;
            let active: usize = conn
                .prepare_cached(
                    "SELECT COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
                )?
                .query_row([repo_id], |r| r.get(0))?;
            (total, active, total - active)
        };

        let type_sql = format!(
            "SELECT doc_type, COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE{since_clause} GROUP BY doc_type"
        );
        let mut stmt = conn.prepare_cached(&type_sql)?;
        let by_type: HashMap<String, usize> = stmt
            .query_map(&*params, |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "unknown".into()),
                    row.get(1)?,
                ))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(RepoStats {
            total,
            active,
            deleted,
            by_type,
        })
    }
}

impl Database {
    /// Count deferred review questions across documents.
    ///
    /// A deferred question has an answer but is not marked as answered (checkbox unchecked).
    pub fn count_deferred_questions(&self, repo_id: Option<&str>) -> Result<usize, FactbaseError> {
        let docs = self.get_documents_with_review_queue(repo_id)?;
        Ok(docs
            .iter()
            .filter_map(|d| crate::processor::parse_review_queue(&d.content))
            .flatten()
            .filter(|q| q.is_deferred())
            .count())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_compute_stats() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2");
        db.mark_deleted("doc2").expect("mark_deleted");

        let stats = db.compute_stats("test-repo", None).expect("compute_stats");
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 1);
        assert_eq!(stats.deleted, 1);
    }

    #[test]
    fn test_compute_stats_since_filters_by_date() {
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
            .compute_stats(&repo.id, Some(&since))
            .expect("compute_stats with since");
        assert_eq!(filtered_stats.active, 1);
    }

    #[test]
    fn test_compute_stats_since_empty_when_no_matches() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Old Doc");
        doc.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::days(10));
        db.upsert_document(&doc).expect("upsert");

        let since = chrono::Utc::now() - chrono::Duration::hours(1);
        let filtered_stats = db
            .compute_stats(&repo.id, Some(&since))
            .expect("compute_stats with since");
        assert_eq!(filtered_stats.active, 0);
        assert!(filtered_stats.by_type.is_empty());
    }

    #[test]
    fn test_count_deferred_questions() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        // Doc with 1 deferred (unchecked + answer), 1 answered, 1 unanswered
        let mut doc = test_doc("doc1", "Doc 1");
        doc.content = "# Doc 1\n\nContent.\n\n<!-- factbase:review -->\n\
            - [ ] `@q[temporal]` When was this? \n\
            > deferred answer\n\
            - [x] `@q[conflict]` Is this right?\n\
            > yes\n\
            - [ ] `@q[missing]` Source needed?"
            .to_string();
        db.upsert_document(&doc).expect("upsert doc1");

        // Doc without review queue
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2");

        assert_eq!(db.count_deferred_questions(None).expect("count"), 1);
        assert_eq!(
            db.count_deferred_questions(Some(&repo.id)).expect("count"),
            1
        );
        assert_eq!(
            db.count_deferred_questions(Some("nonexistent"))
                .expect("count"),
            0
        );
    }
}
