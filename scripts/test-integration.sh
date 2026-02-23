#!/bin/bash
# Run integration tests (requires Ollama)
set -e

# Check if Ollama is running
if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo "Error: Ollama is not running"
    echo "Start Ollama with: ollama serve"
    exit 1
fi

echo "Ollama is running, starting integration tests..."
echo ""

echo "=== Ollama Integration Tests ==="
cargo test --test ollama_integration -- --ignored --nocapture

echo ""
echo "=== Multi-Repo Integration Tests ==="
cargo test --test multi_repo_integration -- --ignored --nocapture

echo ""
echo "=== MCP Integration Tests ==="
cargo test --test mcp_integration -- --ignored --nocapture

echo ""
echo "=== Watcher Integration Tests ==="
cargo test --test watcher_integration -- --ignored --nocapture

echo ""
echo "✓ All integration tests passed"
