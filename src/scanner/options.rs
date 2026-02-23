//! Scan options configuration

use crate::Config;

/// Options for controlling scan behavior
pub struct ScanOptions {
    /// Enable verbose output during scan
    pub verbose: bool,
    /// Preview changes without writing to database
    pub dry_run: bool,
    /// Show progress bar during scan
    pub show_progress: bool,
    /// Check for duplicate documents after indexing
    pub check_duplicates: bool,
    /// Maximum chunk size in bytes for document splitting
    pub chunk_size: usize,
    /// Overlap in bytes between adjacent chunks
    pub chunk_overlap: usize,
    /// Collect timing statistics during scan
    pub collect_stats: bool,
    /// Only process files modified after this timestamp
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Minimum temporal tag coverage threshold (0.0 to 1.0)
    pub min_coverage: f32,
    /// Force re-generation of embeddings even if content unchanged
    pub force_reindex: bool,
    /// Batch size for embedding generation
    pub embedding_batch_size: usize,
    /// Batch size for link detection
    pub link_batch_size: usize,
    /// Skip link detection phase for faster indexing
    pub skip_links: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            verbose: false,
            dry_run: false,
            show_progress: false,
            check_duplicates: false,
            chunk_size: 100_000,
            chunk_overlap: 2_000,
            collect_stats: false,
            since: None,
            min_coverage: 0.8,
            force_reindex: false,
            embedding_batch_size: 10,
            link_batch_size: 5,
            skip_links: false,
        }
    }
}

impl ScanOptions {
    /// Create `ScanOptions` with config-derived values, defaulting behavioral flags.
    pub fn from_config(config: &Config) -> Self {
        Self {
            chunk_size: config.processor.chunk_size,
            chunk_overlap: config.processor.chunk_overlap,
            embedding_batch_size: config.processor.embedding_batch_size,
            link_batch_size: config.processor.link_batch_size,
            min_coverage: config.temporal.min_coverage,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TemporalScanStats;

    #[test]
    fn test_scan_options_default() {
        let opts = ScanOptions::default();
        assert!(!opts.verbose);
        assert!(!opts.dry_run);
        assert!(!opts.check_duplicates);
        assert_eq!(opts.chunk_size, 100_000);
        assert_eq!(opts.chunk_overlap, 2_000);
        assert_eq!(opts.min_coverage, 0.8);
        assert_eq!(opts.embedding_batch_size, 10);
        assert!(!opts.force_reindex);
        assert!(!opts.skip_links);
    }

    #[test]
    fn test_scan_options_from_config() {
        let mut config = Config::default();
        config.processor.chunk_size = 50_000;
        config.processor.chunk_overlap = 1_000;
        config.processor.embedding_batch_size = 5;
        config.temporal.min_coverage = 0.6;

        let opts = ScanOptions::from_config(&config);
        assert_eq!(opts.chunk_size, 50_000);
        assert_eq!(opts.chunk_overlap, 1_000);
        assert_eq!(opts.embedding_batch_size, 5);
        assert!((opts.min_coverage - 0.6).abs() < f32::EPSILON);
        // Behavioral flags default to false/None
        assert!(!opts.verbose);
        assert!(!opts.dry_run);
        assert!(!opts.force_reindex);
        assert!(opts.since.is_none());
    }

    #[test]
    fn test_temporal_scan_stats_default() {
        let stats = TemporalScanStats::default();
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert_eq!(stats.coverage, 0.0);
        assert_eq!(stats.below_threshold_docs, 0);
    }
}
