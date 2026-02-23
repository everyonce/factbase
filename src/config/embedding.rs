//! Embedding and LLM configuration.

use serde::{Deserialize, Serialize};

/// Embedding provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Preferred field for Bedrock: AWS region (e.g. "us-east-1").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// AWS profile name (e.g. "poc"). Optional, uses default credentials if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// For Ollama: HTTP URL. For Bedrock: deprecated alias for `region`.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_dimension")]
    pub dimension: usize,
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
    #[serde(default = "default_persistent_cache_size")]
    pub persistent_cache_size: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

pub(crate) fn default_cache_size() -> usize {
    100
}

pub(crate) fn default_persistent_cache_size() -> usize {
    1000
}

pub(crate) fn default_timeout_secs() -> u64 {
    60
}

fn default_provider() -> String {
    if cfg!(feature = "bedrock") {
        "bedrock".into()
    } else {
        "ollama".into()
    }
}

fn default_base_url() -> String {
    if cfg!(feature = "bedrock") {
        "us-east-1".into()
    } else {
        "http://localhost:11434".into()
    }
}

fn default_embedding_model() -> String {
    if cfg!(feature = "bedrock") {
        "amazon.nova-2-multimodal-embeddings-v1:0".into()
    } else {
        "qwen3-embedding:0.6b".into()
    }
}

fn default_dimension() -> usize {
    1024
}

fn default_llm_model() -> String {
    if cfg!(feature = "bedrock") {
        "us.anthropic.claude-haiku-4-5-20251001-v1:0".into()
    } else {
        "rnj-1-extended".into()
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            region: None,
            profile: None,
            base_url: default_base_url(),
            model: default_embedding_model(),
            dimension: 1024,
            cache_size: default_cache_size(),
            persistent_cache_size: default_persistent_cache_size(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

impl EmbeddingConfig {
    /// Returns the effective base URL / region string.
    /// Prefers `region` if set, otherwise falls back to `base_url`.
    pub fn effective_base_url(&self) -> &str {
        self.region.as_deref().unwrap_or(&self.base_url)
    }
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Preferred field for Bedrock: AWS region (e.g. "us-east-1").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// AWS profile name (e.g. "poc"). Optional, uses default credentials if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// For Ollama: HTTP URL. For Bedrock: deprecated alias for `region`.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            region: None,
            profile: None,
            base_url: default_base_url(),
            model: default_llm_model(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

impl LlmConfig {
    /// Returns the effective base URL / region string.
    /// Prefers `region` if set, otherwise falls back to `base_url`.
    pub fn effective_base_url(&self) -> &str {
        self.region.as_deref().unwrap_or(&self.base_url)
    }
}

/// Ollama-specific configuration (retry settings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

pub(crate) fn default_max_retries() -> u32 {
    3
}

pub(crate) fn default_retry_delay_ms() -> u64 {
    1000
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_defaults() {
        let config = EmbeddingConfig::default();
        assert!(config.region.is_none());
        if cfg!(feature = "bedrock") {
            assert_eq!(config.provider, "bedrock");
            assert_eq!(config.base_url, "us-east-1");
            assert_eq!(config.effective_base_url(), "us-east-1");
            assert_eq!(config.model, "amazon.nova-2-multimodal-embeddings-v1:0");
        } else {
            assert_eq!(config.provider, "ollama");
            assert_eq!(config.base_url, "http://localhost:11434");
            assert_eq!(config.effective_base_url(), "http://localhost:11434");
            assert_eq!(config.model, "qwen3-embedding:0.6b");
        }
        assert_eq!(config.dimension, 1024);
        assert_eq!(config.cache_size, 100);
        assert_eq!(config.persistent_cache_size, 1000);
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();
        assert!(config.region.is_none());
        if cfg!(feature = "bedrock") {
            assert_eq!(config.provider, "bedrock");
            assert_eq!(config.base_url, "us-east-1");
            assert_eq!(config.effective_base_url(), "us-east-1");
            assert_eq!(config.model, "us.anthropic.claude-haiku-4-5-20251001-v1:0");
        } else {
            assert_eq!(config.provider, "ollama");
            assert_eq!(config.base_url, "http://localhost:11434");
            assert_eq!(config.effective_base_url(), "http://localhost:11434");
            assert_eq!(config.model, "rnj-1-extended");
        }
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_ollama_config_defaults() {
        let config = OllamaConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_default_helper_functions() {
        assert_eq!(default_cache_size(), 100);
        assert_eq!(default_persistent_cache_size(), 1000);
        assert_eq!(default_timeout_secs(), 60);
        assert_eq!(default_max_retries(), 3);
        assert_eq!(default_retry_delay_ms(), 1000);
    }

    #[test]
    fn test_region_overrides_base_url() {
        let config = EmbeddingConfig {
            region: Some("eu-west-1".into()),
            base_url: "us-east-1".into(),
            ..Default::default()
        };
        assert_eq!(config.effective_base_url(), "eu-west-1");

        let llm = LlmConfig {
            region: Some("ap-southeast-1".into()),
            base_url: "us-east-1".into(),
            ..Default::default()
        };
        assert_eq!(llm.effective_base_url(), "ap-southeast-1");
    }

    #[test]
    fn test_region_none_falls_back_to_base_url() {
        let config = EmbeddingConfig {
            region: None,
            base_url: "http://localhost:11434".into(),
            ..Default::default()
        };
        assert_eq!(config.effective_base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_region_deserialized_from_yaml() {
        let yaml = r#"
provider: bedrock
region: eu-west-1
model: amazon.titan-embed-text-v2:0
"#;
        let config: EmbeddingConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.region, Some("eu-west-1".into()));
        assert_eq!(config.effective_base_url(), "eu-west-1");
    }
}
