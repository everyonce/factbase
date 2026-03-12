# Testing Guide

## Overview

Factbase has comprehensive test coverage across multiple categories:

1. **Unit Tests** - Fast, no external dependencies (~2500+ tests)
2. **Integration Tests** - Require live Ollama instance or Bedrock credentials
3. **E2E Tests** - Full system tests with MCP server
4. **Frontend Tests** - Web UI component tests (requires `web` feature)

## Prerequisites

### Ollama Setup (for integration tests)

Integration and E2E tests that require an embedding backend use Ollama:

```bash
# Start Ollama
ollama serve

# Pull required model
ollama pull qwen3-embedding:0.6b
```

### Verify Setup

```bash
factbase doctor
# ✓ Embedding provider: local (bge-small-en-v1.5, 384-dim)
# All checks passed. Ready to scan.
```

## Running Tests

### Unit Tests Only (Fast, No External Dependencies)

```bash
cargo test --lib
```

### Binary/CLI Tests

```bash
cargo test --bin factbase
```

### All Tests (Unit + Integration, Requires Ollama or Bedrock)

```bash
cargo test
```

### Specific Test Files

```bash
cargo test --test ollama_integration
cargo test --test mcp_integration
cargo test --test watcher_integration
```

### E2E Tests

```bash
cargo test --test serve_e2e
cargo test --test full_scan_e2e
cargo test --test mcp_e2e
cargo test --test watcher_e2e
```

### Frontend Tests (web feature)

```bash
cd web && npm test
```

### E2E Frontend Tests (requires running server)

```bash
cd web && npm run test:e2e
```

## Test Files

### Unit Tests (in-module)

| Module | Tests |
|--------|-------|
| `src/config/` | Config loading and validation |
| `src/database/` | Database operations |
| `src/embedding.rs` | Embedding provider |
| `src/processor/` | Document processing |
| `src/scanner/` | File scanning |
| `src/watcher.rs` | File watching |
| `src/mcp/tools/` | MCP tool implementations |

### Integration Tests

| File | Description |
|------|-------------|
| `tests/ollama_integration.rs` | Embedding tests with Ollama |
| `tests/multi_repo_integration.rs` | Multi-repository workflows |
| `tests/mcp_integration.rs` | MCP server tool tests |
| `tests/watcher_integration.rs` | File watcher tests |

### E2E Tests

| File | Description |
|------|-------------|
| `tests/serve_e2e.rs` | Serve command E2E tests |
| `tests/full_scan_e2e.rs` | Full scan with real embedding backend |
| `tests/multi_repo_e2e.rs` | Multi-repo workflows |
| `tests/watcher_e2e.rs` | File watcher with real rescans |
| `tests/mcp_e2e.rs` | MCP server with real search |

### Additional Tests

| File | Description |
|------|-------------|
| `tests/concurrent_stress.rs` | Concurrent operations stress test |
| `tests/error_recovery.rs` | Error handling and resilience |
| `tests/stability.rs` | Long-running stability tests |
| `tests/benchmarks.rs` | Performance benchmarks |
| `tests/edge_cases.rs` | Edge case and boundary tests |
| `tests/data_integrity.rs` | Data consistency tests |
| `tests/cli.rs` | CLI command integration tests |

## Test Fixtures

A test fixture repository is available at `tests/fixtures/`:

```rust
use common::TestContext;

#[tokio::test]
async fn test_something() {
    let ctx = TestContext::new("test-repo");
    ctx.add_file("doc.md", "# Test\nContent here");

    let result = run_scan(&ctx.db, &ctx.repo, &ctx.config).await;
    assert!(result.is_ok());
}
```

## Code Quality

```bash
# Format check
cargo fmt --check

# Lint check
cargo clippy -- -D warnings

# Build release
cargo build --release

# Run all checks
cargo fmt --check && cargo clippy -- -D warnings && cargo test --lib
```

## CI Configuration

The CI workflow runs:

1. **Unit tests** — Always run, no external dependencies needed
2. **Integration tests** — Run on runners with Ollama or Bedrock access
3. **Clippy** — Lint checks
4. **Format** — Code formatting checks

```yaml
# Unit tests only (any runner)
- run: cargo test --lib

# Integration tests (requires Ollama)
- run: |
    ollama serve &
    sleep 5
    ollama pull qwen3-embedding:0.6b
    cargo test
```

## Troubleshooting

### Ollama Not Available

```
Error: Ollama not available at http://localhost:11434
Start Ollama with: ollama serve
```

Solution: Start Ollama and ensure models are pulled.

### Model Not Found

```
Error: Model 'qwen3-embedding:0.6b' not found
```

Solution: Pull the model with `ollama pull qwen3-embedding:0.6b`.

### Test Timeout

Long-running tests may timeout. Increase timeout:

```bash
RUST_TEST_TIME_UNIT=60000 cargo test --test stability
```

### Flaky Tests

Some tests involving file system events may be timing-sensitive. Retry or increase debounce windows if tests fail intermittently.
