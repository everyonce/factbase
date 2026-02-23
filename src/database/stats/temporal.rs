//! Temporal statistics computation.

use super::super::Database;
use crate::error::FactbaseError;
use crate::models::TemporalStats;
use crate::patterns::normalize_date_for_comparison;
use std::collections::HashMap;

impl Database {
    /// Compute temporal tag statistics for a repository (with caching)
    pub fn compute_temporal_stats(&self, repo_id: &str) -> Result<TemporalStats, FactbaseError> {
        if let Some(cached) = self.get_cached_stats(repo_id) {
            if let Some(temporal) = cached.temporal {
                return Ok(temporal);
            }
        }

        let conn = self.get_conn()?;
        let docs = super::fetch_active_doc_content(&conn, repo_id)?;

        let mut total_facts = 0usize;
        let mut facts_with_tags = 0usize;
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut oldest_date: Option<String> = None;
        let mut newest_date: Option<String> = None;

        for doc in &docs {
            total_facts += doc.metadata.fact_stats.total_facts;
            facts_with_tags += doc.metadata.fact_stats.facts_with_tags;

            for tag in &doc.metadata.temporal_tags {
                let type_name = format!("{:?}", tag.tag_type);
                *by_type.entry(type_name).or_insert(0) += 1;

                for date in [&tag.start_date, &tag.end_date].into_iter().flatten() {
                    let normalized = normalize_date_for_comparison(date);
                    if let Some(ref old) = oldest_date {
                        if normalized < normalize_date_for_comparison(old) {
                            oldest_date = Some(date.clone());
                        }
                    } else {
                        oldest_date = Some(date.clone());
                    }
                    if let Some(ref new) = newest_date {
                        if normalized > normalize_date_for_comparison(new) {
                            newest_date = Some(date.clone());
                        }
                    } else {
                        newest_date = Some(date.clone());
                    }
                }
            }
        }

        let coverage_percent = if total_facts > 0 {
            (facts_with_tags as f32 / total_facts as f32) * 100.0
        } else {
            0.0
        };

        let result = TemporalStats {
            total_facts,
            facts_with_tags,
            coverage_percent,
            by_type,
            oldest_date,
            newest_date,
        };

        self.cache_temporal_stats(repo_id, result.clone());

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::content_hash;
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_temporal_stats_caching() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "- Fact one @t[2020..2022]\n- Fact two @t[2023..]".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats1 = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats1.total_facts, 2);
        assert_eq!(stats1.facts_with_tags, 2);

        let stats2 = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats2.total_facts, stats1.total_facts);
        assert_eq!(stats2.facts_with_tags, stats1.facts_with_tags);

        doc.content = "- Fact one @t[2020..2022]\n- Fact two\n- Fact three".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats3 = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats3.total_facts, 3);
        assert_eq!(stats3.facts_with_tags, 1);
    }

    #[test]
    fn test_temporal_stats_empty_repo() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let stats = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert!(stats.by_type.is_empty());
        assert!(stats.oldest_date.is_none());
        assert!(stats.newest_date.is_none());
    }

    #[test]
    fn test_temporal_stats_no_tags() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "- Fact one\n- Fact two\n- Fact three".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats.total_facts, 3);
        assert_eq!(stats.facts_with_tags, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert!(stats.by_type.is_empty());
        assert!(stats.oldest_date.is_none());
        assert!(stats.newest_date.is_none());
    }

    #[test]
    fn test_temporal_stats_various_tag_types() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "- Point in time @t[=2020-06]\n\
                       - Last known @t[~2024-01]\n\
                       - Date range @t[2020..2022]\n\
                       - Ongoing @t[2021..]\n\
                       - Historical @t[..2019]\n\
                       - Unknown @t[?]"
            .to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats.total_facts, 6);
        assert_eq!(stats.facts_with_tags, 6);
        assert!((stats.coverage_percent - 100.0).abs() < 0.01);

        // Check tag type distribution
        assert!(stats.by_type.len() >= 4); // At least 4 different tag types
    }

    #[test]
    fn test_temporal_stats_date_range_tracking() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "- Early fact @t[2015-03]\n\
                       - Middle fact @t[2020..2022]\n\
                       - Recent fact @t[2024-12-15]"
            .to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats = db.compute_temporal_stats(&repo.id).expect("compute");
        assert_eq!(stats.oldest_date, Some("2015-03".to_string()));
        assert_eq!(stats.newest_date, Some("2024-12-15".to_string()));
    }
}
