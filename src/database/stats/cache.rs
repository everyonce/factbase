//! Stats caching infrastructure.

use super::super::{CachedStats, Database};
use crate::models::{DetailedStats, RepoStats, SourceStats, TemporalStats};

impl Database {
    /// Invalidate cached stats for a repository
    pub fn invalidate_stats_cache(&self, repo_id: &str) {
        if let Ok(mut cache) = self.stats_cache.write() {
            cache.remove(repo_id);
        }
    }

    pub(crate) fn get_cached_stats(&self, repo_id: &str) -> Option<CachedStats> {
        self.stats_cache.read().ok()?.get(repo_id).cloned()
    }

    pub(crate) fn cache_stats(&self, repo_id: &str, stats: RepoStats, detailed: DetailedStats) {
        if let Ok(mut cache) = self.stats_cache.write() {
            cache.insert(
                repo_id.to_string(),
                CachedStats {
                    stats,
                    detailed,
                    temporal: None,
                    source: None,
                },
            );
        }
    }

    pub(crate) fn cache_temporal_stats(&self, repo_id: &str, temporal: TemporalStats) {
        if let Ok(mut cache) = self.stats_cache.write() {
            if let Some(cached) = cache.get_mut(repo_id) {
                cached.temporal = Some(temporal);
            }
        }
    }

    pub(crate) fn cache_source_stats(&self, repo_id: &str, source: SourceStats) {
        if let Ok(mut cache) = self.stats_cache.write() {
            if let Some(cached) = cache.get_mut(repo_id) {
                cached.source = Some(source);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_stats_cache_invalidation() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1");

        let stats1 = db.get_stats("test-repo", None).expect("get_stats first");
        assert_eq!(stats1.active, 1);

        let stats2 = db.get_stats("test-repo", None).expect("get_stats second");
        assert_eq!(stats2.active, 1);

        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2");

        let stats3 = db.get_stats("test-repo", None).expect("get_stats third");
        assert_eq!(stats3.active, 2);

        db.mark_deleted("doc1").expect("mark_deleted");
        let stats4 = db.get_stats("test-repo", None).expect("get_stats fourth");
        assert_eq!(stats4.active, 1);
    }
}
