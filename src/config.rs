use crate::error::FactbaseError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub repositories: Vec<RepositoryConfig>,
    pub watcher: WatcherConfig,
    pub processor: ProcessorConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorConfig {
    pub max_file_size: usize,
    pub snippet_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub dimension: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model: "nomic-embed-text".into(),
            dimension: 768,
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model: "rnj-1".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                path: shellexpand::tilde("~/.config/factbase/factbase.db").to_string(),
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
            },
            embedding: EmbeddingConfig::default(),
            llm: LlmConfig::default(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, FactbaseError> {
        let config_path = path.map(PathBuf::from).unwrap_or_else(|| {
            let home = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join("factbase").join("config.yaml")
        });

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
