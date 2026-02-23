//! Processor and watcher configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File watcher configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_ignore_patterns")]
    pub ignore_patterns: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            ignore_patterns: default_ignore_patterns(),
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}

fn default_ignore_patterns() -> Vec<String> {
    vec![
        "*.swp".into(),
        "*.tmp".into(),
        "*~".into(),
        ".*/**".into(),
        ".DS_Store".into(),
    ]
}

/// Document processor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorConfig {
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
    #[serde(default = "default_snippet_length")]
    pub snippet_length: usize,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: usize,
    #[serde(default = "default_link_batch_size")]
    pub link_batch_size: usize,
    #[serde(default = "default_lint_concurrency")]
    pub lint_concurrency: usize,
    #[serde(default = "default_metadata_cache_size")]
    pub metadata_cache_size: usize,
}

pub(crate) fn default_embedding_batch_size() -> usize {
    10
}

pub(crate) fn default_link_batch_size() -> usize {
    5
}

pub(crate) fn default_lint_concurrency() -> usize {
    5
}

fn default_max_file_size() -> usize {
    100_000
}

fn default_snippet_length() -> usize {
    200
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            max_file_size: default_max_file_size(),
            snippet_length: default_snippet_length(),
            chunk_size: default_chunk_size(),
            chunk_overlap: default_chunk_overlap(),
            embedding_batch_size: default_embedding_batch_size(),
            link_batch_size: default_link_batch_size(),
            lint_concurrency: default_lint_concurrency(),
            metadata_cache_size: default_metadata_cache_size(),
        }
    }
}

pub(crate) fn default_chunk_size() -> usize {
    100_000 // 100K chars, safe margin within 128K token limit
}

pub(crate) fn default_chunk_overlap() -> usize {
    2_000 // 2K chars overlap for context continuity
}

pub(crate) fn default_metadata_cache_size() -> usize {
    100 // Default cache size for parsed document metadata
}

/// Repository configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_chunk_size() {
        assert_eq!(default_chunk_size(), 100_000);
    }

    #[test]
    fn test_default_chunk_overlap() {
        assert_eq!(default_chunk_overlap(), 2_000);
    }

    #[test]
    fn test_default_embedding_batch_size() {
        assert_eq!(default_embedding_batch_size(), 10);
    }

    #[test]
    fn test_default_metadata_cache_size() {
        assert_eq!(default_metadata_cache_size(), 100);
    }
}
