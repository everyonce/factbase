# Active Tasks

## Learnings from Past Phases

### General Patterns
- **Graceful degradation**: Log warnings for individual failures, don't fail entire batch/document/scan.
- **`--dry-run` semantics**: Show results but prevent all state changes (DB updates, file writes, hash tracking).
- **Test IDs must be valid 6-char hex** (`[a-f0-9]{6}`) to match `MANUAL_LINK_REGEX` — e.g., `ab1234` not `js1234`.
- **`upsert_document()` does INSERT OR REPLACE** which resets nullable columns not in the INSERT list to NULL — useful for auto-clearing tracking columns. To test deleted docs, call `upsert` then `mark_deleted()`.
- **LLM response parsing**: Always strip markdown fences (`` ```json ... ``` ``) before JSON parsing — LLMs commonly wrap JSON output.
- **Snippet truncation** (200 chars) in LLM prompts prevents context bloat with large documents.
- **`Serialize` derive** needed on any new struct that appears in JSON output (MCP responses, web API, etc.).
- **New fields on shared structs** (e.g., `AnalysisResults`) require updating all existing tests that construct the struct.

### Architecture Patterns
- `cmd_lint` is already `async fn` — no `Runtime::new()` or `block_on` needed for async operations within it.
- `search_semantic_paginated` is the standard search path (same as MCP `search_knowledge` tool).
- `McpServer::new` parameter additions go at the end for backward compatibility.
- `blocking_tool!` macro can be replaced with direct async calls when tool functions become async.
- `AppState` holds `Option<Box<dyn LlmProvider>>` — optional because tests use `None`.
- Link graph invalidation: when doc A changes, clear tracking hashes for docs that link TO A (via `get_links_to`).
- MCP protocol: `McpRequest.id` is `Option<Value>` (notifications have no id); HTTP returns 202 for notifications; stdio skips writing.
- Session management: `Mutex<Option<String>>` in AppState, UUID v4 via `getrandom`, `Mcp-Session-Id` header, 409 on mismatch.
- `protocol::initialize_result()` returns `serde_json::Value` — both transports wrap it in their own response format.
- Shared MCP code between transports: `handle_tool_call()`, `tools_list()`, `initialize_result()`.
- Web server lacks embedding provider — features requiring embeddings (split detection, duplicate entry detection) only work via MCP/CLI.

### Testing Patterns
- Mock embedding providers: `HashEmbedding` produces deterministic vectors from text hash for unit tests.
- Integration tests requiring inference backend are `#[ignore]` — run with `cargo test -- --ignored`.
- f32 precision in JSON: use `round()` comparison in tests, not exact equality.
- `models::temporal` and `models::question` are private modules — import via `crate::models::TemporalTagType` re-export path.

### Entity & Document Processing
- Entity entry extraction: two patterns — H3+ headings under H2 sections, and bold-name list items (`- **Name** - desc`). Entries without child facts are filtered out.
- Consecutive bold-name list items require finalizing previous entry before starting new one.
- Duplicate entry matching: exact normalized name grouping first, then embedding-based fuzzy matching (0.85 cosine threshold) for singletons.
- Three-layer filtering for duplicates: cross-reference-only entries, self-mentions, authoritative doc exclusion.
- Staleness determination: Ongoing → today, LastSeen/PointInTime → start_date, Range/Historical → end_date; fall back to `file_modified_at`. Same-date entries not flagged; no-date entries skipped.
- Cross-document fact validation: per-fact embeddings, top 10 search results (excluding source doc), 0.3 relevance threshold, LLM classifies as CONSISTENT/CONFLICT/STALE/UNCERTAIN.
- `cross_check_hash` column tracks which docs need re-checking; cleared for linked docs when source changes.

### Current Test Counts
- Lib tests (default features): 950
- Bin tests (default features): 348
- Lib tests (all features incl. web): 1011
- Bin tests (all features incl. web): 355
- Integration tests: 73+
- Frontend tests: 56

---

## No active phase

All phases through 46 are complete. Awaiting next phase definition.
