#!/usr/bin/env bash
# Release build script for factbase
#
# Builds release binaries for the current platform.
# Run on each target platform to produce its binary:
#
#   Linux (x86_64):  ./scripts/release-build.sh
#   macOS (ARM):     ./scripts/release-build.sh
#   macOS (Intel):   ./scripts/release-build.sh
#   Windows:         cargo build --release --features bedrock
#
# After building on each platform, create a tagged release:
#
#   1. Update VERSION file and Cargo.toml
#   2. git commit -m "release: vX.Y.Z" && git tag vX.Y.Z && git push --tags
#   3. Build on each platform (Linux, macOS ARM, macOS Intel, Windows)
#   4. Collect binaries into release/ directory
#   5. Upload to your release hosting (Gitea releases, S3, etc.)
#
# Binary naming convention:
#   factbase-vX.Y.Z-linux-x86_64
#   factbase-vX.Y.Z-darwin-arm64
#   factbase-vX.Y.Z-darwin-x86_64
#   factbase-vX.Y.Z-windows-x86_64.exe

set -euo pipefail

VERSION=$(cat VERSION 2>/dev/null || grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Normalize arch names
case "$ARCH" in
    aarch64|arm64) ARCH="arm64" ;;
    x86_64|amd64)  ARCH="x86_64" ;;
esac

BINARY_NAME="factbase-v${VERSION}-${OS}-${ARCH}"

echo "Building factbase v${VERSION} for ${OS}-${ARCH}..."

cargo build --release --features bedrock

mkdir -p release

if [ "$OS" = "windows" ] || [ -f "target/release/factbase.exe" ]; then
    cp target/release/factbase.exe "release/${BINARY_NAME}.exe"
    echo "Built: release/${BINARY_NAME}.exe"
else
    cp target/release/factbase "release/${BINARY_NAME}"
    chmod +x "release/${BINARY_NAME}"
    echo "Built: release/${BINARY_NAME}"
fi

ls -lh release/${BINARY_NAME}*
