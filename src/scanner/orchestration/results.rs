//! Result building helpers for scan orchestration

use crate::models::TemporalScanStats;
use crate::{ScanResult, ScanStats};

/// Parameters for building an interrupted scan result
pub(super) struct InterruptedResultParams {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub moved: usize,
    pub reindexed: usize,
    pub links_detected: usize,
    pub total_facts: usize,
    pub facts_with_tags: usize,
    pub facts_with_sources: usize,
    pub below_threshold_docs: usize,
    pub file_discovery_ms: u64,
    pub parsing_ms: u64,
    pub embedding_ms: u64,
    pub db_write_ms: u64,
    pub link_detection_ms: u64,
    pub total_ms: u64,
    pub docs_embedded: usize,
    pub docs_link_detected: usize,
    pub fact_embeddings_generated: usize,
    pub file_offset: usize,
}

/// Build a ScanResult for an interrupted scan
pub(super) fn build_interrupted_result(params: InterruptedResultParams) -> ScanResult {
    let overall_coverage = if params.total_facts > 0 {
        params.facts_with_tags as f32 / params.total_facts as f32
    } else {
        1.0
    };
    let source_coverage = if params.total_facts > 0 {
        params.facts_with_sources as f32 / params.total_facts as f32
    } else {
        1.0
    };

    ScanResult {
        added: params.added,
        updated: params.updated,
        deleted: params.deleted,
        unchanged: params.unchanged,
        moved: params.moved,
        reindexed: params.reindexed,
        links_detected: params.links_detected,
        fact_embeddings_generated: params.fact_embeddings_generated,
        fact_embeddings_needed: 0,
        total: params.added + params.updated + params.unchanged + params.moved + params.reindexed,
        duplicates: vec![],
        stats: Some(ScanStats {
            file_discovery_ms: params.file_discovery_ms,
            parsing_ms: params.parsing_ms,
            embedding_ms: params.embedding_ms,
            db_write_ms: params.db_write_ms,
            link_detection_ms: params.link_detection_ms,
            total_ms: params.total_ms,
            docs_embedded: params.docs_embedded,
            docs_link_detected: params.docs_link_detected,
            slowest_files: vec![],
        }),
        temporal_stats: Some(TemporalScanStats {
            total_facts: params.total_facts,
            facts_with_tags: params.facts_with_tags,
            coverage: overall_coverage,
            below_threshold_docs: params.below_threshold_docs,
            facts_with_sources: params.facts_with_sources,
            source_coverage,
        }),
        interrupted: true,
        embeddings_skipped: false,
        file_offset: params.file_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> InterruptedResultParams {
        InterruptedResultParams {
            added: 0,
            updated: 0,
            deleted: 0,
            unchanged: 0,
            moved: 0,
            reindexed: 0,
            links_detected: 0,
            total_facts: 0,
            facts_with_tags: 0,
            facts_with_sources: 0,
            below_threshold_docs: 0,
            file_discovery_ms: 0,
            parsing_ms: 0,
            embedding_ms: 0,
            db_write_ms: 0,
            link_detection_ms: 0,
            total_ms: 0,
            docs_embedded: 0,
            docs_link_detected: 0,
            fact_embeddings_generated: 0,
            file_offset: 0,
        }
    }

    #[test]
    fn test_coverage_zero_facts_returns_one() {
        let params = InterruptedResultParams {
            total_facts: 0,
            facts_with_tags: 0,
            ..default_params()
        };
        let result = build_interrupted_result(params);
        assert_eq!(result.temporal_stats.unwrap().coverage, 1.0);
    }

    #[test]
    fn test_coverage_all_facts_tagged() {
        let params = InterruptedResultParams {
            total_facts: 10,
            facts_with_tags: 10,
            ..default_params()
        };
        let result = build_interrupted_result(params);
        assert_eq!(result.temporal_stats.unwrap().coverage, 1.0);
    }

    #[test]
    fn test_coverage_partial_tags() {
        let params = InterruptedResultParams {
            total_facts: 100,
            facts_with_tags: 75,
            ..default_params()
        };
        let result = build_interrupted_result(params);
        assert!((result.temporal_stats.unwrap().coverage - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_total_calculation() {
        let params = InterruptedResultParams {
            added: 5,
            updated: 3,
            unchanged: 10,
            moved: 2,
            reindexed: 1,
            ..default_params()
        };
        let result = build_interrupted_result(params);
        // total = added + updated + unchanged + moved + reindexed
        assert_eq!(result.total, 21);
    }

    #[test]
    fn test_all_fields_mapped() {
        let params = InterruptedResultParams {
            added: 1,
            updated: 2,
            deleted: 3,
            unchanged: 4,
            moved: 5,
            reindexed: 6,
            links_detected: 7,
            total_facts: 100,
            facts_with_tags: 80,
            facts_with_sources: 60,
            below_threshold_docs: 8,
            file_discovery_ms: 10,
            parsing_ms: 20,
            embedding_ms: 30,
            db_write_ms: 40,
            link_detection_ms: 50,
            total_ms: 150,
            docs_embedded: 9,
            docs_link_detected: 11,
            fact_embeddings_generated: 42,
            file_offset: 99,
        };
        let result = build_interrupted_result(params);

        // ScanResult fields
        assert_eq!(result.added, 1);
        assert_eq!(result.updated, 2);
        assert_eq!(result.deleted, 3);
        assert_eq!(result.unchanged, 4);
        assert_eq!(result.moved, 5);
        assert_eq!(result.reindexed, 6);
        assert_eq!(result.links_detected, 7);
        assert_eq!(result.fact_embeddings_generated, 42);
        assert_eq!(result.file_offset, 99);
        assert!(result.interrupted);
        assert!(result.duplicates.is_empty());

        // ScanStats fields
        let stats = result.stats.unwrap();
        assert_eq!(stats.file_discovery_ms, 10);
        assert_eq!(stats.parsing_ms, 20);
        assert_eq!(stats.embedding_ms, 30);
        assert_eq!(stats.db_write_ms, 40);
        assert_eq!(stats.link_detection_ms, 50);
        assert_eq!(stats.total_ms, 150);
        assert_eq!(stats.docs_embedded, 9);
        assert_eq!(stats.docs_link_detected, 11);
        assert!(stats.slowest_files.is_empty());

        // TemporalScanStats fields
        let temporal = result.temporal_stats.unwrap();
        assert_eq!(temporal.total_facts, 100);
        assert_eq!(temporal.facts_with_tags, 80);
        assert_eq!(temporal.below_threshold_docs, 8);
    }

    #[test]
    fn test_interrupted_always_true() {
        let result = build_interrupted_result(default_params());
        assert!(result.interrupted);
    }

    #[test]
    fn test_file_offset_propagated() {
        let params = InterruptedResultParams {
            file_offset: 42,
            ..default_params()
        };
        let result = build_interrupted_result(params);
        assert_eq!(result.file_offset, 42);
    }
}
