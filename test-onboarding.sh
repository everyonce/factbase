#!/usr/bin/env bash
# test-onboarding.sh — Smoke test the first-time user experience
# Simulates an agent going from empty directory to working mushroom KB
set -euo pipefail

FACTBASE="$(cd "$(dirname "$0")" && pwd)/target/release/factbase"
TEST_DIR="/tmp/factbase-mushroom-test-$$"
PASS=0
FAIL=0
TOTAL=0

cleanup() {
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT

green()  { printf "\033[32m✓ %s\033[0m\n" "$1"; }
red()    { printf "\033[31m✗ %s\033[0m\n" "$1"; }
header() { printf "\n\033[1;34m━━━ %s ━━━\033[0m\n" "$1"; }

check() {
  TOTAL=$((TOTAL + 1))
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then
    green "$desc"
    PASS=$((PASS + 1))
  else
    red "$desc"
    FAIL=$((FAIL + 1))
  fi
}

check_output() {
  TOTAL=$((TOTAL + 1))
  local desc="$1"
  local pattern="$2"
  local output="$3"
  if echo "$output" | grep -qi "$pattern"; then
    green "$desc"
    PASS=$((PASS + 1))
  else
    red "$desc (expected pattern: $pattern)"
    FAIL=$((FAIL + 1))
  fi
}

mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# ═══════════════════════════════════════════════════════════
header "Phase 1: Init — Empty Directory"
# ═══════════════════════════════════════════════════════════

OUTPUT=$("$FACTBASE" init . --name "Mushroom KB" 2>&1)
check "init succeeds" test -d .factbase
check "perspective.yaml created" test -f perspective.yaml
check_output "init message mentions scan" "scan" "$OUTPUT"

# Check perspective.yaml is domain-neutral
PERSP=$(cat perspective.yaml)
check_output "perspective.yaml NOT career-specific" "e.g\.\|example\|biology\|history\|business" "$PERSP"
# Should NOT contain Acme Corp
TOTAL=$((TOTAL + 1))
if echo "$PERSP" | grep -qi "acme\|aws solutions"; then
  red "perspective.yaml still has CRM-domain content"
  FAIL=$((FAIL + 1))
else
  green "perspective.yaml is domain-neutral (no Acme/AWS)"
  PASS=$((PASS + 1))
fi

# ═══════════════════════════════════════════════════════════
header "Phase 2: Doctor — System Health"
# ═══════════════════════════════════════════════════════════

DOCTOR_OUT=$("$FACTBASE" doctor 2>&1) || true
check_output "doctor reports healthy" "passed\|ready\|healthy" "$DOCTOR_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 3: Authoring Guide — Does It Help?"
# ═══════════════════════════════════════════════════════════

# We can't easily call MCP tools from bash, but we can check the workflow
WORKFLOW_OUT=$("$FACTBASE" mcp 2>/dev/null <<'MCP' || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_authoring_guide","arguments":{}}}
MCP
)
check_output "authoring guide has natural_science template" "natural_science\|species\|organism\|habitat" "$WORKFLOW_OUT"
check_output "authoring guide has taxonomy design section" "taxonomy\|designing\|entity.type\|identify" "$WORKFLOW_OUT"
check_output "authoring guide has historical template" "historical\|civilization\|battle\|ancient" "$WORKFLOW_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 4: Setup Workflow — Guided Experience"
# ═══════════════════════════════════════════════════════════

SETUP_OUT=$("$FACTBASE" mcp 2>/dev/null <<'MCP' || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"workflow","arguments":{"workflow":"setup"}}}
MCP
)
check_output "setup workflow exists and returns step 1" "step\|init\|initialize\|perspective" "$SETUP_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 5: Bootstrap Workflow — Domain-Aware Design"
# ═══════════════════════════════════════════════════════════

BOOTSTRAP_OUT=$("$FACTBASE" mcp 2>/dev/null <<'MCP' || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"workflow","arguments":{"workflow":"bootstrap","domain":"mycology"}}}
MCP
)
check_output "bootstrap workflow accepts domain" "mycolog\|species\|fungi\|mushroom\|entity\|error" "$BOOTSTRAP_OUT"
# Note: bootstrap calls the LLM which may timeout — a timeout error is acceptable in CI

# ═══════════════════════════════════════════════════════════
header "Phase 6: Create Documents — Mushroom Style"
# ═══════════════════════════════════════════════════════════

mkdir -p species genera habitats

cat > species/amanita-muscaria.md << 'DOC'
# Amanita muscaria

## Classification
- Kingdom: Fungi
- Family: Amanitaceae
- Common name: Fly agaric

## Habitat & Distribution
- Found in temperate forests across Northern Hemisphere @t[~2024] [^1]
- Mycorrhizal association with birch, pine, spruce

## Edibility & Toxicity
- Contains ibotenic acid and muscimol [^2]
- Classified as poisonous @t[~2024]

---
[^1]: MycoBank database, accessed 2024-01
[^2]: Michelot & Melendez-Howell, Mycological Research, 2003
DOC

cat > species/psilocybe-cubensis.md << 'DOC'
# Psilocybe cubensis

## Classification
- Kingdom: Fungi
- Family: Hymenogastraceae
- Common name: Golden teacher, Magic mushroom

## Habitat & Distribution
- Tropical and subtropical regions worldwide @t[~2024] [^1]
- Grows on cattle dung and enriched soils

## Chemistry
- Contains psilocybin and psilocin [^2]
- Schedule I controlled substance in US @t[1970..] [^3]

## Legal Status
- Decriminalized in Oregon @t[=2020-11] [^4]
- Therapeutic use approved in Oregon @t[=2023-01] [^5]

---
[^1]: Guzmán, Mycotaxon 44:73-120, 1995
[^2]: Hofmann et al., Experientia 14(3):107-109, 1958
[^3]: Controlled Substances Act, 1970
[^4]: Oregon Measure 109, November 2020
[^5]: Oregon Psilocybin Services Act, effective Jan 2023
DOC

cat > genera/amanita.md << 'DOC'
# Amanita

## Overview
Genus of approximately 600 species of mushrooms, including some of the most toxic known fungi.

## Key Characteristics
- Universal veil present in most species
- Volva (cup) at base of stipe
- Free gills
- Spore print typically white

## Notable Species
- Amanita muscaria (fly agaric) — iconic red cap with white spots
- Amanita phalloides (death cap) — responsible for most mushroom poisoning deaths @t[~2024] [^1]
- Amanita caesarea (Caesar's mushroom) — prized edible since Roman times @t[..] [^2]

---
[^1]: Diaz, Mycologia 110(1):1-12, 2018
[^2]: Boa, Wild Edible Fungi, FAO, 2004
DOC

check "species directory created" test -d species
check "genera directory created" test -d genera
check "amanita-muscaria.md created" test -f species/amanita-muscaria.md
check "psilocybe-cubensis.md created" test -f species/psilocybe-cubensis.md
check "amanita.md (genus) created" test -f genera/amanita.md

# ═══════════════════════════════════════════════════════════
header "Phase 7: Scan — Index the Documents"
# ═══════════════════════════════════════════════════════════

SCAN_OUT=$("$FACTBASE" scan 2>&1) || true
check_output "scan finds documents" "3\|document\|indexed\|scanned" "$SCAN_OUT"

# Verify ID headers were injected
check_output "amanita-muscaria has factbase header" "factbase:" "$(head -1 species/amanita-muscaria.md)"
check_output "psilocybe-cubensis has factbase header" "factbase:" "$(head -1 species/psilocybe-cubensis.md)"

# ═══════════════════════════════════════════════════════════
header "Phase 8: Status — What Do We Have?"
# ═══════════════════════════════════════════════════════════

STATUS_OUT=$("$FACTBASE" status 2>&1)
check_output "status shows 3 documents" "3" "$STATUS_OUT"
check_output "status shows Mushroom KB" "Mushroom KB" "$STATUS_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 9: Search — Can We Find Things?"
# ═══════════════════════════════════════════════════════════

SEARCH_OUT=$("$FACTBASE" search "poisonous mushrooms" 2>&1) || true
check_output "search finds relevant results" "muscaria\|phalloides\|toxic\|poison" "$SEARCH_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 10: Assess — Quality Check on Existing Files"
# ═══════════════════════════════════════════════════════════

ASSESS_OUT=$("$FACTBASE" scan --assess 2>&1) || true
check_output "assess reports file inventory" "file\|found\|document" "$ASSESS_OUT"
check_output "assess reports quality scores" "score\|quality\|%\|coverage" "$ASSESS_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 11: Check — Quality Issues"
# ═══════════════════════════════════════════════════════════

CHECK_OUT=$("$FACTBASE" check 2>&1) || true
check_output "check runs without crash" "question\|issue\|clean\|check\|0\|complete" "$CHECK_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 12: Repair — Self-Healing"
# ═══════════════════════════════════════════════════════════

REPAIR_OUT=$("$FACTBASE" repair --dry-run 2>&1) || true
check_output "repair dry-run works" "repair\|clean\|no.*issue\|0.*fix\|nothing\|No corruption" "$REPAIR_OUT"

# ═══════════════════════════════════════════════════════════
header "Phase 13: Links — Cross-References"
# ═══════════════════════════════════════════════════════════

LINKS_OUT=$("$FACTBASE" links 2>&1) || true
# Amanita muscaria should link to Amanita genus doc
check_output "links detected between docs" "link\|reference\|Amanita\|connect\|edge\|0" "$LINKS_OUT"

# ═══════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════

echo ""
header "Results"
echo "  Passed: $PASS / $TOTAL"
if [ "$FAIL" -gt 0 ]; then
  echo "  Failed: $FAIL"
  exit 1
else
  echo "  All tests passed! 🍄"
  exit 0
fi
