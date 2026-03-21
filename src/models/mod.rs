//! Data models for Factbase
//!
//! This module contains all data structures used throughout the application,
//! organized into focused submodules by domain.

mod document;
pub(crate) mod format;
mod question;
pub(crate) mod repository;
mod scan;
mod search;
mod stats;
mod temporal;

// Link struct (small enough to live here)
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub source_id: String,
    pub target_id: String,
    pub context: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Re-export all public items for backward compatibility
pub use document::{word_count, Document};
pub use format::{
    ensure_obsidian_gitignore, write_obsidian_app_json, write_obsidian_css_snippet, FormatConfig,
    IdPlacement, LinkStyle, ResolvedFormat,
};
pub use question::{QuestionType, ReviewQuestion};
pub use repository::{
    load_perspective_from_file, CitationPattern, Perspective, Repository, ReviewPerspective,
    PERSPECTIVE_TEMPLATE,
};
pub use scan::{DuplicatePair, FileTimingInfo, ScanResult, ScanStats, TemporalScanStats};
pub use search::{
    ContentMatch, ContentSearchResult, FactPair, FactSearchResult, PaginatedSearchResult,
    SearchResult,
};
pub use stats::{
    CompressionStats, DetailedStats, PoolStats, RepoStats, SourceStats, TemporalStats,
};
pub use temporal::{FactStats, SourceDefinition, SourceReference, TemporalTag, TemporalTagType};

/// Returns a canonical (smaller, larger) pair of owned strings for deduplication.
pub fn normalize_pair(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pair_ordering() {
        assert_eq!(normalize_pair("aaa", "bbb"), ("aaa".into(), "bbb".into()));
        assert_eq!(normalize_pair("bbb", "aaa"), ("aaa".into(), "bbb".into()));
        assert_eq!(normalize_pair("abc", "abc"), ("abc".into(), "abc".into()));
    }

    #[test]
    fn test_file_timing_info() {
        let info = FileTimingInfo {
            file_path: "people/john.md".to_string(),
            title: "John Doe".to_string(),
            size_bytes: 1024,
            embedding_ms: 150,
            total_ms: 200,
        };
        assert_eq!(info.file_path, "people/john.md");
        assert_eq!(info.title, "John Doe");
        assert_eq!(info.size_bytes, 1024);
        assert_eq!(info.embedding_ms, 150);
        assert_eq!(info.total_ms, 200);
    }

    #[test]
    fn test_scan_stats_with_slowest_files() {
        let stats = ScanStats {
            file_discovery_ms: 10,
            parsing_ms: 20,
            embedding_ms: 100,
            db_write_ms: 30,
            link_detection_ms: 50,
            total_ms: 210,
            docs_embedded: 5,
            docs_link_detected: 5,
            slowest_files: vec![
                FileTimingInfo {
                    file_path: "large.md".to_string(),
                    title: "Large Doc".to_string(),
                    size_bytes: 50000,
                    embedding_ms: 80,
                    total_ms: 100,
                },
                FileTimingInfo {
                    file_path: "medium.md".to_string(),
                    title: "Medium Doc".to_string(),
                    size_bytes: 10000,
                    embedding_ms: 20,
                    total_ms: 30,
                },
            ],
        };
        assert_eq!(stats.slowest_files.len(), 2);
        assert_eq!(stats.slowest_files[0].file_path, "large.md");
        assert_eq!(stats.slowest_files[0].embedding_ms, 80);
    }

    #[test]
    fn test_scan_stats_default_empty_slowest_files() {
        let stats = ScanStats::default();
        assert!(stats.slowest_files.is_empty());
    }
}
