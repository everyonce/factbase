//! Statistics and caching.
//!
//! This module handles:
//! - Basic statistics (`get_stats` with optional `since` filter)
//! - Detailed statistics (`get_detailed_stats` with optional `since` filter)
//! - Temporal statistics (`compute_temporal_stats`)
//! - Source statistics (`compute_source_stats`)
//! - Stats cache management (`invalidate_stats_cache`)
//!
//! # Caching
//!
//! Statistics are cached per-repository to avoid expensive recomputation.
//! The cache is invalidated when documents are added, updated, or deleted.
//!
//! # Submodules
//!
//! - `basic` - Basic statistics computation
//! - `detailed` - Detailed statistics computation
//! - `cache` - Caching infrastructure
//! - `temporal` - Temporal tag statistics
//! - `sources` - Source attribution statistics
//! - `compression` - Compression statistics

mod basic;
mod cache;
mod compression;
mod detailed;
mod sources;
mod temporal;

use super::{decode_content_lossy, Database, DbConn};
use crate::cache::DocumentMetadata;
use crate::error::FactbaseError;
use crate::models::{DetailedStats, PoolStats, RepoStats};
use crate::patterns::{date_cmp, normalize_date_for_comparison};
use chrono::{DateTime, Utc};

/// Update oldest/newest date tracking. Used by temporal and source stats.
pub(crate) fn update_date_range(
    date: &str,
    oldest: &mut Option<String>,
    newest: &mut Option<String>,
) {
    let normalized = normalize_date_for_comparison(date);
    match oldest {
        Some(ref old)
            if date_cmp(&normalized, &normalize_date_for_comparison(old))
                != std::cmp::Ordering::Less => {}
        _ => *oldest = Some(date.to_string()),
    }
    match newest {
        Some(ref new)
            if date_cmp(&normalized, &normalize_date_for_comparison(new))
                != std::cmp::Ordering::Greater => {}
        _ => *newest = Some(date.to_string()),
    }
}

// Shared query for fetching content-only from active documents in a repo.
pub(crate) const CONTENT_ONLY_QUERY: &str =
    "SELECT content FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE";

// Document content with pre-computed metadata, used by temporal and source stats.
pub(crate) struct DocContent {
    pub decoded: String,
    pub metadata: DocumentMetadata,
}

// Fetch all active documents for a repo with decoded content and cached metadata.
pub(crate) fn fetch_active_doc_content(
    conn: &DbConn,
    repo_id: &str,
) -> Result<Vec<DocContent>, FactbaseError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_hash, content FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
    )?;
    let mut rows = stmt.query([repo_id])?;
    let mut docs = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let file_hash: String = row.get(1)?;
        let content: String = row.get(2)?;
        let decoded = decode_content_lossy(content);
        let metadata = crate::cache::get_or_compute_metadata(&id, &file_hash, &decoded);
        docs.push(DocContent { decoded, metadata });
    }
    Ok(docs)
}

impl Database {
    /// Gets basic statistics for a repository.
    ///
    /// When `since` is `Some`, only documents modified on or after that date are counted
    /// and the cache is bypassed.
    pub fn get_stats(
        &self,
        repo_id: &str,
        since: Option<&DateTime<Utc>>,
    ) -> Result<RepoStats, FactbaseError> {
        if since.is_none() {
            if let Some(cached) = self.get_cached_stats(repo_id) {
                return Ok(cached.stats);
            }
        }
        let stats = self.compute_stats(repo_id, since)?;
        if since.is_none() {
            let detailed = self.compute_detailed_stats(repo_id, None)?;
            self.cache_stats(repo_id, stats.clone(), detailed);
        }
        Ok(stats)
    }

    /// Gets detailed statistics for a repository.
    ///
    /// When `since` is `Some`, only documents modified on or after that date are included
    /// and the cache is bypassed.
    pub fn get_detailed_stats(
        &self,
        repo_id: &str,
        since: Option<&DateTime<Utc>>,
    ) -> Result<DetailedStats, FactbaseError> {
        if since.is_none() {
            if let Some(cached) = self.get_cached_stats(repo_id) {
                return Ok(cached.detailed);
            }
        }
        let detailed = self.compute_detailed_stats(repo_id, since)?;
        if since.is_none() {
            let stats = self.compute_stats(repo_id, None)?;
            self.cache_stats(repo_id, stats, detailed.clone());
        }
        Ok(detailed)
    }

    /// Get connection pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        let state = self.pool.state();
        PoolStats {
            connections: state.connections,
            idle_connections: state.idle_connections,
            max_size: self.pool.max_size(),
        }
    }

    /// Runs VACUUM and ANALYZE to optimize the database.
    pub fn vacuum(&self) -> Result<(u64, u64), FactbaseError> {
        let conn = self.get_conn()?;
        let size_before: u64 = conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count, pragma_page_size",
            [],
            |r| r.get(0),
        )?;
        conn.execute("VACUUM", [])?;
        conn.execute("ANALYZE", [])?;
        let size_after: u64 = conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count, pragma_page_size",
            [],
            |r| r.get(0),
        )?;
        Ok((size_before, size_after))
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_get_stats() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2");
        db.mark_deleted("doc2").expect("mark_deleted");

        let stats = db.get_stats("test-repo", None).expect("get_stats");
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 1);
        assert_eq!(stats.deleted, 1);
    }

    #[test]
    fn test_get_detailed_stats() {
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
            &[crate::link_detection::DetectedLink {
                target_id: "doc1".into(),
                target_title: "Doc 1".into(),
                mention_text: "Doc 1".into(),
                context: "".into(),
            }],
        )
        .expect("update_links doc2");
        db.update_links(
            "doc3",
            &[crate::link_detection::DetectedLink {
                target_id: "doc1".into(),
                target_title: "Doc 1".into(),
                mention_text: "Doc 1".into(),
                context: "".into(),
            }],
        )
        .expect("update_links doc3");

        let detailed = db
            .get_detailed_stats("test-repo", None)
            .expect("get_detailed_stats");
        assert_eq!(detailed.most_linked.len(), 1);
        assert_eq!(detailed.most_linked[0].0, "doc1");
        assert_eq!(detailed.most_linked[0].2, 2);
        assert_eq!(detailed.orphans.len(), 1);
        assert_eq!(detailed.orphans[0].0, "doc4");
    }

    #[test]
    fn test_pool_stats_returns_valid_values() {
        let (db, _tmp) = test_db();
        let stats = db.pool_stats();
        assert!(stats.max_size >= 1);
        assert!(stats.connections <= stats.max_size);
        assert!(stats.idle_connections <= stats.max_size);
    }

    #[test]
    fn test_vacuum_returns_sizes() {
        let (db, _tmp) = test_db();
        let (before, after) = db.vacuum().expect("vacuum");
        assert!(before > 0);
        assert!(after > 0);
    }

    #[test]
    fn test_get_stats_since_filters_by_date() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc1 = test_doc("abc123", "Old Doc");
        doc1.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::days(10));
        db.upsert_document(&doc1).expect("upsert doc1");

        let mut doc2 = test_doc("def456", "New Doc");
        doc2.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        db.upsert_document(&doc2).expect("upsert doc2");

        let all_stats = db.get_stats(&repo.id, None).expect("get_stats");
        assert_eq!(all_stats.active, 2);

        let since = chrono::Utc::now() - chrono::Duration::days(1);
        let filtered_stats = db
            .get_stats(&repo.id, Some(&since))
            .expect("get_stats with since");
        assert_eq!(filtered_stats.active, 1);
    }

    #[test]
    fn test_get_detailed_stats_since_filters_by_date() {
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
            .get_detailed_stats(&repo.id, Some(&since))
            .expect("get_detailed_stats with since");

        assert_eq!(filtered_stats.orphans.len(), 1);
        assert_eq!(filtered_stats.orphans[0].0, "def456");
    }

    #[test]
    fn test_get_stats_since_empty_when_no_matches() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Old Doc");
        doc.file_modified_at = Some(chrono::Utc::now() - chrono::Duration::days(10));
        db.upsert_document(&doc).expect("upsert");

        let since = chrono::Utc::now() - chrono::Duration::hours(1);
        let filtered_stats = db
            .get_stats(&repo.id, Some(&since))
            .expect("get_stats with since");
        assert_eq!(filtered_stats.active, 0);
        assert!(filtered_stats.by_type.is_empty());
    }
}
