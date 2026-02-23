//! Document metadata cache for avoiding repeated parsing.
//!
//! Caches parsed temporal tags, source references, source definitions, and fact stats
//! to reduce CPU usage for repeated operations (lint, status, search with filters).

use crate::models::{FactStats, SourceDefinition, SourceReference, TemporalTag};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Default cache size (number of documents)
const DEFAULT_CACHE_SIZE: usize = 100;

/// Cached metadata for a single document
#[derive(Debug, Clone)]
pub(crate) struct DocumentMetadata {
    pub(crate) temporal_tags: Vec<TemporalTag>,
    pub(crate) source_refs: Vec<SourceReference>,
    pub(crate) source_defs: Vec<SourceDefinition>,
    pub(crate) fact_stats: FactStats,
}

/// Cache key combining document ID and content hash
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    doc_id: String,
    content_hash: String,
}

/// Thread-safe LRU cache for document metadata
pub(crate) struct DocumentCache {
    cache: Mutex<LruCache<CacheKey, DocumentMetadata>>,
}

impl DocumentCache {
    /// Create a new cache with the specified capacity
    pub(crate) fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(
            NonZeroUsize::new(DEFAULT_CACHE_SIZE).expect("DEFAULT_CACHE_SIZE is non-zero"),
        );
        Self {
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Get cached metadata for a document, if available
    pub(crate) fn get(&self, doc_id: &str, content_hash: &str) -> Option<DocumentMetadata> {
        let key = CacheKey {
            doc_id: doc_id.to_string(),
            content_hash: content_hash.to_string(),
        };
        if let Ok(mut cache) = self.cache.lock() {
            cache.get(&key).cloned()
        } else {
            None // Graceful degradation on poisoned mutex
        }
    }

    /// Store metadata in the cache
    pub(crate) fn put(&self, doc_id: &str, content_hash: &str, metadata: DocumentMetadata) {
        let key = CacheKey {
            doc_id: doc_id.to_string(),
            content_hash: content_hash.to_string(),
        };
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, metadata);
        }
        // Silently skip caching on poisoned mutex
    }
}

#[cfg(test)]
impl DocumentCache {
    fn invalidate(&self, doc_id: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            let keys_to_remove: Vec<CacheKey> = cache
                .iter()
                .filter(|(k, _)| k.doc_id == doc_id)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys_to_remove {
                cache.pop(&key);
            }
        }
    }

    fn clear(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn len(&self) -> usize {
        if let Ok(cache) = self.cache.lock() {
            cache.len()
        } else {
            0
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn capacity(&self) -> usize {
        if let Ok(cache) = self.cache.lock() {
            cache.cap().get()
        } else {
            0
        }
    }
}

impl Default for DocumentCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_SIZE)
    }
}

/// Global document cache instance
static DOCUMENT_CACHE: std::sync::OnceLock<DocumentCache> = std::sync::OnceLock::new();

/// Initialize the global document cache with the specified capacity
pub(crate) fn init_document_cache(capacity: usize) {
    let _ = DOCUMENT_CACHE.get_or_init(|| DocumentCache::new(capacity));
}

/// Get the global document cache, initializing with default capacity if needed
pub(crate) fn get_document_cache() -> &'static DocumentCache {
    DOCUMENT_CACHE.get_or_init(DocumentCache::default)
}

/// Get or compute document metadata, using cache when available
pub(crate) fn get_or_compute_metadata(
    doc_id: &str,
    content_hash: &str,
    content: &str,
) -> DocumentMetadata {
    let cache = get_document_cache();

    // Check cache first
    if let Some(metadata) = cache.get(doc_id, content_hash) {
        return metadata;
    }

    // Compute metadata
    let metadata = DocumentMetadata {
        temporal_tags: crate::processor::parse_temporal_tags(content),
        source_refs: crate::processor::parse_source_references(content),
        source_defs: crate::processor::parse_source_definitions(content),
        fact_stats: crate::processor::calculate_fact_stats(content),
    };

    // Store in cache
    cache.put(doc_id, content_hash, metadata.clone());

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_cache_new() {
        let cache = DocumentCache::new(50);
        assert_eq!(cache.capacity(), 50);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_document_cache_put_get() {
        let cache = DocumentCache::new(10);
        let metadata = DocumentMetadata {
            temporal_tags: vec![],
            source_refs: vec![],
            source_defs: vec![],
            fact_stats: FactStats::default(),
        };

        cache.put("doc1", "hash1", metadata.clone());
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("doc1", "hash1");
        assert!(retrieved.is_some());

        // Different hash should not match
        let not_found = cache.get("doc1", "hash2");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_document_cache_invalidate() {
        let cache = DocumentCache::new(10);
        let metadata = DocumentMetadata {
            temporal_tags: vec![],
            source_refs: vec![],
            source_defs: vec![],
            fact_stats: FactStats::default(),
        };

        cache.put("doc1", "hash1", metadata.clone());
        cache.put("doc1", "hash2", metadata.clone());
        cache.put("doc2", "hash1", metadata.clone());
        assert_eq!(cache.len(), 3);

        cache.invalidate("doc1");
        assert_eq!(cache.len(), 1);

        // doc2 should still be there
        assert!(cache.get("doc2", "hash1").is_some());
    }

    #[test]
    fn test_document_cache_clear() {
        let cache = DocumentCache::new(10);
        let metadata = DocumentMetadata {
            temporal_tags: vec![],
            source_refs: vec![],
            source_defs: vec![],
            fact_stats: FactStats::default(),
        };

        cache.put("doc1", "hash1", metadata.clone());
        cache.put("doc2", "hash2", metadata.clone());
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_document_cache_lru_eviction() {
        let cache = DocumentCache::new(2);
        let metadata = DocumentMetadata {
            temporal_tags: vec![],
            source_refs: vec![],
            source_defs: vec![],
            fact_stats: FactStats::default(),
        };

        cache.put("doc1", "hash1", metadata.clone());
        cache.put("doc2", "hash2", metadata.clone());
        cache.put("doc3", "hash3", metadata.clone()); // Should evict doc1

        assert_eq!(cache.len(), 2);
        assert!(cache.get("doc1", "hash1").is_none()); // Evicted
        assert!(cache.get("doc2", "hash2").is_some());
        assert!(cache.get("doc3", "hash3").is_some());
    }

    #[test]
    fn test_document_cache_default() {
        let cache = DocumentCache::default();
        assert_eq!(cache.capacity(), DEFAULT_CACHE_SIZE);
    }
}
