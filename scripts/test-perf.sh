#!/bin/bash
# Run performance tests (requires Ollama)
set -e

# Check if Ollama is running
if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo "Error: Ollama is not running"
    echo "Start Ollama with: ollama serve"
    exit 1
fi

echo "Running performance tests..."
echo "Note: These tests may take several minutes"
echo ""

cargo test --test performance -- --ignored --nocapture

echo ""
echo "✓ Performance tests completed"
