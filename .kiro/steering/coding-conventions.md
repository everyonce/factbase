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
// Use async-trait crate for async trait methods
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn generate(&self, text: &str) -> Result<Vec<f32>>;
    fn dimension(&self) -> usize;
}
```

### Database Access
```rust
// Database wraps connection in Arc<Mutex> for thread safety
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let conn = self.conn.lock().unwrap();
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
            provider: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text".to_string(),
            dimension: 768,
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
├── embedding.rs     # Embedding provider trait + Ollama impl
├── llm.rs           # LLM provider trait + Ollama impl + LinkDetector
├── scanner.rs       # File discovery + scan orchestration
├── processor.rs     # Document processing (ID, title, type)
├── watcher.rs       # File system monitoring
└── mcp/
    ├── mod.rs       # MCP module
    ├── server.rs    # HTTP server setup
    └── tools.rs     # Tool implementations
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

### Ollama Integration Tests
```rust
// Mark with #[ignore] - run with `cargo test -- --ignored`
#[tokio::test]
#[ignore]
async fn test_embedding_generation() {
    // Skip if Ollama not running
    if !is_ollama_available().await {
        return;
    }
    // Test with real Ollama
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
