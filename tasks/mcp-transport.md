# MCP Transport Compliance

## Problem

Factbase's MCP server uses a custom HTTP JSON-RPC endpoint (`POST /mcp`) that doesn't conform to either standard MCP transport. MCP clients (like kiro-cli) can't connect to it because:

1. No stdio transport — clients can't launch factbase as a subprocess
2. HTTP endpoint doesn't implement the MCP lifecycle (`initialize`, `initialized`, `tools/list` via proper protocol)
3. HTTP endpoint doesn't follow the Streamable HTTP spec (no session management, no SSE support, no GET handler)

The MCP spec (2025-03-26) defines exactly two standard transports: **stdio** and **Streamable HTTP**. We need both.

## Spec Reference

https://modelcontextprotocol.io/specification/2025-03-26/basic/transports

## Task 1: Stdio Transport ✅ COMPLETE (Phase 41)

Add a `factbase mcp` subcommand that runs as a stdio MCP server.

### Behavior

- Read newline-delimited JSON-RPC messages from stdin
- Write newline-delimited JSON-RPC responses to stdout
- Logging to stderr only (never write non-JSON-RPC to stdout)
- Messages MUST NOT contain embedded newlines (serialize as single-line JSON)

### Lifecycle

Must handle these methods before any tool calls:

1. Client sends `initialize` request:
```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocolVersion":"2025-03-26",
  "capabilities":{},
  "clientInfo":{"name":"kiro-cli","version":"1.0"}
}}
```

Server responds with:
```json
{"jsonrpc":"2.0","id":1,"result":{
  "protocolVersion":"2025-03-26",
  "capabilities":{"tools":{}},
  "serverInfo":{"name":"factbase","version":"0.4.3"}
}}
```

2. Client sends `notifications/initialized` notification (no `id`, no response needed):
```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

3. Then `tools/list` and `tools/call` work as they do today.

### Implementation

New file: `src/mcp/stdio.rs`

```
loop:
  read line from stdin
  parse as JSON-RPC
  match method:
    "initialize" → return InitializeResult
    "notifications/initialized" → no response (it's a notification — no `id` field)
    "tools/list" → return tools_list() (existing)
    "tools/call" → route through handle_tool_call() (existing)
    "ping" → return empty result {}
    _ → return method-not-found error
  serialize response as single-line JSON
  write to stdout + newline
```

New command: `src/commands/mcp.rs`

- `factbase mcp` — starts stdio MCP server
- Needs the same setup as `cmd_serve`: load config, open DB, create embedding provider
- No file watcher needed (the client manages the lifecycle)
- Runs until stdin closes or EOF

### Wiring

- Add `mcp` subcommand to clap in `src/commands/mod.rs`
- The `mcp` feature flag should gate this (same as the HTTP server)

### Client Config

When this works, kiro-cli config will be:
```json
{
  "mcpServers": {
    "factbase": {
      "command": "/home/ubuntu/work/factbase/target/release/factbase",
      "args": ["mcp"],
      "cwd": "/home/ubuntu/work/factbase-docs"
    }
  }
}
```

## Task 2: Streamable HTTP Transport ✅ COMPLETE (Phase 41)

Upgrade the existing `factbase serve` HTTP endpoint to comply with the Streamable HTTP spec.

### Current State (`src/mcp/server.rs`)

- `POST /mcp` accepts a single `McpRequest`, returns `Content-Type: application/json`
- No `initialize` handling — `handle_tool_call` returns method-not-found for it
- No GET handler on `/mcp`
- No session management
- No SSE support

### Required Changes

#### 1. Handle `initialize` in `mcp_handler`

Before routing to `handle_tool_call`, check if `method == "initialize"` and return `InitializeResult` (same as stdio). Also handle `notifications/initialized` (return 202 Accepted, no body) and `ping`.

#### 2. Content-Type negotiation

The client sends `Accept: application/json, text/event-stream`. For simple request/response (which is all factbase needs — no streaming), returning `Content-Type: application/json` is spec-compliant. No SSE needed for our use case.

Per the spec: the server MUST return either `Content-Type: text/event-stream` or `Content-Type: application/json`. Returning `application/json` with a single JSON-RPC response is valid.

#### 3. Handle notifications and responses (no `id` field)

When the POST body is a JSON-RPC notification (no `id`) or response, return HTTP 202 Accepted with no body. Currently the server would fail to parse these because `McpRequest` requires an `id` field.

Fix: make `id` optional in `McpRequest` (`pub id: Option<Value>` or `#[serde(default)] pub id: Value`). If the message has no `id`, it's a notification — process it and return 202.

#### 4. GET /mcp

Add a GET handler that returns `405 Method Not Allowed`. This is spec-compliant for servers that don't initiate server-to-client messages. (Factbase doesn't need to push notifications to clients.)

#### 5. Session management (optional but recommended)

On `initialize` response, include `Mcp-Session-Id` header (UUID). Validate it on subsequent requests. This prevents cross-client confusion when multiple agents hit the same server.

### Files to Modify

- `src/mcp/server.rs` — add initialize handling, GET handler, notification handling
- `src/mcp/tools/mod.rs` — make `McpRequest.id` handle missing/null values for notifications

## Shared Code

Both transports reuse:
- `handle_tool_call()` in `src/mcp/tools/mod.rs` — all 18 tool implementations
- `tools_list()` in `src/mcp/tools/schema.rs` — tool schema
- `McpRequest`, `McpResponse` types

Extract the `InitializeResult` construction into a shared helper:
```rust
pub fn initialize_result() -> Value {
    serde_json::json!({
        "protocolVersion": "2025-03-26",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "factbase",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}
```

## Testing

### Stdio
```bash
# Should respond to initialize + tools/list
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | factbase mcp
```

### Streamable HTTP
```bash
# Initialize
curl -X POST http://localhost:3000/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'

# GET should return 405
curl -X GET http://localhost:3000/mcp -H 'Accept: text/event-stream'
```

### Integration
```bash
# Stdio with kiro-cli (after adding to MCP config)
kiro-cli chat --trust-all-tools --no-interactive --model claude-opus-4.6 \
  "Call list_repositories using factbase MCP tools and tell me what you see"
```

## Order of Operations

1. Implement stdio transport (`factbase mcp`) — this unblocks kiro-cli integration immediately
2. Upgrade HTTP transport — this enables shared-server mode for production
3. Update `docs/agent-integration.md` with both config examples
