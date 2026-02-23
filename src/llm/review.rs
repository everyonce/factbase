//! Review LLM wrapper for review operations.

use super::ollama::LlmProvider;
use crate::error::FactbaseError;
use crate::BoxFuture;

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
    use crate::llm::test_helpers::MockLlm;

    #[test]
    fn test_review_llm_model_name() {
        let review = ReviewLlm::new(Box::new(MockLlm::new("mock")), "test-model".into());
        assert_eq!(review.model(), "test-model");
    }
}
