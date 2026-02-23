# Coding Conventions & Patterns

## Rust Conventions

### Error Handling
```rust
// Use thiserror for error types
#[derive(Error, Debug)]
pub enum FactbaseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
}

// Return Result from functions
pub fn process_file(&self, path: &Path) -> Result<Document, FactbaseError> {
    // Use ? for propagation
    let content = fs::read_to_string(path)?;
    // ...
}
```

### Async Traits
```rust
// Manual desugaring with BoxFuture type alias (no async-trait crate)
use std::future::Future;
use std::pin::Pin;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait EmbeddingProvider: Send + Sync {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>>>;
    fn dimension(&self) -> usize;
}
```

### Database Access
```rust
// Database uses r2d2 connection pool for thread safety
pub struct Database {
    pool: r2d2::Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let conn = self.pool.get()?;
        // Use connection...
    }
}
```

### Configuration
```rust
// Use serde for config structs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub embedding: EmbeddingConfig,
    // ...
}

// Implement Default for sensible defaults
impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "bedrock".to_string(),
            region: None,
            base_url: "us-east-1".to_string(),
            model: "amazon.titan-embed-text-v2:0".to_string(),
            dimension: 1024,
        }
    }
}
```

## File Organization

```
src/
├── main.rs          # CLI entry point, clap setup
├── lib.rs           # Module declarations, re-exports
├── config.rs        # Configuration loading
├── error.rs         # Error types
├── models.rs        # Data structures
├── database.rs      # SQLite operations
├── ollama.rs        # Shared Ollama HTTP client
├── bedrock.rs       # Amazon Bedrock provider (feature-gated)
├── embedding.rs     # EmbeddingProvider trait + Ollama impl
├── llm.rs           # LlmProvider trait + Ollama impl + LinkDetector
├── scanner.rs       # File discovery + scan orchestration
├── processor.rs     # Document processing (ID, title, type)
├── watcher.rs       # File system monitoring
└── mcp/
    ├── mod.rs       # MCP module
    ├── server.rs    # HTTP server setup
    └── tools/       # Tool implementations (18 tools)
        ├── mod.rs
        ├── search.rs
        ├── entity.rs
        ├── document.rs
        └── review.rs
```

## Testing Patterns

### Unit Tests
```rust
// In-module tests
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_id_valid() {
        let content = "<!-- factbase:a1cb2b -->\n# Title";
        let processor = DocumentProcessor::new();
        assert_eq!(processor.extract_id(content), Some("a1cb2b".to_string()));
    }
}
```

### Integration Tests with Fixtures
```rust
// In tests/integration_test.rs
use tempfile::TempDir;

#[test]
fn test_full_scan() {
    let temp = TempDir::new().unwrap();
    // Copy fixtures to temp
    // Run scan
    // Assert results
}
```

### Integration Tests
```rust
// Mark with #[ignore] - run with `cargo test -- --ignored`
#[tokio::test]
#[ignore]
async fn test_embedding_generation() {
    // Skip if inference backend not available
    if !is_backend_available().await {
        return;
    }
    // Test with real inference backend
}
```

## Naming Conventions

- **Structs**: PascalCase (`DocumentProcessor`, `EmbeddingService`)
- **Functions**: snake_case (`extract_id`, `generate_embedding`)
- **Constants**: SCREAMING_SNAKE_CASE (`EMBEDDING_DIM`)
- **Modules**: snake_case (`document_processor`, `mcp_server`)
- **Traits**: PascalCase, often ending in -er or -Provider (`EmbeddingProvider`)

## Documentation

```rust
/// Brief description of the function.
///
/// More detailed explanation if needed.
///
/// # Arguments
/// * `path` - Path to the markdown file
///
/// # Returns
/// The processed document or an error
///
/// # Errors
/// Returns `FactbaseError::Io` if file cannot be read
pub fn process_file(&self, path: &Path) -> Result<Document> {
    // ...
}
```

## Logging

```rust
use tracing::{info, warn, error, debug};

// Info for normal operations
info!("Scanning repository: {}", repo.path.display());

// Warn for recoverable issues
warn!("Skipping file with permission error: {}", path.display());

// Error for failures
error!("Database error: {}", e);

// Debug for detailed info
debug!("Processing file: {}", path.display());
```

## CLI Structure

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "factbase")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init(InitArgs),
    Scan(ScanArgs),
    Search(SearchArgs),
    Serve(ServeArgs),
    Status(StatusArgs),
    #[command(subcommand)]
    Repo(RepoCommands),
}
```

## HTTP/MCP Patterns

```rust
// Axum handler
async fn handle_tool_call(
    State(state): State<AppState>,
    Json(request): Json<McpRequest>,
) -> Result<Json<McpResponse>, StatusCode> {
    match request.tool.as_str() {
        "search_knowledge" => handle_search(state, request.params).await,
        "get_entity" => handle_get_entity(state, request.params).await,
        _ => Err(StatusCode::NOT_FOUND),
    }
}
```
