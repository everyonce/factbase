#!/bin/bash
# Validate README CLI examples against actual --help output
# Exits with non-zero if README documents flags that don't exist

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
README="$PROJECT_ROOT/README.md"
FACTBASE="${FACTBASE:-$PROJECT_ROOT/target/release/factbase}"

# Colors (disabled if NO_COLOR set or not a terminal)
if [[ -z "$NO_COLOR" && -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' NC=''
fi

errors=0
warnings=0

log_ok() { echo -e "${GREEN}✓${NC} $1"; }
log_warn() { echo -e "${YELLOW}⚠${NC} $1"; ((warnings++)); }
log_err() { echo -e "${RED}✗${NC} $1"; ((errors++)); }

# Check binary exists
if [[ ! -x "$FACTBASE" ]]; then
    echo "Error: factbase binary not found at $FACTBASE"
    echo "Run 'cargo build --release' first"
    exit 1
fi

echo "Validating README CLI documentation..."
echo "Binary: $FACTBASE"
echo

# Get main help output
main_help=$("$FACTBASE" --help 2>&1)

# Extract subcommands from main help
subcommands=$(echo "$main_help" | sed -n '/^Commands:/,/^Options:/p' | grep -E '^\s+\w+' | awk '{print $1}')

echo "=== Checking subcommands ==="

# Extract subcommands mentioned in README (from ### `factbase <cmd>` headers)
readme_cmds=$(grep -oE '### `factbase ([a-z]+)' "$README" | sed 's/### `factbase //' | sort -u)

for cmd in $readme_cmds; do
    if echo "$subcommands" | grep -qw "$cmd"; then
        log_ok "Subcommand '$cmd' exists"
    else
        log_err "README documents 'factbase $cmd' but subcommand doesn't exist"
    fi
done

echo
echo "=== Checking documented flags ==="

# Check if a flag exists in help output
# Handles: -v, --verbose, -v/--verbose, etc.
flag_exists() {
    local help_text="$1"
    local flag="$2"
    
    # For short flags like -v, match "-v," or "-v " or "-v..."
    if [[ "$flag" == -? ]]; then
        echo "$help_text" | command grep -qE "(^|\s)${flag}(,|\s|\.\.\.)"
    else
        # For long flags like --verbose, match "--verbose" anywhere
        echo "$help_text" | command grep -qF -- "$flag"
    fi
}

check_flags() {
    local cmd="$1"
    shift
    local flags=("$@")
    
    local cmd_help
    cmd_help=$("$FACTBASE" "$cmd" --help 2>&1) || {
        log_err "Failed to get help for '$cmd'"
        return
    }
    
    for flag in "${flags[@]}"; do
        if flag_exists "$cmd_help" "$flag"; then
            log_ok "$cmd: $flag"
        else
            log_err "$cmd: flag '$flag' documented in README but not in --help"
        fi
    done
}

# scan flags from README
check_flags scan -v --verbose -q --quiet -j --json --dry-run -w --watch \
    --check-duplicates --stats --since --stats-only --check --verify --fix \
    --prune --hard --reindex --batch-size --no-links -y --yes



# status flags from README
check_flags status -d --detailed -j --json -f --format






# repo subcommands
repo_help=$("$FACTBASE" repo --help 2>&1)
for subcmd in add remove list; do
    if echo "$repo_help" | grep -qw "$subcmd"; then
        log_ok "repo $subcmd exists"
    else
        log_err "README documents 'repo $subcmd' but doesn't exist"
    fi
done

# db subcommands
db_help=$("$FACTBASE" db --help 2>&1)
if echo "$db_help" | grep -qw "vacuum"; then
    log_ok "db vacuum exists"
else
    log_err "README documents 'db vacuum' but doesn't exist"
fi

echo
echo "=== Summary ==="
echo "Errors: $errors"
echo "Warnings: $warnings"

if [[ $errors -gt 0 ]]; then
    echo -e "${RED}README validation failed${NC}"
    exit 1
fi

echo -e "${GREEN}README validation passed${NC}"
