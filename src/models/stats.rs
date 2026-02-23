use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Basic repository statistics.
#[derive(Debug, Clone, Default)]
pub struct RepoStats {
    /// Total number of documents
    pub total: usize,
    /// Number of active (non-deleted) documents
    pub active: usize,
    /// Number of soft-deleted documents
    pub deleted: usize,
    /// Document count grouped by type
    pub by_type: HashMap<String, usize>,
}

/// Detailed repository statistics including links, sizes, and temporal data.
#[derive(Debug, Clone, Default)]
pub struct DetailedStats {
    /// Top documents by incoming link count: (id, title, count)
    pub most_linked: Vec<(String, String, usize)>,
    /// Documents with no links in or out: (id, title)
    pub orphans: Vec<(String, String)>,
    /// Average document size in bytes
    pub avg_doc_size: usize,
    /// Total word count across all documents
    pub total_words: usize,
    /// Average words per document
    pub avg_words_per_doc: usize,
    /// Compression statistics (only when compression feature enabled)
    pub compression_stats: Option<CompressionStats>,
    /// Oldest document: (id, title, date)
    pub oldest_doc: Option<(String, String, DateTime<Utc>)>,
    /// Newest document: (id, title, date)
    pub newest_doc: Option<(String, String, DateTime<Utc>)>,
    /// Temporal tag statistics
    pub temporal_stats: Option<TemporalStats>,
    /// Source attribution statistics
    pub source_stats: Option<SourceStats>,
}

/// Temporal tag statistics for status --detailed output
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalStats {
    /// Total facts across all documents
    pub total_facts: usize,
    /// Facts with at least one temporal tag
    pub facts_with_tags: usize,
    /// Coverage percentage (0.0 to 100.0)
    pub coverage_percent: f32,
    /// Count by tag type (PointInTime, LastSeen, Range, Ongoing, Historical, Unknown)
    pub by_type: HashMap<String, usize>,
    /// Oldest date found in any temporal tag
    pub oldest_date: Option<String>,
    /// Newest date found in any temporal tag
    pub newest_date: Option<String>,
}

/// Source attribution statistics for status --detailed output
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceStats {
    /// Total facts across all documents
    pub total_facts: usize,
    /// Facts with at least one source reference
    pub facts_with_sources: usize,
    /// Coverage percentage (0.0 to 100.0)
    pub coverage_percent: f32,
    /// Count by source type (LinkedIn, News, Website, etc.)
    pub by_type: HashMap<String, usize>,
    /// Oldest source date found in any footnote definition
    pub oldest_source_date: Option<String>,
    /// Newest source date found in any footnote definition
    pub newest_source_date: Option<String>,
    /// Count of orphan references (refs without definitions)
    pub orphan_references: usize,
    /// Count of orphan definitions (defs without references)
    pub orphan_definitions: usize,
}

/// Database compression statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStats {
    /// Number of documents stored with compression
    pub compressed_docs: usize,
    /// Total number of documents
    pub total_docs: usize,
    /// Total size in database (compressed bytes)
    pub compressed_size: usize,
    /// Total size after decompression (original bytes)
    pub original_size: usize,
    /// Percentage of space saved by compression
    pub savings_percent: f64,
}

/// Database connection pool statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Current active connections
    pub connections: u32,
    /// Idle connections in pool
    pub idle_connections: u32,
    /// Maximum pool size
    pub max_size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_stats_default() {
        let stats = TemporalStats::default();
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert!(stats.by_type.is_empty());
        assert!(stats.oldest_date.is_none());
        assert!(stats.newest_date.is_none());
    }

    #[test]
    fn test_temporal_stats_with_data() {
        let mut by_type = HashMap::new();
        by_type.insert("Range".to_string(), 5);
        by_type.insert("PointInTime".to_string(), 3);

        let stats = TemporalStats {
            total_facts: 20,
            facts_with_tags: 17,
            coverage_percent: 85.0,
            by_type,
            oldest_date: Some("2020-01".to_string()),
            newest_date: Some("2024-06".to_string()),
        };

        assert_eq!(stats.total_facts, 20);
        assert_eq!(stats.facts_with_tags, 17);
        assert_eq!(stats.coverage_percent, 85.0);
        assert_eq!(stats.by_type.get("Range"), Some(&5));
        assert_eq!(stats.by_type.get("PointInTime"), Some(&3));
        assert_eq!(stats.oldest_date, Some("2020-01".to_string()));
        assert_eq!(stats.newest_date, Some("2024-06".to_string()));
    }

    #[test]
    fn test_source_stats_default() {
        let stats = SourceStats::default();
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
    fn test_source_stats_with_data() {
        let mut by_type = HashMap::new();
        by_type.insert("LinkedIn".to_string(), 5);
        by_type.insert("News".to_string(), 3);

        let stats = SourceStats {
            total_facts: 20,
            facts_with_sources: 14,
            coverage_percent: 70.0,
            by_type,
            oldest_source_date: Some("2020-01-15".to_string()),
            newest_source_date: Some("2024-06-20".to_string()),
            orphan_references: 2,
            orphan_definitions: 1,
        };

        assert_eq!(stats.total_facts, 20);
        assert_eq!(stats.facts_with_sources, 14);
        assert_eq!(stats.coverage_percent, 70.0);
        assert_eq!(stats.by_type.get("LinkedIn"), Some(&5));
        assert_eq!(stats.by_type.get("News"), Some(&3));
        assert_eq!(stats.oldest_source_date, Some("2020-01-15".to_string()));
        assert_eq!(stats.newest_source_date, Some("2024-06-20".to_string()));
        assert_eq!(stats.orphan_references, 2);
        assert_eq!(stats.orphan_definitions, 1);
    }
}
