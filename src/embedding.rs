use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::FactbaseError;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn generate(&self, text: &str) -> Result<Vec<f32>, FactbaseError>;
    fn dimension(&self) -> usize;
}

#[derive(Clone)]
pub struct OllamaEmbedding {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dim: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f64>,
}

impl OllamaEmbedding {
    pub fn new(base_url: &str, model: &str, dimension: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            dim: dimension,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn generate(&self, text: &str) -> Result<Vec<f32>, FactbaseError> {
        let url = format!("{}/api/embeddings", self.base_url);
        let req = EmbeddingRequest {
            model: &self.model,
            prompt: text,
        };

        let resp = match self.client.post(&url).json(&req).send().await {
            Ok(r) => r,
            Err(_) => {
                eprintln!("Error: Failed to connect to Ollama at {}", self.base_url);
                eprintln!("Ensure Ollama is running: ollama serve");
                std::process::exit(1);
            }
        };

        if !resp.status().is_success() {
            eprintln!("Error: Ollama returned status {}", resp.status());
            eprintln!(
                "Ensure model '{}' is available: ollama pull {}",
                self.model, self.model
            );
            std::process::exit(1);
        }

        let body: EmbeddingResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error: Failed to parse Ollama response: {}", e);
                std::process::exit(1);
            }
        };

        let embedding: Vec<f32> = body.embedding.into_iter().map(|v| v as f32).collect();

        if embedding.len() != self.dim {
            eprintln!(
                "Error: Expected {} dimensions, got {}",
                self.dim,
                embedding.len()
            );
            std::process::exit(1);
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_embedding_new() {
        let emb = OllamaEmbedding::new("http://localhost:11434", "nomic-embed-text", 768);
        assert_eq!(emb.dimension(), 768);
        assert_eq!(emb.base_url, "http://localhost:11434");
        assert_eq!(emb.model, "nomic-embed-text");
    }

    #[test]
    fn test_dimension_returns_configured_value() {
        let emb = OllamaEmbedding::new("http://test", "model", 512);
        assert_eq!(emb.dimension(), 512);
    }
}
