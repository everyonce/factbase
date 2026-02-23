# Testing Guide

## Overview

Factbase has comprehensive test coverage across multiple categories:

1. **Unit Tests** - Fast, no external dependencies (~83 tests)
2. **Integration Tests** - Require live Ollama instance
3. **E2E Tests** - Full system tests with MCP server
4. **Performance Tests** - Stress and load testing
5. **Phase 5 Tests** - Comprehensive end-to-end testing with real Ollama

## Prerequisites

### Ollama Setup

All integration and E2E tests require Ollama with specific models:

```bash
# Start Ollama
ollama serve

# Pull required models
ollama pull qwen3-embedding:0.6b

# Create extended context model for link detection
cat > /tmp/rnj-1-extended.modelfile << 'EOF'
FROM rnj-1:latest
PARAMETER num_ctx 49152
EOF
ollama create rnj-1-extended -f /tmp/rnj-1-extended.modelfile
```

### Verify Setup

```bash
factbase doctor
# ✓ Ollama server: http://localhost:11434 (running)
# ✓ Embedding model: qwen3-embedding:0.6b (available)
# ✓ LLM model: rnj-1-extended (available)
```

Or use the setup script:

```bash
./scripts/setup-test-env.sh
```

## Running Tests

### Unit Tests Only (Fast, No Ollama)

```bash
cargo test --lib
```

Runs ~83 unit tests in <1 second.

### All Integration Tests (Requires Ollama)

```bash
# Run all tests (unit + integration)
cargo test

# Or run specific test files
cargo test --test ollama_integration
cargo test --test multi_repo_integration
cargo test --test mcp_integration
cargo test --test watcher_integration
```

### E2E Tests

```bash
cargo test --test serve_e2e
cargo test --test full_scan_e2e
cargo test --test multi_repo_e2e
cargo test --test watcher_e2e
cargo test --test mcp_e2e
```

### Phase 5 Comprehensive Tests

```bash
# All Phase 5 tests
cargo test --test concurrent_stress
cargo test --test error_recovery
cargo test --test stability
cargo test --test benchmarks
cargo test --test edge_cases
cargo test --test data_integrity
cargo test --test cli_integration

# Long-running stability test (10 minutes)
cargo test test_stability_long -- --ignored --nocapture
```

### Performance Benchmarks

```bash
# Run benchmarks with output
cargo test benchmark --release -- --nocapture
```

## Test Files

### Unit Tests (in-module)

| Module | Tests |
|--------|-------|
| `src/config.rs` | Config loading and validation |
| `src/database.rs` | Database operations |
| `src/embedding.rs` | Embedding provider |
| `src/llm.rs` | LLM and link detection |
| `src/processor.rs` | Document processing |
| `src/scanner.rs` | File scanning |
| `src/watcher.rs` | File watching |
| `src/mcp/tools.rs` | MCP tool implementations |

### Integration Tests

| File | Description |
|------|-------------|
| `tests/ollama_integration.rs` | Embedding and LLM tests |
| `tests/multi_repo_integration.rs` | Multi-repository workflows |
| `tests/mcp_integration.rs` | MCP server tool tests |
| `tests/watcher_integration.rs` | File watcher tests |

### E2E Tests

| File | Description |
|------|-------------|
| `tests/serve_e2e.rs` | Serve command E2E tests |
| `tests/full_scan_e2e.rs` | Full scan with real Ollama |
| `tests/multi_repo_e2e.rs` | Multi-repo with real Ollama |
| `tests/watcher_e2e.rs` | File watcher with real rescans |
| `tests/mcp_e2e.rs` | MCP server with real search |

### Phase 5 Tests

| File | Description |
|------|-------------|
| `tests/concurrent_stress.rs` | Concurrent operations stress test |
| `tests/error_recovery.rs` | Error handling and resilience |
| `tests/stability.rs` | Long-running stability tests |
| `tests/benchmarks.rs` | Performance benchmarks |
| `tests/edge_cases.rs` | Edge case and boundary tests |
| `tests/data_integrity.rs` | Data consistency tests |
| `tests/cli_integration.rs` | CLI command integration tests |

## Test Fixtures

### Fixture Repository

A comprehensive test fixture repository is available at `tests/fixtures/test-repo/`:

```
tests/fixtures/test-repo/
├── people/           # 10 person documents
├── projects/         # 8 project documents
├── concepts/         # 5 concept documents
├── notes/            # 5 edge case documents
└── perspective.yaml  # Repository configuration
```

### Using Fixtures

```rust
use common::fixtures::{copy_fixture_repo, create_temp_repo};

// Copy fixture repo to temp directory
let temp = TempDir::new().unwrap();
copy_fixture_repo(temp.path());

// Or create empty temp repo
let temp = create_temp_repo("test");
```

## Test Helpers

### Common Module

`tests/common/mod.rs` provides shared test utilities:

```rust
// Ollama availability check
require_ollama().await;  // Panics with helpful message if unavailable

// Test server helper
let server = TestServer::start_with_data().await;
let resp = server.call_tool("search_knowledge", json!({"query": "test"})).await;
```

### Ollama Helpers

`tests/common/ollama_helpers.rs`:

```rust
// Require Ollama (fails fast if unavailable)
require_ollama().await;

// Require specific models
require_models().await;

// Wait for Ollama with retries
wait_for_ollama(10, Duration::from_secs(2)).await;
```

## CI Configuration

### GitHub Actions

The CI workflow runs:

1. **Unit tests** - Always run, no Ollama needed
2. **Integration tests** - Run on self-hosted runners with Ollama
3. **Clippy** - Lint checks
4. **Format** - Code formatting checks

See `.github/workflows/ci.yml` for configuration.

### Running in CI

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
