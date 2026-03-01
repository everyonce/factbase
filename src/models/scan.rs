use serde::{Deserialize, Serialize};

/// Results from a scan operation, tracking document changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanResult {
    /// Number of newly added documents
    pub added: usize,
    /// Number of updated documents (content changed)
    pub updated: usize,
    /// Number of deleted documents (file removed)
    pub deleted: usize,
    /// Number of unchanged documents
    pub unchanged: usize,
    /// Number of moved documents (same ID, different path)
    pub moved: usize,
    /// Number of documents re-indexed (forced reindex)
    pub reindexed: usize,
    /// Number of entity links detected
    pub links_detected: usize,
    /// Number of fact-level embeddings generated
    pub fact_embeddings_generated: usize,
    /// Total documents processed
    pub total: usize,
    /// Detected duplicate document pairs
    pub duplicates: Vec<DuplicatePair>,
    /// Optional timing statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ScanStats>,
    /// Optional temporal tag statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_stats: Option<TemporalScanStats>,
    /// True if scan was interrupted by Ctrl+C
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    /// True if embedding generation was skipped (--no-embed)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embeddings_skipped: bool,
}

impl std::fmt::Display for ScanResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.embeddings_skipped {
            write!(
                f,
                "Indexed {} documents (embeddings skipped \u{2014} use embeddings import to load)",
                self.total
            )
        } else if self.reindexed > 0 {
            write!(
                f,
                "{} added, {} updated, {} reindexed, {} deleted, {} moved, {} unchanged (total: {})",
                self.added, self.updated, self.reindexed, self.deleted, self.moved, self.unchanged, self.total
            )
        } else {
            write!(
                f,
                "{} added, {} updated, {} deleted, {} moved, {} unchanged (total: {})",
                self.added, self.updated, self.deleted, self.moved, self.unchanged, self.total
            )
        }
    }
}

/// Timing statistics for a scan operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    /// Time spent discovering files (ms)
    pub file_discovery_ms: u64,
    /// Time spent parsing documents (ms)
    pub parsing_ms: u64,
    /// Time spent generating embeddings (ms)
    pub embedding_ms: u64,
    /// Time spent writing to database (ms)
    pub db_write_ms: u64,
    /// Time spent on LLM link detection (ms)
    pub link_detection_ms: u64,
    /// Total scan time (ms)
    pub total_ms: u64,
    /// Number of documents that had embeddings generated
    pub docs_embedded: usize,
    /// Number of documents that had link detection run
    pub docs_link_detected: usize,
    /// Per-file timing for slowest files (sorted by total_ms descending)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slowest_files: Vec<FileTimingInfo>,
}

/// Timing breakdown for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTimingInfo {
    /// Relative path to the file
    pub file_path: String,
    /// Document title
    pub title: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Time spent generating embedding (ms)
    pub embedding_ms: u64,
    /// Total processing time (ms)
    pub total_ms: u64,
}

/// A pair of documents detected as potential duplicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicatePair {
    /// First document ID
    pub doc1_id: String,
    /// First document title
    pub doc1_title: String,
    /// Second document ID
    pub doc2_id: String,
    /// Second document title
    pub doc2_title: String,
    /// Cosine similarity score between the two documents
    pub similarity: f32,
}

/// Aggregated temporal tag statistics from a scan
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalScanStats {
    /// Total facts across all scanned documents
    pub total_facts: usize,
    /// Facts with at least one temporal tag
    pub facts_with_tags: usize,
    /// Overall coverage percentage (0.0 to 1.0)
    pub coverage: f32,
    /// Number of documents below the coverage threshold
    pub below_threshold_docs: usize,
    /// Facts with at least one source footnote
    pub facts_with_sources: usize,
    /// Source coverage percentage (0.0 to 1.0)
    pub source_coverage: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_result_default() {
        let r = ScanResult::default();
        assert_eq!(r.added, 0);
        assert_eq!(r.updated, 0);
        assert_eq!(r.deleted, 0);
        assert_eq!(r.unchanged, 0);
        assert_eq!(r.moved, 0);
        assert_eq!(r.reindexed, 0);
        assert_eq!(r.links_detected, 0);
        assert_eq!(r.total, 0);
        assert!(r.duplicates.is_empty());
        assert!(r.stats.is_none());
        assert!(r.temporal_stats.is_none());
        assert!(!r.interrupted);
        assert!(!r.embeddings_skipped);
    }

    #[test]
    fn test_scan_stats_default() {
        let s = ScanStats::default();
        assert_eq!(s.file_discovery_ms, 0);
        assert_eq!(s.parsing_ms, 0);
        assert_eq!(s.embedding_ms, 0);
        assert_eq!(s.db_write_ms, 0);
        assert_eq!(s.link_detection_ms, 0);
        assert_eq!(s.total_ms, 0);
        assert_eq!(s.docs_embedded, 0);
        assert_eq!(s.docs_link_detected, 0);
        assert!(s.slowest_files.is_empty());
    }

    #[test]
    fn test_temporal_scan_stats_default() {
        let t = TemporalScanStats::default();
        assert_eq!(t.total_facts, 0);
        assert_eq!(t.facts_with_tags, 0);
        assert!((t.coverage - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.below_threshold_docs, 0);
        assert_eq!(t.facts_with_sources, 0);
        assert!((t.source_coverage - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scan_result_display_without_reindexed() {
        let r = ScanResult {
            added: 3,
            updated: 1,
            deleted: 2,
            moved: 1,
            unchanged: 5,
            total: 12,
            ..Default::default()
        };
        let s = format!("{r}");
        assert_eq!(
            s,
            "3 added, 1 updated, 2 deleted, 1 moved, 5 unchanged (total: 12)"
        );
    }

    #[test]
    fn test_scan_result_display_with_reindexed() {
        let r = ScanResult {
            added: 1,
            updated: 0,
            reindexed: 4,
            deleted: 0,
            moved: 0,
            unchanged: 3,
            total: 8,
            ..Default::default()
        };
        let s = format!("{r}");
        assert_eq!(
            s,
            "1 added, 0 updated, 4 reindexed, 0 deleted, 0 moved, 3 unchanged (total: 8)"
        );
    }

    #[test]
    fn test_scan_result_display_embeddings_skipped() {
        let r = ScanResult {
            added: 10,
            updated: 2,
            total: 15,
            embeddings_skipped: true,
            ..Default::default()
        };
        let s = format!("{r}");
        assert!(s.contains("Indexed 15 documents"));
        assert!(s.contains("embeddings skipped"));
    }
}
