# Contributing to Factbase

## Development Setup

### Prerequisites

- Rust 1.70+
- AWS credentials configured (for Bedrock integration tests)
- Or [Ollama](https://ollama.ai) for self-hosted inference (see [docs/inference-providers.md](docs/inference-providers.md))

```bash
# Clone and build
git clone https://gitea.home.everyonce.com/daniel/factbase.git
cd factbase
cargo build --features bedrock

# Verify setup
cargo run --features bedrock -- doctor
```

## Code Quality Rules

### Error Handling

- **No `unwrap()` in production code** - use `expect("descriptive message")` or `?`
- **No `process::exit()`** - propagate errors via `Result` using `FactbaseError` variants
- **Use error helpers** - `doc_not_found()`, `repo_not_found()`, `format_user_error()`

```rust
// Bad
let doc = db.get_document(id).unwrap();

// Good
let doc = db.get_document(id)?;

// Good (with context)
let doc = db.get_document(id).expect("document should exist after validation");
```

### Patterns & Idioms

- **Use `LazyLock`** for static regex patterns (see `src/patterns.rs`)
- **Use `const`** for compile-time values
- **Replace `if x.is_some() { x.unwrap() }`** with `if let Some(ref val) = x`
- **Mutex handling** - use `if let Ok()` pattern for graceful degradation on poisoned mutex

```rust
// Bad
if config.is_some() {
    let c = config.unwrap();
}

// Good
if let Some(ref c) = config {
    // use c
}
```

### Code Organization

- **Command modules** in `src/commands/` - one file per CLI command
- **MCP tools** in `src/mcp/tools/` - grouped by functionality (document, entity, review, search)
- **Shared helpers** in `src/output.rs` (format_bytes, format_duration)
- **Error formatting** in `src/error.rs`

## Testing

### Running Tests

```bash
# Unit tests only (fast, no inference backend needed)
cargo test --lib

# All tests including integration (requires Bedrock or Ollama)
cargo test --features bedrock

# Binary/CLI tests
cargo test --bin factbase --features bedrock

# Specific test file
cargo test --test ollama_integration
```

### Test Patterns

Use `TestContext` for integration tests:

```rust
use crate::common::TestContext;

#[tokio::test]
async fn test_something() {
    let ctx = TestContext::new("test-repo");
    ctx.add_file("doc.md", "# Test\nContent here");
    
    let result = run_scan(&ctx.db, &ctx.repo, &ctx.config).await;
    assert!(result.is_ok());
}
```

For custom perspective:

```rust
let ctx = TestContext::with_perspective("test-repo", Perspective {
    allowed_types: Some(vec!["person".into(), "project".into()]),
    ..Default::default()
});
```

Use `expect()` with descriptive messages in tests:

```rust
// Bad
let doc = result.unwrap();

// Good
let doc = result.expect("scan should succeed with valid input");
```

## CLI Conventions

### Standard Flags

Most commands support:
- `-j, --json` - JSON output
- `-q, --quiet` - Suppress non-essential output
- `--format <table|json|yaml>` - Output format

### Command Structure

```rust
#[derive(Parser)]
#[command(
    version,
    about = "Brief description",
    after_help = "\
EXAMPLES:
    factbase mycommand \"arg\"
    factbase mycommand --flag value
"
)]
pub struct MyCommandArgs {
    pub required_arg: String,
    
    #[arg(long, short = 'j')]
    pub json: bool,
    
    #[arg(long, short = 'q', help = "Suppress non-essential output")]
    pub quiet: bool,
}
```

### Implementation Pattern

```rust
pub fn run(args: MyCommandArgs) -> Result<(), FactbaseError> {
    let config = Config::load()?;
    let db = setup_database(&config)?;
    
    // Implementation...
    
    if args.json {
        println!("{}", format_json(&result)?);
    } else if !args.quiet {
        println!("{}", result);
    }
    
    Ok(())
}
```

## Adding an MCP Tool

### 1. Add to appropriate module in `src/mcp/tools/`

```rust
// In src/mcp/tools/entity.rs (or new file)

pub async fn my_new_tool(
    db: &Database,
    args: &Value,
) -> Result<Value, FactbaseError> {
    // Use helpers for argument extraction
    let id = get_str_arg_required(args, "id")?;
    let limit = get_u64_arg(args, "limit", 10);
    
    // Use spawn_blocking for database operations
    let result = run_blocking({
        let db = db.clone();
        move || db.some_query(&id)
    }).await?;
    
    Ok(json!({ "result": result }))
}
```

### 2. Export from mod.rs

```rust
// In src/mcp/tools/mod.rs
pub use entity::my_new_tool;
```

### 3. Register in server

```rust
// In src/mcp/server.rs, add to handle_tool_call match
"my_new_tool" => my_new_tool(&state.db, &params.arguments).await,
```

### 4. Add tool definition

```rust
// In src/mcp/server.rs, add to TOOLS array
json!({
    "name": "my_new_tool",
    "description": "Brief description of what it does",
    "inputSchema": {
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Document ID" },
            "limit": { "type": "integer", "description": "Max results" }
        },
        "required": ["id"]
    }
}),
```

### Helper Functions

Available in `src/mcp/tools/mod.rs`:

```rust
get_str_arg(args, "key")           // Option<&str>
get_str_arg_required(args, "key")  // Result<String>
get_u64_arg(args, "key", default)  // u64
get_u64_arg_required(args, "key")  // Result<u64>
get_bool_arg(args, "key", default) // bool
run_blocking(|| { ... })           // For sync DB operations
```

### Error Helpers

```rust
use crate::error::{doc_not_found, repo_not_found};

// Returns helpful error with suggestion
.ok_or_else(|| doc_not_found(&id))?
```

## Feature Flags

- `full` (default) - All features
- `progress` - Progress bars during scan
- `compression` - zstd compression for export/import
- `mcp` - MCP server for AI agents
- `minimal` - CLI-only, no MCP/compression/progress

```bash
# Minimal build
cargo build --release --no-default-features

# Without MCP server
cargo build --release --no-default-features --features "progress,compression"
```

## Pull Request Guidelines

1. Run `cargo test --lib` before submitting
2. Run `cargo clippy` and fix warnings
3. Add tests for new functionality
4. Update README if adding CLI commands or MCP tools
5. Follow existing code patterns and conventions
