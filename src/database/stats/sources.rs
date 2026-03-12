//! Source attribution statistics computation.

use super::super::Database;
use crate::error::FactbaseError;
use crate::models::SourceStats;
use crate::processor::count_facts_with_sources;
use std::collections::{HashMap, HashSet};

impl Database {
    /// Compute source attribution statistics for a repository (with caching)
    pub fn compute_source_stats(&self, repo_id: &str) -> Result<SourceStats, FactbaseError> {
        if let Some(cached) = self.get_cached_stats(repo_id) {
            if let Some(source) = cached.source {
                return Ok(source);
            }
        }

        let conn = self.get_conn()?;
        let docs = super::fetch_active_doc_content(&conn, repo_id)?;

        let mut total_facts = 0usize;
        let mut facts_with_sources = 0usize;
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut oldest_source_date: Option<String> = None;
        let mut newest_source_date: Option<String> = None;
        let mut orphan_references = 0usize;
        let mut orphan_definitions = 0usize;

        for doc in &docs {
            total_facts += doc.metadata.fact_stats.total_facts;
            facts_with_sources += count_facts_with_sources(&doc.decoded);

            let refs = &doc.metadata.source_refs;
            let defs = &doc.metadata.source_defs;

            let defined_numbers: HashSet<_> = defs.iter().map(|d| d.number).collect();
            let referenced_numbers: HashSet<_> = refs.iter().map(|r| r.number).collect();

            orphan_references += refs
                .iter()
                .filter(|r| !defined_numbers.contains(&r.number))
                .count();
            orphan_definitions += defs
                .iter()
                .filter(|d| !referenced_numbers.contains(&d.number))
                .count();

            for def in defs {
                *by_type.entry(def.source_type.clone()).or_insert(0) += 1;

                if let Some(ref date) = def.date {
                    super::update_date_range(
                        date,
                        &mut oldest_source_date,
                        &mut newest_source_date,
                    );
                }
            }
        }

        let coverage_percent = if total_facts > 0 {
            (facts_with_sources as f32 / total_facts as f32) * 100.0
        } else {
            0.0
        };

        let result = SourceStats {
            total_facts,
            facts_with_sources,
            coverage_percent,
            by_type,
            oldest_source_date,
            newest_source_date,
            orphan_references,
            orphan_definitions,
        };

        self.cache_source_stats(repo_id, result.clone());

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::content_hash;
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_source_stats_caching() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content =
            "- Fact one [^1]\n- Fact two [^2]\n\n[^1]: LinkedIn, 2024-01\n[^2]: News, 2024-02"
                .to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats1 = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats1.total_facts, 2);
        assert_eq!(stats1.facts_with_sources, 2);

        let stats2 = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats2.total_facts, stats1.total_facts);

        doc.content = "- Fact one\n- Fact two\n- Fact three".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats3 = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats3.total_facts, 3);
        assert_eq!(stats3.facts_with_sources, 0);
    }

    #[test]
    fn test_source_stats_empty_repo() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let stats = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_sources, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert!(stats.by_type.is_empty());
        assert!(stats.oldest_source_date.is_none());
        assert!(stats.newest_source_date.is_none());
        assert_eq!(stats.orphan_references, 0);
        assert_eq!(stats.orphan_definitions, 0);
    }

    #[test]
    fn test_source_stats_orphan_detection() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        // [^1] referenced but not defined, [^3] defined but not referenced
        doc.content = "- Fact with orphan ref [^1]\n- Fact with valid ref [^2]\n\n[^2]: LinkedIn, 2024-01\n[^3]: News, 2024-02".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats.orphan_references, 1); // [^1] has no definition
        assert_eq!(stats.orphan_definitions, 1); // [^3] has no reference
    }

    #[test]
    fn test_source_stats_date_range_tracking() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).expect("add repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "- Fact [^1]\n- Fact [^2]\n- Fact [^3]\n\n[^1]: LinkedIn, 2022-06\n[^2]: News, 2024-01-15\n[^3]: Website, 2023".to_string();
        doc.file_hash = content_hash(&doc.content);
        db.upsert_document(&doc).expect("upsert");

        let stats = db.compute_source_stats(&repo.id).expect("compute");
        assert_eq!(stats.oldest_source_date, Some("2022-06".to_string()));
        assert_eq!(stats.newest_source_date, Some("2024-01-15".to_string()));
        assert_eq!(stats.by_type.get("LinkedIn"), Some(&1));
        assert_eq!(stats.by_type.get("News"), Some(&1));
        assert_eq!(stats.by_type.get("Website"), Some(&1));
    }
}
