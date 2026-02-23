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
