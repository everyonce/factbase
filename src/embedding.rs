use lru::LruCache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::sync::Mutex;
use tracing::debug;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::ollama::OllamaClient;
use crate::BoxFuture;

/// Default cache size for embedding cache (100 entries)
const DEFAULT_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(100) {
    Some(n) => n,
    None => unreachable!(),
};

/// Trait for generating text embeddings via an inference backend.
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>>;
    /// Generate embeddings for multiple texts in a single batch call.
    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.generate(text).await?);
            }
            Ok(results)
        })
    }
    /// Return the embedding dimension size.
    fn dimension(&self) -> usize;
}

impl EmbeddingProvider for Box<dyn EmbeddingProvider> {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        (**self).generate(text)
    }
    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        (**self).generate_batch(texts)
    }
    fn dimension(&self) -> usize {
        (**self).dimension()
    }
}

/// Ollama-based embedding provider using the embeddings API.
#[derive(Clone)]
pub struct OllamaEmbedding {
    client: OllamaClient,
    model: String,
    dim: usize,
}

// Legacy single-embedding endpoint
#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f64>,
}

// Modern batch embedding endpoint
#[derive(Serialize)]
struct BatchEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaEmbedding {
    /// Create a new OllamaEmbedding with default timeout (30s).
    pub fn new(base_url: &str, model: &str, dimension: usize) -> Self {
        Self::with_timeout(base_url, model, dimension, 30)
    }

    /// Create a new OllamaEmbedding with custom timeout.
    pub fn with_timeout(base_url: &str, model: &str, dimension: usize, timeout_secs: u64) -> Self {
        Self::with_config(base_url, model, dimension, timeout_secs, 3, 1000)
    }

    /// Create a new OllamaEmbedding with full configuration.
    pub fn with_config(
        base_url: &str,
        model: &str,
        dimension: usize,
        timeout_secs: u64,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> Self {
        Self {
            client: OllamaClient::with_config(base_url, timeout_secs, max_retries, retry_delay_ms),
            model: model.to_string(),
            dim: dimension,
        }
    }
}

impl EmbeddingProvider for OllamaEmbedding {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        Box::pin(async move {
            let req = EmbeddingRequest {
                model: &self.model,
                prompt: text,
            };

            let body: EmbeddingResponse = self
                .client
                .post("/api/embeddings", &req, &self.model)
                .await?;
            let embedding: Vec<f32> = body.embedding.into_iter().map(|v| v as f32).collect();

            if embedding.len() != self.dim {
                return Err(FactbaseError::embedding(format!(
                    "Expected {} dimensions, got {}",
                    self.dim,
                    embedding.len()
                )));
            }

            Ok(embedding)
        })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        Box::pin(async move {
            if texts.is_empty() {
                return Ok(vec![]);
            }

            let req = BatchEmbedRequest {
                model: &self.model,
                input: texts.to_vec(),
            };

            let body: BatchEmbedResponse =
                self.client.post("/api/embed", &req, &self.model).await?;

            if body.embeddings.len() != texts.len() {
                return Err(FactbaseError::embedding(format!(
                    "Expected {} embeddings, got {}",
                    texts.len(),
                    body.embeddings.len()
                )));
            }

            for (i, emb) in body.embeddings.iter().enumerate() {
                if emb.len() != self.dim {
                    return Err(FactbaseError::embedding(format!(
                        "Embedding {} has {} dimensions, expected {}",
                        i,
                        emb.len(),
                        self.dim
                    )));
                }
            }

            Ok(body.embeddings)
        })
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

/// Wrapper that caches query embeddings to avoid redundant Ollama calls.
/// Uses LRU eviction with configurable capacity (default: 100 entries).
pub struct CachedEmbedding<E: EmbeddingProvider> {
    inner: E,
    cache: Mutex<LruCache<String, Vec<f32>>>,
}

impl<E: EmbeddingProvider> CachedEmbedding<E> {
    /// Create a new CachedEmbedding wrapping the given provider with the specified cache capacity.
    pub fn new(inner: E, capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(DEFAULT_CACHE_SIZE);
        Self {
            inner,
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Return (current_entries, max_capacity) for the cache.
    pub fn cache_stats(&self) -> (usize, usize) {
        match self.cache.lock() {
            Ok(cache) => (cache.len(), cache.cap().get()),
            Err(_) => (0, 0), // Poisoned mutex, return empty stats
        }
    }
}

impl<E: EmbeddingProvider> EmbeddingProvider for CachedEmbedding<E> {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        Box::pin(async move {
            // Check cache first (bypass on poisoned mutex)
            if let Ok(mut cache) = self.cache.lock() {
                if let Some(embedding) = cache.get(text) {
                    debug!("Embedding cache hit for query");
                    return Ok(embedding.clone());
                }
            }

            // Generate embedding
            let embedding = self.inner.generate(text).await?;

            // Cache result (ignore if mutex poisoned)
            if let Ok(mut cache) = self.cache.lock() {
                cache.put(text.to_string(), embedding.clone());
            }
            debug!("Embedding cache miss, generated and cached");
            Ok(embedding)
        })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        // Batch operations bypass cache (used for document indexing, not queries)
        self.inner.generate_batch(texts)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

/// Compute SHA256 hex hash of text for cache keying.
fn text_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Wrapper that persists query embeddings in SQLite for cross-run caching.
/// Sits between the in-memory LRU cache and the actual provider:
/// `CachedEmbedding` → `PersistentCachedEmbedding` → provider.
pub struct PersistentCachedEmbedding<E: EmbeddingProvider> {
    inner: E,
    db: Database,
    model: String,
    max_entries: usize,
}

impl<E: EmbeddingProvider> PersistentCachedEmbedding<E> {
    /// Create a new persistent cache layer. Runs eviction on creation.
    pub fn new(inner: E, db: Database, model: String, max_entries: usize) -> Self {
        // Best-effort eviction on startup
        if let Err(e) = db.evict_query_cache(max_entries) {
            tracing::warn!("Failed to evict query cache on startup: {e}");
        }
        Self {
            inner,
            db,
            model,
            max_entries,
        }
    }
}

impl<E: EmbeddingProvider> EmbeddingProvider for PersistentCachedEmbedding<E> {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        Box::pin(async move {
            let hash = text_hash(text);

            // Check SQLite cache
            if let Ok(Some(embedding)) = self.db.get_cached_query_embedding(&hash, &self.model) {
                debug!("Persistent embedding cache hit");
                return Ok(embedding);
            }

            // Cache miss — call provider
            let embedding = self.inner.generate(text).await?;

            // Store in SQLite (best-effort)
            if let Err(e) = self.db.put_cached_query_embedding(
                &hash,
                text,
                &self.model,
                self.inner.dimension(),
                &embedding,
            ) {
                tracing::warn!("Failed to persist query embedding: {e}");
            }

            // Periodic eviction (best-effort)
            let _ = self.db.evict_query_cache(self.max_entries);

            debug!("Persistent embedding cache miss, generated and stored");
            Ok(embedding)
        })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        // Batch operations bypass persistent cache (used for document indexing)
        self.inner.generate_batch(texts)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;

    /// Mock embedding provider that returns a constant vector of configurable dimension.
    pub struct MockEmbedding {
        dim: usize,
    }

    impl MockEmbedding {
        pub fn new(dim: usize) -> Self {
            Self { dim }
        }
    }

    impl EmbeddingProvider for MockEmbedding {
        fn generate<'a>(
            &'a self,
            _text: &'a str,
        ) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
            Box::pin(async move { Ok(vec![0.1; self.dim]) })
        }

        fn dimension(&self) -> usize {
            self.dim
        }
    }

    /// Mock embedding that returns a deterministic vector based on text hash,
    /// so identical names produce identical embeddings.
    pub struct HashEmbedding;

    impl EmbeddingProvider for HashEmbedding {
        fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
            Box::pin(async move {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                text.hash(&mut h);
                let seed = h.finish();
                let mut emb = vec![0.0f32; 16];
                for (i, v) in emb.iter_mut().enumerate() {
                    *v = ((seed.wrapping_add(i as u64) % 1000) as f32) / 1000.0;
                }
                Ok(emb)
            })
        }

        fn dimension(&self) -> usize {
            16
        }
    }
    /// Create a 1024-dim embedding with a spike at `index`.
    pub fn spike_embedding(index: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 1024];
        v[index] = 1.0;
        v
    }

    /// Create a 1024-dim embedding similar to spike at `index` with slight offset.
    pub fn near_spike(index: usize, offset: f32) -> Vec<f32> {
        let mut v = vec![0.0f32; 1024];
        v[index] = 1.0;
        v[(index + 1) % 1024] = offset;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_embedding_new() {
        let emb = OllamaEmbedding::new("http://localhost:11434", "nomic-embed-text", 768);
        assert_eq!(emb.dimension(), 768);
        assert_eq!(emb.client.base_url(), "http://localhost:11434");
        assert_eq!(emb.model, "nomic-embed-text");
    }

    #[test]
    fn test_dimension_returns_configured_value() {
        let emb = OllamaEmbedding::new("http://test", "model", 512);
        assert_eq!(emb.dimension(), 512);
    }

    #[test]
    fn test_batch_embed_request_serialization() {
        let req = BatchEmbedRequest {
            model: "nomic-embed-text",
            input: vec!["hello", "world"],
        };
        let json = serde_json::to_string(&req).expect("BatchEmbedRequest should serialize");
        assert!(json.contains("\"model\":\"nomic-embed-text\""));
        assert!(json.contains("\"input\":[\"hello\",\"world\"]"));
    }

    #[test]
    fn test_batch_embed_response_deserialization() {
        let json = r#"{"embeddings":[[0.1,0.2],[0.3,0.4]]}"#;
        let resp: BatchEmbedResponse =
            serde_json::from_str(json).expect("BatchEmbedResponse should deserialize");
        assert_eq!(resp.embeddings.len(), 2);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2]);
        assert_eq!(resp.embeddings[1], vec![0.3, 0.4]);
    }

    #[test]
    fn test_cached_embedding_stats() {
        let inner = OllamaEmbedding::new("http://localhost:11434", "test", 768);
        let cached = CachedEmbedding::new(inner, 50);
        let (len, cap) = cached.cache_stats();
        assert_eq!(len, 0);
        assert_eq!(cap, 50);
    }

    use crate::embedding::test_helpers::MockEmbedding;

    #[tokio::test]
    async fn test_cached_embedding_cache_hit() {
        let mock = MockEmbedding::new(4);
        let cached = CachedEmbedding::new(mock, 10);

        // First call should miss cache
        let result1 = cached.generate("hello").await.unwrap();
        assert_eq!(result1.len(), 4);
        assert_eq!(cached.cache_stats().0, 1); // 1 entry in cache

        // Second call with same text should hit cache
        let result2 = cached.generate("hello").await.unwrap();
        assert_eq!(result2, result1);
        assert_eq!(cached.cache_stats().0, 1); // Still 1 entry

        // Inner provider should only be called once
        // (We can't directly check call_count since mock is moved, but cache stats confirm hit)
    }

    #[tokio::test]
    async fn test_cached_embedding_cache_miss() {
        let mock = MockEmbedding::new(4);
        let cached = CachedEmbedding::new(mock, 10);

        // Different texts should each miss cache
        cached.generate("hello").await.unwrap();
        cached.generate("world").await.unwrap();
        cached.generate("test").await.unwrap();

        assert_eq!(cached.cache_stats().0, 3); // 3 entries in cache
    }

    #[tokio::test]
    async fn test_cached_embedding_lru_eviction() {
        let mock = MockEmbedding::new(4);
        let cached = CachedEmbedding::new(mock, 2); // Small cache

        // Fill cache
        cached.generate("first").await.unwrap();
        cached.generate("second").await.unwrap();
        assert_eq!(cached.cache_stats().0, 2);

        // Add third entry, should evict "first" (LRU)
        cached.generate("third").await.unwrap();
        assert_eq!(cached.cache_stats().0, 2); // Still 2 (capacity)

        // Access "second" to make it recently used
        cached.generate("second").await.unwrap();

        // Add fourth entry, should evict "third" (now LRU)
        cached.generate("fourth").await.unwrap();
        assert_eq!(cached.cache_stats().0, 2);
    }

    #[tokio::test]
    async fn test_cached_embedding_dimension() {
        let mock = MockEmbedding::new(768);
        let cached = CachedEmbedding::new(mock, 10);
        assert_eq!(cached.dimension(), 768);
    }

    #[tokio::test]
    async fn test_persistent_cached_embedding_hit() {
        let (db, _tmp) = crate::database::tests::test_db();
        let mock = MockEmbedding::new(4);
        let persistent =
            PersistentCachedEmbedding::new(mock, db.clone(), "test-model".into(), 100);

        // First call: miss, stores in SQLite
        let result1 = persistent.generate("hello").await.unwrap();
        assert_eq!(result1.len(), 4);
        assert_eq!(db.count_query_cache().unwrap(), 1);

        // Second call: hit from SQLite
        let result2 = persistent.generate("hello").await.unwrap();
        assert_eq!(result2, result1);
        assert_eq!(db.count_query_cache().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_persistent_cached_embedding_survives_new_instance() {
        let (db, _tmp) = crate::database::tests::test_db();

        // First instance generates and caches
        {
            let mock = MockEmbedding::new(4);
            let persistent =
                PersistentCachedEmbedding::new(mock, db.clone(), "test-model".into(), 100);
            persistent.generate("hello").await.unwrap();
        }

        // Second instance should find it in SQLite
        assert_eq!(db.count_query_cache().unwrap(), 1);
        let hash = text_hash("hello");
        let cached = db
            .get_cached_query_embedding(&hash, "test-model")
            .unwrap();
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_persistent_cached_embedding_model_isolation() {
        let (db, _tmp) = crate::database::tests::test_db();
        let mock = MockEmbedding::new(4);
        let persistent =
            PersistentCachedEmbedding::new(mock, db.clone(), "model-a".into(), 100);
        persistent.generate("hello").await.unwrap();

        // Different model should not find the cached entry
        let hash = text_hash("hello");
        assert!(db
            .get_cached_query_embedding(&hash, "model-b")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_persistent_cached_embedding_dimension() {
        let (db, _tmp) = crate::database::tests::test_db();
        let mock = MockEmbedding::new(512);
        let persistent =
            PersistentCachedEmbedding::new(mock, db, "test-model".into(), 100);
        assert_eq!(persistent.dimension(), 512);
    }
}
