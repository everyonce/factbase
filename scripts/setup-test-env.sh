#!/bin/bash
# Setup test environment for Phase 5 E2E tests
# Verifies Ollama is running and required models are available

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Factbase Test Environment Setup ==="
echo ""

# Check Ollama is running
echo -n "Checking Ollama server... "
if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo -e "${RED}NOT RUNNING${NC}"
    echo "Start Ollama with: ollama serve"
    exit 1
fi
echo -e "${GREEN}OK${NC}"

# Check embedding model
echo -n "Checking nomic-embed-text model... "
if ! curl -s http://localhost:11434/api/tags | grep -q "nomic-embed-text"; then
    echo -e "${YELLOW}NOT FOUND${NC}"
    echo "Pulling nomic-embed-text..."
    ollama pull nomic-embed-text
else
    echo -e "${GREEN}OK${NC}"
fi

# Check LLM model
echo -n "Checking rnj-1-extended model... "
if ! curl -s http://localhost:11434/api/tags | grep -q "rnj-1-extended"; then
    echo -e "${YELLOW}NOT FOUND${NC}"
    echo "Creating rnj-1-extended model..."
    cat > /tmp/rnj-1-extended.modelfile << 'EOF'
FROM rnj-1:latest
PARAMETER num_ctx 49152
EOF
    ollama create rnj-1-extended -f /tmp/rnj-1-extended.modelfile
else
    echo -e "${GREEN}OK${NC}"
fi

# Quick embedding test
echo -n "Testing embedding generation... "
RESP=$(curl -s http://localhost:11434/api/embeddings -d '{"model":"nomic-embed-text","prompt":"test"}')
if echo "$RESP" | grep -q "embedding"; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo "Embedding test failed: $RESP"
    exit 1
fi

# Create temp test directory
TEST_DIR="${TMPDIR:-/tmp}/factbase-test"
mkdir -p "$TEST_DIR"
echo ""
echo "Test directory: $TEST_DIR"

echo ""
echo -e "${GREEN}=== Environment Ready ===${NC}"
echo "Run tests with: cargo test -- --ignored"
