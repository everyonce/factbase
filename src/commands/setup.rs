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

use super::utils::db_not_found_error;
use factbase::{
    CachedEmbedding, Config, Database, EmbeddingProvider,
    OllamaEmbedding, PersistentCachedEmbedding, Repository,
};
use std::path::PathBuf;

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
///
/// Checks for the `.factbase/` directory (not the DB file itself) since the
/// directory indicates "this is a factbase repo" even before the DB is created.
fn local_or_global_db_path(config: &Config) -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let factbase_dir = cwd.join(".factbase");
        if factbase_dir.is_dir() {
            return factbase_dir.join("factbase.db");
        }
    }
    PathBuf::from(shellexpand::tilde(&config.database.path).to_string())
}

/// Load config and open database - common setup for most commands.
///
/// Checks for a local `.factbase/factbase.db` in the current directory first,
/// then falls back to the global config `database.path`.
pub fn setup_database() -> anyhow::Result<(Config, Database)> {
    print_first_run_notice();
    let config = Config::load(None)?;
    let db_path = local_or_global_db_path(&config);
    let db = config.open_database(&db_path)?;
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
    let db_path = local_or_global_db_path(&config);

    if !db_path.exists() {
        return Err(db_not_found_error(&db_path));
    }

    let db = config.open_database(&db_path)?;
    Ok((config, db, db_path))
}

/// Auto-initialize a factbase repository in the given directory.
///
/// Creates `.factbase/` dir, `perspective.yaml`, database, and registers the repo.
/// Returns (Config, Database, Repository). Shared by `cmd_mcp`, `cmd_serve`, etc.
pub fn auto_init_repo(dir: &std::path::Path) -> anyhow::Result<(Config, Database, Repository)> {
    let factbase_dir = dir.join(".factbase");
    std::fs::create_dir_all(&factbase_dir)?;
    let perspective_path = dir.join("perspective.yaml");
    if !perspective_path.exists() {
        std::fs::write(&perspective_path, factbase::PERSPECTIVE_TEMPLATE)?;
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

/// Canonicalize a path, stripping the Windows `\\?\` prefix if present.
pub fn clean_canonicalize(path: &std::path::Path) -> std::path::PathBuf {
    factbase::organize::clean_canonicalize(path)
}

/// Find repository by ID or from current directory, returning config for callers that need it.
///
/// Use this when you need both the repository and config (e.g., for embedding/LLM setup).
/// Use `find_repo()` when you only need the database and repository.
pub fn find_repo_with_config(
    repo_id: Option<&str>,
) -> anyhow::Result<(Config, Database, Repository)> {
    print_first_run_notice();
    let dir = std::env::current_dir()?;
    let config = Config::load(None)?;

    // Only check the current directory for .factbase/ — don't walk up.
    // Walking up caused confusion when a parent directory had its own .factbase/.
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
        anyhow::bail!("No repository found in {}", factbase_dir.display());
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

/// Validate that a base_url looks like an HTTP URL for Ollama.
/// Exits with a clear error if it looks like an AWS region or other non-URL value.
fn validate_ollama_url(base_url: &str, section: &str, provider: &str) {
    if base_url.starts_with("http://") || base_url.starts_with("https://") {
        return;
    }
    eprintln!(
        "error: {section}.base_url is '{}' which is not a valid URL for provider '{}'.",
        base_url, provider
    );
    if base_url.contains('-') && base_url.chars().all(|c| c.is_alphanumeric() || c == '-') {
        // Looks like an AWS region
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
            match factbase::LocalEmbeddingProvider::new(true) {
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
    use factbase::config::{EmbeddingConfig, OllamaConfig};
    use factbase::EmbeddingProvider;

    fn test_config() -> Config {
        let config = Config::default();
        Config {
            embedding: EmbeddingConfig {
                // Use whatever the default provider is for this build
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
        let _detector = factbase::LinkDetector::new();
    }
}
