//! Review LLM wrapper for review operations.

use std::future::Future;
use std::pin::Pin;

use super::ollama::LlmProvider;
use crate::error::FactbaseError;

/// Boxed future type alias for async trait methods.
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// LLM provider for review operations (question generation, answer processing).
/// Uses review.model from config if set, otherwise falls back to llm.model.
pub struct ReviewLlm {
    inner: Box<dyn LlmProvider>,
    model_name: String,
}

impl ReviewLlm {
    /// Create a ReviewLlm wrapping any LlmProvider.
    pub fn new(inner: Box<dyn LlmProvider>, model_name: String) -> Self {
        Self { inner, model_name }
    }

    /// Get the model name being used.
    pub fn model(&self) -> &str {
        &self.model_name
    }
}

impl LlmProvider for ReviewLlm {
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
        self.inner.complete(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLlm;

    impl LlmProvider for MockLlm {
        fn complete<'a>(
            &'a self,
            _prompt: &'a str,
        ) -> BoxFuture<'a, Result<String, FactbaseError>> {
            Box::pin(async { Ok("mock".into()) })
        }
    }

    #[test]
    fn test_review_llm_model_name() {
        let review = ReviewLlm::new(Box::new(MockLlm), "test-model".into());
        assert_eq!(review.model(), "test-model");
    }
}
