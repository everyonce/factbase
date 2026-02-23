#!/bin/bash
# Run all tests
set -e

echo "=== Unit Tests ==="
cargo test --lib

echo ""
echo "=== Integration Tests ==="
# Check if Ollama is running
if curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    cargo test -- --ignored
else
    echo "Skipping integration tests (Ollama not running)"
fi

echo ""
echo "✓ Tests completed"
