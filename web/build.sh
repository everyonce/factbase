#!/bin/bash
# Build frontend assets for factbase web UI
# Called by build.rs when web feature is enabled

set -e

cd "$(dirname "$0")"

# Check if npm is available
if ! command -v npm &> /dev/null; then
    echo "Error: npm not found. Install Node.js to build web UI." >&2
    exit 1
fi

# Install dependencies if node_modules doesn't exist or package.json changed
if [ ! -d "node_modules" ] || [ "package.json" -nt "node_modules/.package-lock.json" ]; then
    echo "Installing npm dependencies..."
    npm ci --silent
fi

# Build frontend
echo "Building frontend assets..."
npm run build --silent

echo "Frontend build complete: web/dist/"
