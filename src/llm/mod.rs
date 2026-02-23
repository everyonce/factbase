//! LLM provider traits and implementations.
//!
//! This module provides the LLM abstraction layer for factbase:
//!
//! - `LlmProvider` trait for async text completion
//! - `OllamaLlm` implementation using Ollama API
//! - `ReviewLlm` wrapper for review operations
//! - `LinkDetector` service for entity detection
//!
//! # Module Organization
//!
//! - `ollama` - OllamaLlm implementation
//! - `review` - ReviewLlm wrapper
//! - `link_detector` - LinkDetector service and DetectedLink struct

mod link_detector;
mod ollama;
mod review;

pub use link_detector::{DetectedLink, LinkDetector};
pub use ollama::{LlmProvider, OllamaLlm};
pub use review::ReviewLlm;

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
