#!/bin/bash
# Run unit tests only (fast, no Ollama required)
set -e
echo "Running unit tests..."
cargo test --lib
echo "✓ Unit tests passed"
