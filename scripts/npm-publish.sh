#!/usr/bin/env bash
set -euo pipefail

# Publish @everyonce/factbase to npm with pre-built binaries.
#
# BUILD ALL 4 PLATFORMS FROM MACOS:
#   brew install zig
#   cargo install cargo-zigbuild
#   rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu
#   ./scripts/npm-publish.sh
#
# Usage:
#   ./scripts/npm-publish.sh              # build all platforms + publish
#   ./scripts/npm-publish.sh --dry-run    # build all, skip publish
#   ./scripts/npm-publish.sh --local-only # build current platform only
#   ./scripts/npm-publish.sh --publish-only # skip build, publish what's in bin/

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$ROOT/npm"
VERSION=$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
DRY_RUN=false
LOCAL_ONLY=false
PUBLISH_ONLY=false

for arg in "$@"; do
  case $arg in
    --dry-run) DRY_RUN=true ;;
    --local-only) LOCAL_ONLY=true ;;
    --publish-only) PUBLISH_ONLY=true ;;
  esac
done

echo "=== @everyonce/factbase v${VERSION} ==="

# Platform map: npm-dir|rust-target|binary-name
TARGETS="factbase-linux-x64|x86_64-unknown-linux-gnu|factbase
factbase-darwin-x64|x86_64-apple-darwin|factbase
factbase-darwin-arm64|aarch64-apple-darwin|factbase
factbase-win32-x64|x86_64-pc-windows-gnu|factbase.exe"

get_target_spec() {
  echo "$TARGETS" | grep "^$1|" | head -1
}

# Detect native rust target
native_target() {
  rustc -vV | grep '^host:' | awk '{print $2}'
}

detect_local_pkg() {
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)   echo "factbase-linux-x64" ;;
    Darwin-arm64)   echo "factbase-darwin-arm64" ;;
    Darwin-x86_64)  echo "factbase-darwin-x64" ;;
    MINGW*|MSYS*)   echo "factbase-win32-x64" ;;
    *) echo "" ;;
  esac
}

build_target() {
  local npm_pkg="$1"
  local spec
  spec=$(get_target_spec "$npm_pkg")
  local target=$(echo "$spec" | cut -d'|' -f2)
  local binary=$(echo "$spec" | cut -d'|' -f3)
  local pkg_dir="$NPM_DIR/$npm_pkg"
  local native
  native=$(native_target)

  echo "--- Building $target ---"

  if [ "$target" = "$native" ]; then
    cargo build --release --features bedrock --target "$target"
  elif command -v cargo-zigbuild &>/dev/null; then
    cargo zigbuild --release --features bedrock --target "$target"
  else
    echo "  SKIP: $target is not native and cargo-zigbuild is not installed"
    echo "  Install: brew install zig && cargo install cargo-zigbuild"
    return 1
  fi

  mkdir -p "$pkg_dir/bin"
  cp "$ROOT/target/$target/release/$binary" "$pkg_dir/bin/$binary"
  chmod +x "$pkg_dir/bin/$binary"
  echo "  ✓ $(du -h "$pkg_dir/bin/$binary" | cut -f1)"
}

publish_pkg() {
  local pkg_dir="$1"
  local name
  name=$(node -p "require('$pkg_dir/package.json').name" 2>/dev/null || basename "$pkg_dir")
  if [ "$DRY_RUN" = true ]; then
    echo "  [dry-run] $name@$VERSION"
  else
    echo "  Publishing $name@$VERSION..."
    (cd "$pkg_dir" && npm publish --access public)
  fi
}

# Update versions in all package.json files
echo "--- Syncing versions to $VERSION ---"
for pkg_dir in "$NPM_DIR"/factbase*; do
  [ -f "$pkg_dir/package.json" ] || continue
  node -e "
    const fs = require('fs');
    const p = JSON.parse(fs.readFileSync('$pkg_dir/package.json', 'utf8'));
    p.version = '$VERSION';
    if (p.optionalDependencies) {
      for (const k of Object.keys(p.optionalDependencies)) {
        p.optionalDependencies[k] = '$VERSION';
      }
    }
    fs.writeFileSync('$pkg_dir/package.json', JSON.stringify(p, null, 2) + '\n');
  "
done

# Build
if [ "$PUBLISH_ONLY" = false ]; then
  if [ "$LOCAL_ONLY" = true ]; then
    local_pkg=$(detect_local_pkg)
    if [ -z "$local_pkg" ]; then
      echo "ERROR: Could not detect local platform"; exit 1
    fi
    build_target "$local_pkg"
  else
    failed=0
    echo "$TARGETS" | while IFS='|' read -r npm_pkg _ _; do
      build_target "$npm_pkg" || failed=$((failed+1))
    done
    if [ "$failed" -gt 0 ]; then
      echo ""
      echo "WARNING: $failed target(s) failed. Install cargo-zigbuild for cross-compilation."
    fi
  fi
fi

# Show status
echo ""
echo "--- Package status ---"
echo "$TARGETS" | while IFS='|' read -r npm_pkg _ binary; do
  pkg_dir="$NPM_DIR/$npm_pkg"
  if [ -f "$pkg_dir/bin/$binary" ]; then
    echo "  ✓ $npm_pkg ($(du -h "$pkg_dir/bin/$binary" | cut -f1))"
  else
    echo "  ✗ $npm_pkg"
  fi
done

# Publish
if [ "$LOCAL_ONLY" = true ]; then
  echo ""
  echo "Skipping publish (--local-only)."
  exit 0
fi

echo ""
echo "--- Publishing ---"
echo "$TARGETS" | while IFS='|' read -r npm_pkg _ binary; do
  pkg_dir="$NPM_DIR/$npm_pkg"
  [ -f "$pkg_dir/bin/$binary" ] && publish_pkg "$pkg_dir"
done
publish_pkg "$NPM_DIR/factbase"

echo ""
echo "=== Done! ==="
echo "Test with: npx @everyonce/factbase version"
