//! Database and service setup for CLI commands.
//!
//! # Builder Pattern
//!
//! Use `Setup::new()` to configure what you need, then `.build()` to get a `SetupContext`:
//!
//! ```ignore
//! // Config + database from .factbase/ in cwd
//! let ctx = Setup::new().build()?;
//!
//! // Require database file exists (error if missing)
//! let ctx = Setup::new().check_exists().build()?;
//!
//! // Require a repository (from .factbase/ in cwd)
//! let ctx = Setup::new().require_repo(None).build()?;
//! // ctx.config, ctx.db, ctx.repo() all available
//! ```

use super::errors::db_not_found_error;
use factbase::config::Config;
use factbase::database::Database;
use factbase::embedding::{
    CachedEmbedding,
    EmbeddingProvider,
    OllamaEmbedding,
    PersistentCachedEmbedding,
};
use factbase::models::Repository;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for CLI command setup. Configures what resources are needed,
/// then `.build()` loads/validates everything in one shot.
pub struct Setup {
    check_exists: bool,
    require_repo: bool,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            check_exists: false,
            require_repo: false,
        }
    }

    /// Fail at build time if the database file doesn't exist on disk.
    pub fn check_exists(mut self) -> Self {
        self.check_exists = true;
        self
    }

    /// Resolve the single repository from `.factbase/` in the current directory.
    /// The `_repo_id` parameter is accepted for backward compatibility but ignored
    /// (single-KB-per-directory model).
    pub fn require_repo(mut self, _repo_id: Option<&str>) -> Self {
        self.require_repo = true;
        self
    }

    pub fn build(self) -> anyhow::Result<SetupContext> {
        let config = Config::load(None)?;
        let dir = std::env::current_dir()?;
        let factbase_dir = dir.join(".factbase");

        if !factbase_dir.exists() && (self.require_repo || self.check_exists) {
            anyhow::bail!(
                "No .factbase/ directory found in {}.\n\
                 Use the MCP 'create' workflow to set up a new knowledge base,\n\
                 or run `factbase scan` from a directory containing markdown files.",
                dir.display()
            );
        }

        let db_path = factbase_dir.join("factbase.db");

        if self.check_exists && !db_path.exists() {
            return Err(db_not_found_error(&db_path));
        }

        let db = config.open_database(&db_path)?;

        let repo = if self.require_repo {
            let repos = db.list_repositories()?;
            let r = repos.into_iter().next().ok_or_else(|| {
                anyhow::anyhow!("No repository found in {}", factbase_dir.display())
            })?;
            Some(r)
        } else {
            None
        };

        Ok(SetupContext {
            config,
            db,
            db_path,
            repo,
        })
    }
}

// ---------------------------------------------------------------------------
// SetupContext
// ---------------------------------------------------------------------------

/// Result of `Setup::build()`. Contains config, database, and optionally resolved repository.
pub struct SetupContext {
    pub config: Config,
    pub db: Database,
    pub db_path: PathBuf,
    pub repo: Option<Repository>,
}

impl SetupContext {
    /// Take ownership of the resolved single repository.
    pub fn take_repo(self) -> (Config, Database, Repository) {
        let repo = self.repo.expect("require_repo() was not called on Setup builder");
        (self.config, self.db, repo)
    }
}

// ---------------------------------------------------------------------------
// Auto-init (for serve/mcp)
// ---------------------------------------------------------------------------

/// Auto-initialize a factbase repository in the given directory.
///
/// Creates `.factbase/` dir, `perspective.yaml`, database, and registers the repo.
pub fn auto_init_repo(dir: &std::path::Path) -> anyhow::Result<(Config, Database, Repository)> {
    let factbase_dir = dir.join(".factbase");
    std::fs::create_dir_all(&factbase_dir)?;
    let perspective_path = dir.join("perspective.yaml");
    if !perspective_path.exists() {
        std::fs::write(&perspective_path, factbase::models::PERSPECTIVE_TEMPLATE)?;
    }
    let config = Config::load(None)?;
    let db_path = factbase_dir.join("factbase.db");
    let db = config.open_database(&db_path)?;
    let name = dir
        .file_name()
        .map_or_else(|| factbase::DEFAULT_REPO_ID.into(), |s| s.to_string_lossy().to_string());
    let repo = super::create_repository(factbase::DEFAULT_REPO_ID, &name, dir);
    db.upsert_repository(&repo)?;
    tracing::info!("Auto-initialized factbase at {}", dir.display());
    Ok((config, db, repo))
}

// ---------------------------------------------------------------------------
// Embedding setup
// ---------------------------------------------------------------------------

/// Resolve a Bedrock region from a base_url config value.
#[cfg(feature = "bedrock")]
fn resolve_bedrock_region(base_url: &str) -> Option<&str> {
    if base_url.is_empty() || base_url.starts_with("http") {
        None
    } else {
        Some(base_url)
    }
}

/// Validate that a base_url looks like an HTTP URL for Ollama.
fn validate_ollama_url(base_url: &str, section: &str, provider: &str) {
    if base_url.starts_with("http://") || base_url.starts_with("https://") {
        return;
    }
    eprintln!(
        "error: {section}.base_url is '{}' which is not a valid URL for provider '{}'.",
        base_url, provider
    );
    if base_url.contains('-') && base_url.chars().all(|c| c.is_alphanumeric() || c == '-') {
        eprintln!("       This looks like an AWS region. Did you mean to use provider 'bedrock'?");
        eprintln!("hint: Set {section}.provider = 'bedrock' in config, or change {section}.base_url to an Ollama URL (e.g., http://localhost:11434).");
    } else {
        eprintln!("hint: Set {section}.base_url to an Ollama URL (e.g., http://localhost:11434).");
    }
    std::process::exit(1);
}

/// Create embedding provider from config with optional timeout override
pub async fn setup_embedding_with_timeout(
    config: &Config,
    timeout_override: Option<u64>,
) -> Box<dyn EmbeddingProvider> {
    match config.embedding.provider.as_str() {
        #[cfg(feature = "local-embedding")]
        "local" => {
            eprintln!("Using local CPU embeddings (BGE-small-en-v1.5, 384-dim)");
            match factbase::local_embedding::LocalEmbeddingProvider::new(true) {
                Ok(provider) => Box::new(provider),
                Err(e) => {
                    eprintln!("error: Failed to initialize local embedding provider: {e}");
                    eprintln!("hint: Check disk space and network connectivity (model downloads on first use).");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(feature = "local-embedding"))]
        "local" => {
            eprintln!("error: Config specifies provider 'local' but this binary was built without local-embedding support.");
            eprintln!("hint: Install with local embedding support: cargo install --path . --features local-embedding");
            eprintln!("      Or switch to Ollama: set embedding.provider = 'ollama' in config.");
            std::process::exit(1);
        }
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            let region = resolve_bedrock_region(config.embedding.effective_base_url());
            let timeout = timeout_override.unwrap_or(config.embedding.timeout_secs);
            Box::new(
                factbase::bedrock::BedrockEmbedding::new(
                    &config.embedding.model,
                    config.embedding.dimension,
                    region,
                    config.embedding.profile.as_deref(),
                    timeout,
                )
                .await,
            )
        }
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => {
            eprintln!("error: Config specifies provider 'bedrock' but this binary was built without Bedrock support.");
            eprintln!("hint: Install with Bedrock support: cargo install --path . --features bedrock");
            eprintln!("      Or switch to Ollama: set embedding.provider = 'ollama' in config.");
            std::process::exit(1);
        }
        other => {
            let base_url = config.embedding.effective_base_url();
            validate_ollama_url(base_url, "embedding", other);
            let timeout = timeout_override.unwrap_or(config.embedding.timeout_secs);
            Box::new(OllamaEmbedding::with_config(
                base_url,
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

/// Create embedding provider wrapped in LRU cache with optional persistent SQLite cache.
pub async fn setup_cached_embedding(
    config: &Config,
    timeout_override: Option<u64>,
    db: &Database,
) -> CachedEmbedding<PersistentCachedEmbedding<Box<dyn EmbeddingProvider>>> {
    let embedding = setup_embedding_with_timeout(config, timeout_override).await;
    let persistent = PersistentCachedEmbedding::new(
        embedding,
        db.clone(),
        config.embedding.model.clone(),
        config.embedding.persistent_cache_size,
    );
    CachedEmbedding::new(persistent, config.embedding.cache_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use factbase::config::EmbeddingConfig;
    use factbase::embedding::EmbeddingProvider;

    fn test_config() -> Config {
        let config = Config::default();
        Config {
            embedding: EmbeddingConfig {
                ..config.embedding
            },
            ..config
        }
    }

    #[tokio::test]
    async fn test_setup_embedding_with_timeout_uses_config_default() {
        let config = test_config();
        let expected_dim = config.embedding.dimension;
        let embedding = setup_embedding_with_timeout(&config, None).await;
        assert_eq!(embedding.dimension(), expected_dim);
    }

    #[tokio::test]
    async fn test_setup_embedding_with_timeout_override() {
        let config = test_config();
        let expected_dim = config.embedding.dimension;
        let embedding = setup_embedding_with_timeout(&config, Some(120)).await;
        assert_eq!(embedding.dimension(), expected_dim);
    }

    #[tokio::test]
    async fn test_setup_link_detector_no_llm_needed() {
        let _detector = factbase::link_detection::LinkDetector::new();
    }

    #[test]
    fn test_setup_builder_default() {
        let setup = Setup::new();
        assert!(!setup.check_exists);
        assert!(!setup.require_repo);
    }

    #[test]
    fn test_setup_builder_check_exists() {
        let setup = Setup::new().check_exists();
        assert!(setup.check_exists);
    }

    #[test]
    fn test_setup_builder_require_repo() {
        let setup = Setup::new().require_repo(Some("test"));
        assert!(setup.require_repo);
    }

    #[test]
    fn test_auto_init_repo_creates_factbase_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        assert!(!dir.join(".factbase").exists());

        let (_, _, repo) = auto_init_repo(dir).unwrap();
        assert!(dir.join(".factbase").exists());
        assert!(dir.join(".factbase/factbase.db").exists());
        assert!(dir.join("perspective.yaml").exists());
        assert_eq!(repo.id, factbase::DEFAULT_REPO_ID);
    }

    #[test]
    fn test_auto_init_repo_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let (_, _, r1) = auto_init_repo(dir).unwrap();
        let (_, _, r2) = auto_init_repo(dir).unwrap();
        assert_eq!(r1.id, r2.id);
    }
}
