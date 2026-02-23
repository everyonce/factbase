#!/bin/bash
# Profile scan performance with various repository sizes
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
FACTBASE="$PROJECT_DIR/target/release/factbase"
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "=== Factbase Scan Performance Profile ==="
echo "Temp dir: $TEMP_DIR"
echo ""

# Check Ollama is running
if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo "ERROR: Ollama not running. Start with 'ollama serve'"
    exit 1
fi

generate_docs() {
    local count=$1
    local dir="$TEMP_DIR/repo_$count"
    mkdir -p "$dir/people" "$dir/projects" "$dir/notes"
    
    for i in $(seq 1 $count); do
        local type=$((i % 3))
        local subdir
        case $type in
            0) subdir="people" ;;
            1) subdir="projects" ;;
            2) subdir="notes" ;;
        esac
        cat > "$dir/$subdir/doc_$i.md" << EOF
# Document $i

This is test document number $i for performance profiling.

## Overview
Lorem ipsum dolor sit amet, consectetur adipiscing elit.
Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Details
- Item one for document $i
- Item two with more content
- Item three referencing Document $((i % count + 1))

## Notes
Additional notes and content to make the document more realistic.
This helps test embedding generation performance.
EOF
    done
    echo "$dir"
}

profile_scan() {
    local count=$1
    local dir=$(generate_docs $count)
    
    echo "--- Profiling $count documents ---"
    
    # Initialize repo (creates .factbase/factbase.db inside dir)
    "$FACTBASE" init "$dir" --id "test$count" 2>&1 | head -2
    
    # Time the scan
    local start=$(python3 -c 'import time; print(time.time())')
    (cd "$dir" && "$FACTBASE" scan 2>&1) | grep -E "(Scanning|Added|Updated|Links|Scan complete)" || true
    local end=$(python3 -c 'import time; print(time.time())')
    
    local duration=$(python3 -c "print(f'{$end - $start:.2f}')")
    local per_doc=$(python3 -c "print(f'{($end - $start) / $count:.3f}')")
    
    echo "Total time: ${duration}s"
    echo "Per document: ${per_doc}s"
    echo ""
}

# Profile with different sizes
for size in 10 25 50; do
    profile_scan $size
done

echo "=== Profile Complete ==="
