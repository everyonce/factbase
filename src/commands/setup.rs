//! Database and service setup for CLI commands.
//!
//! # Builder Pattern
//!
//! Use `Setup::new()` to configure what you need, then `.build()` to get a `SetupContext`:
//!
//! ```ignore
//! // Just config + database (global or local)
//! let ctx = Setup::new().build()?;
//!
//! // Require database file exists (error if missing)
//! let ctx = Setup::new().check_exists().build()?;
//!
//! // Require a single repository (from .factbase/ in cwd)
//! let ctx = Setup::new().require_repo(args.repo.as_deref()).build()?;
//! // ctx.config, ctx.db, ctx.repo() all available
//!
//! // Resolve multiple repositories with optional filter
//! let ctx = Setup::new().resolve_repos(args.repo.as_deref()).build()?;
//! // ctx.config, ctx.db, ctx.repos() all available
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

enum RepoMode {
    /// No repository resolution needed
    None,
    /// Resolve a single repo from .factbase/ in cwd
    Single(Option<String>),
    /// Resolve multiple repos (global DB), with optional filter
    Multiple(Option<String>),
}

/// Builder for CLI command setup. Configures what resources are needed,
/// then `.build()` loads/validates everything in one shot.
pub struct Setup {
    check_exists: bool,
    repo_mode: RepoMode,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            check_exists: false,
            repo_mode: RepoMode::None,
        }
    }

    /// Fail at build time if the database file doesn't exist on disk.
    pub fn check_exists(mut self) -> Self {
        self.check_exists = true;
        self
    }

    /// Resolve a single repository from `.factbase/` in the current directory.
    /// Optionally filter by repo ID. Fails if no `.factbase/` dir or no matching repo.
    pub fn require_repo(mut self, repo_id: Option<&str>) -> Self {
        self.repo_mode = RepoMode::Single(repo_id.map(String::from));
        self
    }

    /// Resolve repositories from the global database, with optional ID/name filter.
    /// Fails if no repositories match.
    pub fn resolve_repos(mut self, filter: Option<&str>) -> Self {
        self.repo_mode = RepoMode::Multiple(filter.map(String::from));
        self
    }

    pub fn build(self) -> anyhow::Result<SetupContext> {
        print_first_run_notice();
        let config = Config::load(None)?;

        // For Single repo mode, we require .factbase/ in cwd
        if let RepoMode::Single(ref repo_id) = self.repo_mode {
            let dir = std::env::current_dir()?;
            let factbase_dir = dir.join(".factbase");
            if !factbase_dir.exists() {
                anyhow::bail!("Not in a factbase repository. Run `factbase init <path>` first.");
            }
            let db_path = factbase_dir.join("factbase.db");
            let db = config.open_database(&db_path)?;
            let repos = db.list_repositories()?;
            let repo = if let Some(id) = repo_id {
                repos.into_iter().find(|r| r.id == *id)
            } else {
                repos.into_iter().next()
            };
            let repo = repo.ok_or_else(|| {
                anyhow::anyhow!("No repository found in {}", factbase_dir.display())
            })?;
            return Ok(SetupContext {
                config,
                db,
                db_path,
                repo: Some(repo),
                repos: None,
            });
        }

        // Global/local DB resolution
        let db_path = local_or_global_db_path(&config);

        if self.check_exists && !db_path.exists() {
            return Err(db_not_found_error(&db_path));
        }

        let db = config.open_database(&db_path)?;

        // Resolve multiple repos if requested
        let repos = if let RepoMode::Multiple(ref filter) = self.repo_mode {
            let all = db.list_repositories()?;
            let resolved = super::resolve_repos(all, filter.as_deref())?;
            Some(resolved)
        } else {
            None
        };

        Ok(SetupContext {
            config,
            db,
            db_path,
            repo: None,
            repos,
        })
    }
}

// ---------------------------------------------------------------------------
// SetupContext
// ---------------------------------------------------------------------------

/// Result of `Setup::build()`. Contains config, database, and optionally resolved repositories.
pub struct SetupContext {
    pub config: Config,
    pub db: Database,
    pub db_path: PathBuf,
    pub repo: Option<Repository>,
    pub repos: Option<Vec<Repository>>,
}

impl SetupContext {
    /// Take ownership of the resolved single repository.
    pub fn take_repo(self) -> (Config, Database, Repository) {
        let repo = self.repo.expect("require_repo() was not called on Setup builder");
        (self.config, self.db, repo)
    }

    /// Get the resolved repositories. Panics if `resolve_repos()` was not called.
    pub fn repos(&self) -> &[Repository] {
        self.repos.as_deref().expect("resolve_repos() was not called on Setup builder")
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Print a one-time notice when no config file exists
fn print_first_run_notice() {
    let local = PathBuf::from(".factbase/config.yaml");
    if local.exists() || Config::config_file_exists() {
        return;
    }
    eprintln!(
        "Note: No config found at {} or .factbase/config.yaml. Using defaults ({} provider).",
        Config::default_path().display(),
        Config::default().embedding.provider,
    );
    eprintln!("  Run `factbase doctor` to verify connectivity.\n");
}

/// Resolve the database path: local `.factbase/factbase.db` takes priority
/// over the global config `database.path`.
fn local_or_global_db_path(config: &Config) -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let factbase_dir = cwd.join(".factbase");
        if factbase_dir.is_dir() {
            return factbase_dir.join("factbase.db");
        }
    }
    PathBuf::from(shellexpand::tilde(&config.database.path).to_string())
}

// ---------------------------------------------------------------------------
// Auto-init (special case for serve/mcp)
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
        assert!(matches!(setup.repo_mode, RepoMode::None));
    }

    #[test]
    fn test_setup_builder_check_exists() {
        let setup = Setup::new().check_exists();
        assert!(setup.check_exists);
    }

    #[test]
    fn test_setup_builder_require_repo() {
        let setup = Setup::new().require_repo(Some("test"));
        assert!(matches!(setup.repo_mode, RepoMode::Single(Some(ref id)) if id == "test"));
    }

    #[test]
    fn test_setup_builder_resolve_repos() {
        let setup = Setup::new().resolve_repos(None);
        assert!(matches!(setup.repo_mode, RepoMode::Multiple(None)));
    }
}
