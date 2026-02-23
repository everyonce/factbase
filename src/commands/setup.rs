//! Database and service setup functions for CLI commands.
//!
//! # Setup Function Guide
//!
//! | Function | Returns | Use When |
//! |----------|---------|----------|
//! | `setup_database()` | `(Config, Database)` | Need global database AND config later |
//! | `setup_database_only()` | `Database` | Need global database, don't need config |
//! | `setup_database_checked()` | `(Config, Database, PathBuf)` | Same as above, but fail if DB doesn't exist |
//! | `find_repo()` | `(Database, Repository)` | Need local repo, don't need config later |
//! | `find_repo_with_config()` | `(Config, Database, Repository)` | Need local repo AND config (e.g., for embedding/LLM) |
//!
//! # Examples
//!
//! ```ignore
//! // Global database operations (repo list, stats)
//! let (config, db) = setup_database()?;
//!
//! // Local repo operations without Ollama
//! let (db, repo) = find_repo(args.repo.as_deref())?;
//!
//! // Local repo operations with Ollama (search, scan)
//! let (config, db, repo) = find_repo_with_config(args.repo.as_deref())?;
//! let embedding = setup_embedding_with_timeout(&config, args.timeout);
//! ```

use super::errors::db_not_found_error;
use factbase::{
    CachedEmbedding, Config, Database, EmbeddingProvider, LinkDetector, LlmProvider,
    OllamaEmbedding, OllamaLlm, Repository, ReviewLlm,
};
use std::path::{Path, PathBuf};

/// Print a one-time notice when no config file exists
fn print_first_run_notice() {
    if !Config::config_file_exists() {
        eprintln!(
            "Note: No config found at {}. Using defaults ({} provider).",
            Config::default_path().display(),
            Config::default().embedding.provider,
        );
        eprintln!("  Run `factbase doctor` to verify connectivity.\n");
    }
}

/// Load config and open database - common setup for most commands
pub fn setup_database() -> anyhow::Result<(Config, Database)> {
    print_first_run_notice();
    let config = Config::load(None)?;
    let db_path = Path::new(&config.database.path);
    let db = config.open_database(db_path)?;
    Ok((config, db))
}

/// Load config and open database, returning only the database.
///
/// Use this when you don't need the config after setup (most commands).
pub fn setup_database_only() -> anyhow::Result<Database> {
    let (_config, db) = setup_database()?;
    Ok(db)
}

/// Load config and open database with explicit path existence check.
/// Returns a helpful error if the database file doesn't exist.
/// Also returns the expanded database path for commands that need it.
pub fn setup_database_checked() -> anyhow::Result<(Config, Database, PathBuf)> {
    let config = Config::load(None)?;
    let db_path = PathBuf::from(shellexpand::tilde(&config.database.path).to_string());

    if !db_path.exists() {
        return Err(db_not_found_error(&db_path));
    }

    let db = config.open_database(&db_path)?;
    Ok((config, db, db_path))
}

/// Find repository by ID or from current directory, returning config for callers that need it.
///
/// Use this when you need both the repository and config (e.g., for embedding/LLM setup).
/// Use `find_repo()` when you only need the database and repository.
pub fn find_repo_with_config(
    repo_id: Option<&str>,
) -> anyhow::Result<(Config, Database, Repository)> {
    print_first_run_notice();
    let mut dir = std::env::current_dir()?;
    let config = Config::load(None)?;

    loop {
        let factbase_dir = dir.join(".factbase");
        if factbase_dir.exists() {
            let db_path = factbase_dir.join("factbase.db");
            let db = config.open_database(&db_path)?;
            let repos = db.list_repositories()?;
            let repo = if let Some(id) = repo_id {
                repos.into_iter().find(|r| r.id == id)
            } else {
                repos.into_iter().next()
            };
            if let Some(r) = repo {
                return Ok((config, db, r));
            }
            anyhow::bail!("No repository found");
        }
        if !dir.pop() {
            break;
        }
    }
    anyhow::bail!("Not in a factbase repository. Run `factbase init <path>` first.")
}

/// Find repository by ID or from current directory.
///
/// Use this when you only need the database and repository.
/// Use `find_repo_with_config()` when you also need config for embedding/LLM setup.
pub fn find_repo(repo_id: Option<&str>) -> anyhow::Result<(Database, Repository)> {
    let (_, db, repo) = find_repo_with_config(repo_id)?;
    Ok((db, repo))
}

/// Resolve a Bedrock region from a base_url config value.
///
/// Returns `None` if the value is empty or an HTTP URL (Ollama),
/// `Some(region)` if it looks like an AWS region string.
#[cfg(feature = "bedrock")]
fn resolve_bedrock_region(base_url: &str) -> Option<&str> {
    if base_url.is_empty() || base_url.starts_with("http") {
        None
    } else {
        Some(base_url)
    }
}

/// Create embedding provider from config with optional timeout override
pub async fn setup_embedding_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> Box<dyn EmbeddingProvider> {
    match config.embedding.provider.as_str() {
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            let region = resolve_bedrock_region(config.embedding.effective_base_url());
            Box::new(
                factbase::bedrock::BedrockEmbedding::new(
                    &config.embedding.model,
                    config.embedding.dimension,
                    region,
                )
                .await,
            )
        }
        _ => {
            let timeout = timeout_override.unwrap_or(config.embedding.timeout_secs);
            Box::new(OllamaEmbedding::with_config(
                config.embedding.effective_base_url(),
                &config.embedding.model,
                config.embedding.dimension,
                timeout,
                config.ollama.max_retries,
                config.ollama.retry_delay_ms,
            ))
        }
    }
}

/// Create embedding provider from config
#[cfg(feature = "mcp")]
pub async fn setup_embedding(config: &Config) -> Box<dyn EmbeddingProvider> {
    setup_embedding_with_timeout(config, None).await
}

/// Create embedding provider wrapped in LRU cache.
pub async fn setup_cached_embedding(
    config: &Config,
    timeout_override: Option<u64>,
) -> CachedEmbedding<Box<dyn EmbeddingProvider>> {
    let embedding = setup_embedding_with_timeout(config, timeout_override).await;
    CachedEmbedding::new(embedding, config.embedding.cache_size)
}

/// Create an LLM provider for the given model name using config settings.
async fn create_llm(
    config: &Config,
    model: &str,
    timeout_override: Option<u64>,
) -> Box<dyn LlmProvider> {
    match config.llm.provider.as_str() {
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            let region = resolve_bedrock_region(config.llm.effective_base_url());
            Box::new(factbase::bedrock::BedrockLlm::new(model, region).await)
        }
        _ => {
            let timeout = timeout_override.unwrap_or(config.llm.timeout_secs);
            Box::new(OllamaLlm::with_config(
                config.llm.effective_base_url(),
                model,
                timeout,
                config.ollama.max_retries,
                config.ollama.retry_delay_ms,
            ))
        }
    }
}

/// Create LLM provider from config with optional timeout override
pub async fn setup_llm_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> Box<dyn LlmProvider> {
    create_llm(config, &config.llm.model, timeout_override).await
}

/// Create LinkDetector from config with optional timeout override
pub async fn setup_link_detector_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> LinkDetector {
    let llm = setup_llm_with_timeout(config, timeout_override).await;
    LinkDetector::new(llm)
}

/// Create LinkDetector from config
#[cfg(feature = "mcp")]
pub async fn setup_link_detector(config: &Config) -> LinkDetector {
    setup_link_detector_with_timeout(config, None).await
}

/// Create embedding provider and LinkDetector from config with optional timeout override
pub async fn setup_services_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> (Box<dyn EmbeddingProvider>, LinkDetector) {
    let embedding = setup_embedding_with_timeout(config, timeout_override).await;
    let link_detector = setup_link_detector_with_timeout(config, timeout_override).await;
    (embedding, link_detector)
}

/// Create ReviewLlm service from config with optional timeout override
pub async fn setup_review_llm_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> ReviewLlm {
    let model_name = config.review_model().to_string();
    let llm = create_llm(config, &model_name, timeout_override).await;
    ReviewLlm::new(llm, model_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use factbase::config::{EmbeddingConfig, LlmConfig, OllamaConfig};
    use factbase::EmbeddingProvider;

    fn test_config() -> Config {
        Config {
            embedding: EmbeddingConfig {
                base_url: "http://localhost:11434".to_string(),
                model: "test-embed".to_string(),
                dimension: 1024,
                timeout_secs: 30,
                cache_size: 100,
                ..Default::default()
            },
            llm: LlmConfig {
                base_url: "http://localhost:11434".to_string(),
                model: "test-llm".to_string(),
                timeout_secs: 60,
                ..Default::default()
            },
            ollama: OllamaConfig {
                max_retries: 3,
                retry_delay_ms: 1000,
            },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_setup_embedding_with_timeout_uses_config_default() {
        let config = test_config();
        let embedding = setup_embedding_with_timeout(&config, None).await;
        assert_eq!(embedding.dimension(), 1024);
    }

    #[tokio::test]
    async fn test_setup_embedding_with_timeout_override() {
        let config = test_config();
        let embedding = setup_embedding_with_timeout(&config, Some(120)).await;
        assert_eq!(embedding.dimension(), 1024);
    }

    #[tokio::test]
    async fn test_setup_llm_with_timeout_uses_config_default() {
        let config = test_config();
        let _llm = setup_llm_with_timeout(&config, None).await;
    }

    #[tokio::test]
    async fn test_setup_llm_with_timeout_override() {
        let config = test_config();
        let _llm = setup_llm_with_timeout(&config, Some(180)).await;
    }

    #[tokio::test]
    async fn test_setup_link_detector_with_timeout() {
        let config = test_config();
        let _detector = setup_link_detector_with_timeout(&config, Some(90)).await;
    }

    #[tokio::test]
    async fn test_setup_services_with_timeout() {
        let config = test_config();
        let (embedding, _detector) = setup_services_with_timeout(&config, Some(45)).await;
        assert_eq!(embedding.dimension(), 1024);
    }

    #[tokio::test]
    async fn test_setup_review_llm_with_timeout() {
        let config = test_config();
        let _review_llm = setup_review_llm_with_timeout(&config, Some(120)).await;
    }
}
