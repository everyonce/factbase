//! LLM provider traits and implementations.

mod link_detector;
mod ollama;

pub use link_detector::{DetectedLink, LinkDetector};
pub use ollama::{LlmProvider, OllamaLlm};

use crate::error::FactbaseError;
use crate::BoxFuture;

/// LLM provider for review operations (question generation, answer processing).
/// Uses review.model from config if set, otherwise falls back to llm.model.
pub struct ReviewLlm {
    inner: Box<dyn LlmProvider>,
    model_name: String,
}

impl ReviewLlm {
    pub fn new(inner: Box<dyn LlmProvider>, model_name: String) -> Self {
        Self { inner, model_name }
    }

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
pub(crate) mod test_helpers {
    use super::ollama::LlmProvider;
    use crate::error::FactbaseError;
    use crate::BoxFuture;

    /// Configurable mock LLM that returns a fixed response string.
    pub struct MockLlm {
        response: String,
    }

    impl MockLlm {
        pub fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
            }
        }
    }

    impl Default for MockLlm {
        fn default() -> Self {
            Self::new("[]")
        }
    }

    impl LlmProvider for MockLlm {
        fn complete<'a>(
            &'a self,
            _prompt: &'a str,
        ) -> BoxFuture<'a, Result<String, FactbaseError>> {
            Box::pin(async move { Ok(self.response.clone()) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::MockLlm;

    #[test]
    fn test_review_llm_model_name() {
        let review = ReviewLlm::new(Box::new(MockLlm::new("mock")), "test-model".into());
        assert_eq!(review.model(), "test-model");
    }
}
