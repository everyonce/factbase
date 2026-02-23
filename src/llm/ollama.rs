//! Ollama LLM implementation.

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

use crate::error::FactbaseError;
use crate::ollama::OllamaClient;

/// Boxed future type alias for async trait methods.
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for LLM text completion providers.
pub trait LlmProvider: Send + Sync {
    /// Generate a completion for the given prompt.
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>>;
}

impl LlmProvider for Box<dyn LlmProvider> {
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
        (**self).complete(prompt)
    }
}

/// Ollama-based LLM provider.
pub struct OllamaLlm {
    pub(crate) client: OllamaClient,
    pub(crate) model: String,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl OllamaLlm {
    /// Create a new OllamaLlm with default timeout (30s).
    pub fn new(base_url: &str, model: &str) -> Self {
        Self::with_timeout(base_url, model, 30)
    }

    /// Create a new OllamaLlm with custom timeout.
    pub fn with_timeout(base_url: &str, model: &str, timeout_secs: u64) -> Self {
        Self::with_config(base_url, model, timeout_secs, 3, 1000)
    }

    /// Create a new OllamaLlm with full configuration.
    pub fn with_config(
        base_url: &str,
        model: &str,
        timeout_secs: u64,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> Self {
        Self {
            client: OllamaClient::with_config(base_url, timeout_secs, max_retries, retry_delay_ms),
            model: model.to_string(),
        }
    }
}

impl LlmProvider for OllamaLlm {
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
        Box::pin(async move {
            let req = GenerateRequest {
                model: &self.model,
                prompt,
                stream: false,
            };

            let body: GenerateResponse =
                self.client.post("/api/generate", &req, &self.model).await?;
            Ok(body.response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_llm_new() {
        let llm = OllamaLlm::new("http://localhost:11434", "llama3");
        assert_eq!(llm.client.base_url(), "http://localhost:11434");
        assert_eq!(llm.model, "llama3");
    }
}
