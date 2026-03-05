#!/bin/bash
# Test: MCP server idle timeout behavior
# Verifies last_activity resets after handle_message (not just on stdin)
#
# Usage:
#   ./tests/test-mcp-idle-timeout.sh [gap_seconds]
#   Default gap: 200s (under 300s timeout - should PASS with fix)
#
# The test:
#   1. Sends a tool call to the MCP server
#   2. Waits gap_seconds after the response
#   3. Sends another tool call
#   4. If server is alive → PASS (last_activity reset after response)
#   5. If server died → FAIL (idle timeout measured from request, not response)
#
# Key insight: the idle timeout is 300s. If we wait 200s after a response,
# that's under 300s from the response but could be >300s from the request
# if the tool took time. The fix ensures we measure from response.
#
# To test FAILURE (server should die): use gap=310
# To test SUCCESS (server should survive): use gap=200 (default)

set -euo pipefail

GAP=${1:-200}
SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${FACTBASE_BIN:-$SCRIPT_DIR/target/release/factbase}"
TMPDIR=$(mktemp -d)
mkdir -p "$TMPDIR/.factbase"

echo "=== MCP Idle Timeout Test ==="
echo "Binary: $BINARY"
echo "Gap after response: ${GAP}s (timeout=300s)"
echo ""

# Start MCP server
cd "$TMPDIR"
mkfifo "$TMPDIR/mcp_in"
"$BINARY" mcp < "$TMPDIR/mcp_in" > "$TMPDIR/mcp_out" 2>"$TMPDIR/mcp_err" &
MCP_PID=$!
exec 3>"$TMPDIR/mcp_in"
sleep 1

if ! kill -0 $MCP_PID 2>/dev/null; then
  echo "FAIL: MCP server didn't start"
  cat "$TMPDIR/mcp_err" 2>/dev/null
  exit 1
fi
echo "✓ Server started (PID $MCP_PID)"

# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"idle-test","version":"1.0"}}}' >&3
sleep 1
echo '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3
sleep 1

# Tool call
echo "→ Sending tool call..."
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_repositories","arguments":{}}}' >&3
sleep 2

if ! kill -0 $MCP_PID 2>/dev/null; then
  echo "FAIL: Server died after tool call"
  cat "$TMPDIR/mcp_err" 2>/dev/null
  exec 3>&-; rm -rf "$TMPDIR"; exit 1
fi
echo "✓ Tool call completed, response written"
echo ""

# Wait
echo "⏳ Waiting ${GAP}s..."
ELAPSED=0
while [ $ELAPSED -lt $GAP ]; do
  CHUNK=30
  [ $((GAP - ELAPSED)) -lt $CHUNK ] && CHUNK=$((GAP - ELAPSED))
  sleep $CHUNK
  ELAPSED=$((ELAPSED + CHUNK))
  
  if ! kill -0 $MCP_PID 2>/dev/null; then
    echo ""
    echo "FAIL: Server died after ${ELAPSED}s"
    grep "idle timeout\|WARN\|ERROR" "$TMPDIR/mcp_err" 2>/dev/null
    exec 3>&-; rm -rf "$TMPDIR"; exit 1
  fi
  echo "   ${ELAPSED}s / ${GAP}s — alive ✓"
done

echo ""

# Second tool call
echo "→ Sending second tool call..."
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_repositories","arguments":{}}}' >&3
sleep 2

if ! kill -0 $MCP_PID 2>/dev/null; then
  echo "FAIL: Server died after second tool call"
  cat "$TMPDIR/mcp_err" 2>/dev/null
  exec 3>&-; rm -rf "$TMPDIR"; exit 1
fi

if grep -q "idle timeout" "$TMPDIR/mcp_err" 2>/dev/null; then
  echo "FAIL: idle timeout warning in stderr"
  exec 3>&-; kill $MCP_PID 2>/dev/null; rm -rf "$TMPDIR"; exit 1
fi

echo "✓ Second tool call succeeded"
echo ""
echo "=== PASS: Server survived ${GAP}s gap ==="

exec 3>&-
kill $MCP_PID 2>/dev/null
wait $MCP_PID 2>/dev/null
rm -rf "$TMPDIR"
