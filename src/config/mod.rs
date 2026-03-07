//! Configuration management for Factbase.
//!
//! This module provides configuration loading, validation, and defaults.

mod database;
mod embedding;
mod processor;
pub mod prompts;
mod server;
mod validation;
mod web;
pub mod workflows;
pub mod cross_validate;

pub use database::DatabaseConfig;
pub use embedding::{EmbeddingConfig, LlmConfig, OllamaConfig};
pub use processor::{ProcessorConfig, RepositoryConfig, WatcherConfig};
pub use prompts::PromptsConfig;
pub use server::{RateLimitConfig, ReviewConfig, ServerConfig, TemporalConfig};
pub use validation::{validate_timeout, TIMEOUT_RANGE};
pub use web::WebConfig;
pub use workflows::WorkflowsConfig;
pub use cross_validate::CrossValidateConfig;

use crate::database::Database;
use crate::error::FactbaseError;
use database::{default_compression, default_pool_size};
use processor::{
    default_chunk_overlap, default_chunk_size, default_embedding_batch_size,
    default_link_batch_size, default_check_concurrency, default_metadata_cache_size,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use validation::{require_non_empty, require_positive, require_range};

/// Main configuration struct containing all settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub watcher: WatcherConfig,
    #[serde(default)]
    pub processor: ProcessorConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub temporal: TemporalConfig,
    #[serde(default)]
    pub review: ReviewConfig,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
    #[serde(default)]
    pub workflows: WorkflowsConfig,
    #[serde(default)]
    pub cross_validate: CrossValidateConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                path: shellexpand::tilde("~/.config/factbase/factbase.db").to_string(),
                pool_size: default_pool_size(),
                compression: default_compression(),
            },
            repositories: vec![],
            watcher: WatcherConfig {
                debounce_ms: 500,
                ignore_patterns: vec![
                    "*.swp".into(),
                    "*.tmp".into(),
                    "*~".into(),
                    ".git/**".into(),
                    ".DS_Store".into(),
                    ".factbase/**".into(),
                ],
            },
            processor: ProcessorConfig {
                max_file_size: 100000,
                snippet_length: 200,
                chunk_size: default_chunk_size(),
                chunk_overlap: default_chunk_overlap(),
                embedding_batch_size: default_embedding_batch_size(),
                link_batch_size: default_link_batch_size(),
                check_concurrency: default_check_concurrency(),
                metadata_cache_size: default_metadata_cache_size(),
            },
            embedding: EmbeddingConfig::default(),
            llm: LlmConfig::default(),
            ollama: OllamaConfig::default(),
            rate_limit: RateLimitConfig::default(),
            server: ServerConfig::default(),
            temporal: TemporalConfig::default(),
            review: ReviewConfig::default(),
            web: WebConfig::default(),
            prompts: PromptsConfig::default(),
            workflows: WorkflowsConfig::default(),
            cross_validate: CrossValidateConfig::default(),
        }
    }
}

impl Config {
    /// Get the default config file path
    pub fn default_path() -> PathBuf {
        let home = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join("factbase").join("config.yaml")
    }

    /// Check if the default config file exists
    pub fn config_file_exists() -> bool {
        Self::default_path().exists()
    }

    pub fn load(path: Option<&Path>) -> Result<Self, FactbaseError> {
        let config_path = path.map_or_else(
            || {
                // Check for local .factbase/config.yaml first, then global
                let local = PathBuf::from(".factbase/config.yaml");
                if local.exists() {
                    local
                } else {
                    Self::default_path()
                }
            },
            PathBuf::from,
        );

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&config_path)?;
        let config: Config = serde_yaml_ng::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Get the effective review LLM model (deprecated — LLM no longer used server-side)
    pub fn review_model(&self) -> &str {
        self.review.model.as_deref().unwrap_or(&self.llm.model)
    }

    /// Open a database connection using this config's pool size and compression settings.
    ///
    /// Also initializes the document metadata cache.
    pub fn open_database(&self, path: &Path) -> Result<Database, FactbaseError> {
        let db = Database::with_options(
            path,
            self.database.pool_size,
            self.database.is_compression_enabled(),
        )?;
        crate::cache::init_document_cache(self.processor.metadata_cache_size);
        Ok(db)
    }

    pub fn validate(&self) -> Result<(), FactbaseError> {
        // Database settings
        require_range(self.database.pool_size, 1, 32, "database.pool_size")?;
        if self.database.compression != "none" && self.database.compression != "zstd" {
            return Err(FactbaseError::config(
                "database.compression must be 'none' or 'zstd'",
            ));
        }

        // Rate limits
        require_positive(self.rate_limit.per_second, "rate_limit.per_second")?;
        require_positive(self.rate_limit.burst_size as u64, "rate_limit.burst_size")?;

        // Embedding settings
        require_positive(self.embedding.dimension as u64, "embedding.dimension")?;
        require_range(self.embedding.cache_size, 1, 10000, "embedding.cache_size")?;
        require_range(
            self.embedding.persistent_cache_size,
            0,
            100000,
            "embedding.persistent_cache_size",
        )?;

        // Processor settings
        require_positive(
            self.processor.max_file_size as u64,
            "processor.max_file_size",
        )?;
        require_positive(
            self.processor.snippet_length as u64,
            "processor.snippet_length",
        )?;
        require_positive(self.processor.chunk_size as u64, "processor.chunk_size")?;
        if self.processor.chunk_overlap >= self.processor.chunk_size {
            return Err(FactbaseError::config(
                "processor.chunk_overlap must be less than chunk_size",
            ));
        }
        require_range(
            self.processor.metadata_cache_size,
            1,
            10000,
            "processor.metadata_cache_size",
        )?;

        // Server settings
        require_positive(self.server.port as u64, "server.port")?;
        require_non_empty(&self.server.host, "server.host")?;
        require_range(
            self.server.time_budget_secs,
            5,
            600,
            "server.time_budget_secs",
        )?;

        // Timeout settings
        if let Err(e) = validate_timeout(self.embedding.timeout_secs) {
            return Err(FactbaseError::config(
                e.to_string().replace("--timeout", "embedding.timeout_secs"),
            ));
        }

        // Temporal settings
        require_range(
            self.temporal.min_coverage,
            0.0,
            1.0,
            "temporal.min_coverage",
        )?;

        // Ollama settings
        require_range(self.ollama.max_retries, 0, 10, "ollama.max_retries")?;
        require_range(
            self.ollama.retry_delay_ms,
            100,
            60000,
            "ollama.retry_delay_ms",
        )?;

        // Web settings
        require_positive(self.web.port as u64, "web.port")?;

        // Prompt templates
        prompts::validate_prompts(&self.prompts);

        // Workflow text overrides
        workflows::validate_workflows(&self.workflows);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config::default()
    }

    #[test]
    fn test_validate_default_config() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn test_validate_database_fields() {
        let mut c = valid_config();
        c.database.pool_size = 0;
        assert!(c.validate().unwrap_err().to_string().contains("pool_size"));
        c.database.pool_size = 33;
        assert!(c.validate().unwrap_err().to_string().contains("pool_size"));

        let mut c = valid_config();
        c.database.compression = "invalid".to_string();
        assert!(c.validate().unwrap_err().to_string().contains("compression"));
        c.database.compression = "none".to_string();
        assert!(c.validate().is_ok());
        c.database.compression = "zstd".to_string();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_validate_rate_limit_fields() {
        let mut c = valid_config();
        c.rate_limit.per_second = 0;
        assert!(c.validate().unwrap_err().to_string().contains("per_second"));
        let mut c = valid_config();
        c.rate_limit.burst_size = 0;
        assert!(c.validate().unwrap_err().to_string().contains("burst_size"));
    }

    #[test]
    fn test_validate_embedding_fields() {
        let mut c = valid_config();
        c.embedding.dimension = 0;
        assert!(c.validate().unwrap_err().to_string().contains("dimension"));
        let mut c = valid_config();
        c.embedding.cache_size = 0;
        assert!(c.validate().unwrap_err().to_string().contains("cache_size"));
        c.embedding.cache_size = 10001;
        assert!(c.validate().unwrap_err().to_string().contains("cache_size"));
        let mut c = valid_config();
        c.embedding.timeout_secs = 0;
        assert!(c.validate().unwrap_err().to_string().contains("embedding.timeout_secs"));
    }

    #[test]
    fn test_validate_llm_timeout() {
        // LLM config is kept for backward compatibility but no longer validated
        let c = valid_config();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_validate_temporal_min_coverage() {
        let mut c = valid_config();
        for valid in [0.0, 0.5, 1.0] {
            c.temporal.min_coverage = valid;
            assert!(c.validate().is_ok());
        }
        c.temporal.min_coverage = -0.1;
        assert!(c.validate().unwrap_err().to_string().contains("min_coverage"));
        c.temporal.min_coverage = 1.1;
        assert!(c.validate().unwrap_err().to_string().contains("min_coverage"));
    }

    #[test]
    fn test_validate_ollama_fields() {
        let mut c = valid_config();
        // max_retries: 0 and 10 valid, 11 invalid
        c.ollama.max_retries = 0;
        assert!(c.validate().is_ok());
        c.ollama.max_retries = 10;
        assert!(c.validate().is_ok());
        c.ollama.max_retries = 11;
        assert!(c.validate().unwrap_err().to_string().contains("max_retries"));
        c.ollama.max_retries = 3; // reset
        // retry_delay_ms: 100 and 60000 valid, 99 and 60001 invalid
        c.ollama.retry_delay_ms = 100;
        assert!(c.validate().is_ok());
        c.ollama.retry_delay_ms = 60000;
        assert!(c.validate().is_ok());
        c.ollama.retry_delay_ms = 99;
        assert!(c.validate().unwrap_err().to_string().contains("retry_delay_ms"));
        c.ollama.retry_delay_ms = 60001;
        assert!(c.validate().unwrap_err().to_string().contains("retry_delay_ms"));
    }

    #[test]
    fn test_validate_web_port() {
        let mut c = valid_config();
        c.web.port = 0;
        assert!(c.validate().unwrap_err().to_string().contains("web.port"));
        c.web.port = 8080;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_validate_time_budget_secs() {
        let mut c = valid_config();
        c.server.time_budget_secs = 4;
        assert!(c.validate().unwrap_err().to_string().contains("time_budget_secs"));
        c.server.time_budget_secs = 601;
        assert!(c.validate().unwrap_err().to_string().contains("time_budget_secs"));
        c.server.time_budget_secs = 5;
        assert!(c.validate().is_ok());
        c.server.time_budget_secs = 600;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_review_model() {
        let config = valid_config();
        assert_eq!(config.review_model(), config.llm.model);
        let mut config = valid_config();
        config.review.model = Some("custom-review-model".to_string());
        assert_eq!(config.review_model(), "custom-review-model");
    }

    #[test]
    fn test_config_defaults() {
        let review = ReviewConfig::default();
        assert!(review.model.is_none());
        let ollama = OllamaConfig::default();
        assert_eq!(ollama.max_retries, 3);
        assert_eq!(ollama.retry_delay_ms, 1000);
        let web = WebConfig::default();
        assert!(!web.enabled);
        assert_eq!(web.port, 3001);
    }
}
