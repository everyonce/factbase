//! Local CPU embedding provider using fastembed (ONNX runtime).
//!
//! Provides zero-config embedding generation using BGE-small-en-v1.5 (384 dimensions).
//! Model is auto-downloaded on first use and cached locally.

use crate::error::FactbaseError;
use crate::BoxFuture;
use crate::EmbeddingProvider;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Local embedding dimension (BGE-small-en-v1.5).
pub const LOCAL_EMBEDDING_DIM: usize = 384;

/// Local embedding model name for metadata tracking.
pub const LOCAL_EMBEDDING_MODEL: &str = "BAAI/bge-small-en-v1.5";

/// Local CPU embedding provider using fastembed with BGE-small-en-v1.5.
pub struct LocalEmbeddingProvider {
    model: Arc<Mutex<TextEmbedding>>,
}

impl LocalEmbeddingProvider {
    /// Create a new local embedding provider.
    ///
    /// Downloads the model on first use (~33MB) and caches it.
    /// Set `show_progress` to display download progress.
    pub fn new(show_progress: bool) -> Result<Self, FactbaseError> {
        Self::with_cache_dir(None, show_progress)
    }

    /// Create with a custom cache directory for the model files.
    pub fn with_cache_dir(
        cache_dir: Option<PathBuf>,
        show_progress: bool,
    ) -> Result<Self, FactbaseError> {
        let mut opts = InitOptions::new(EmbeddingModel::BGESmallENV15)
            .with_show_download_progress(show_progress);
        if let Some(dir) = cache_dir {
            opts = opts.with_cache_dir(dir);
        }
        let model = TextEmbedding::try_new(opts)
            .map_err(|e| FactbaseError::embedding(format!("Failed to load local model: {e}")))?;
        Ok(Self {
            model: Arc::new(Mutex::new(model)),
        })
    }
}

impl EmbeddingProvider for LocalEmbeddingProvider {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        let model = self.model.clone();
        let text = text.to_string();
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut m = model.lock().map_err(|e| {
                    FactbaseError::embedding(format!("Model lock poisoned: {e}"))
                })?;
                m.embed(vec![text.as_str()], None)
                    .map_err(|e| FactbaseError::embedding(format!("Local embedding error: {e}")))
            })
            .await
            .map_err(|e| FactbaseError::embedding(format!("Embedding task failed: {e}")))??;

            result
                .into_iter()
                .next()
                .ok_or_else(|| FactbaseError::embedding("No embedding returned"))
        })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        let model = self.model.clone();
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        Box::pin(async move {
            if texts.is_empty() {
                return Ok(vec![]);
            }
            tokio::task::spawn_blocking(move || {
                let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                let mut m = model.lock().map_err(|e| {
                    FactbaseError::embedding(format!("Model lock poisoned: {e}"))
                })?;
                m.embed(refs, None)
                    .map_err(|e| FactbaseError::embedding(format!("Local batch embedding error: {e}")))
            })
            .await
            .map_err(|e| FactbaseError::embedding(format!("Batch embedding task failed: {e}")))?
        })
    }

    fn dimension(&self) -> usize {
        LOCAL_EMBEDDING_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_embedding_constants() {
        assert_eq!(LOCAL_EMBEDDING_DIM, 384);
        assert_eq!(LOCAL_EMBEDDING_MODEL, "BAAI/bge-small-en-v1.5");
    }

    #[tokio::test]
    async fn test_local_embedding_provider() {
        let provider = match LocalEmbeddingProvider::new(false) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping local embedding test (model not available): {e}");
                return;
            }
        };

        assert_eq!(provider.dimension(), 384);

        let embedding = provider.generate("hello world").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Verify non-zero
        assert!(embedding.iter().any(|&v| v != 0.0));
    }

    #[tokio::test]
    async fn test_local_embedding_batch() {
        let provider = match LocalEmbeddingProvider::new(false) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping local embedding batch test: {e}");
                return;
            }
        };

        let texts = &["hello world", "goodbye world"];
        let embeddings = provider.generate_batch(texts).await.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);
    }

    #[tokio::test]
    async fn test_local_embedding_empty_batch() {
        let provider = match LocalEmbeddingProvider::new(false) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping local embedding empty batch test: {e}");
                return;
            }
        };

        let texts: &[&str] = &[];
        let embeddings = provider.generate_batch(texts).await.unwrap();
        assert!(embeddings.is_empty());
    }
}
