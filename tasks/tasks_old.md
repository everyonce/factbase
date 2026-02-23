# Completed Phases

## Phase 45: Cross-Document Fact Validation (COMPLETE)

Spec: [tasks/cross-conflict.md](cross-conflict.md)

### Summary
Added cross-document fact validation to the lint command. Every fact line in a document gets validated against the rest of the factbase using semantic search + LLM judgment. Generates `@q[conflict]` and `@q[stale]` review questions with cross-document citations.

### Tasks Completed
- **Task 1**: Fact extraction expansion — `extract_all_facts()` in `question_generator/facts.rs` extracts ALL list items (not just temporally-tagged). `FactLine` includes line_number, text, section heading. 21 unit tests.
- **Task 2**: Per-fact semantic search — `cross_validate_document()` in `question_generator/cross_validate.rs`. Generates per-fact embeddings, searches top 10 results (excluding source doc), filters by 0.3 relevance threshold. Uses `search_semantic_paginated`.
- **Task 3**: LLM conflict detection — Prompt template batches 10 facts per LLM call. Parses JSON response (with markdown fence stripping). Classifies as CONSISTENT/CONFLICT/STALE/UNCERTAIN. Generates `ReviewQuestion` for CONFLICT and STALE results.
- **Task 4**: Integration into lint — `--cross-check` flag on `factbase lint`. Runs as separate async pass after existing generators. `cmd_lint` is already async so no `block_on` needed. Progress via stderr `eprint!`.
- **Task 5**: Re-check tracking — `cross_check_hash` column (schema migration v5). Skip unchanged docs. Clear hash for linked documents when source changes (in `full_scan` after commit). `set_cross_check_hash()`, `needs_cross_check()`, `clear_cross_check_hashes()` DB methods.
- **Task 6**: MCP and workflow integration — Threaded `LlmProvider` through MCP `AppState`. `generate_questions` tool now runs cross-validation when LLM available. Resolve workflow updated to guide agents on cross-document conflicts.

### Key Learnings
- `cmd_lint` is already `async fn` — no need for `Runtime::new()` or `block_on` for async operations.
- `upsert_document()` does INSERT OR REPLACE which resets nullable columns not in the INSERT list — useful for auto-clearing `cross_check_hash`.
- `search_semantic_paginated` is the standard search path (same as MCP `search_knowledge`).
- LLM responses commonly wrap JSON in markdown fences — always strip `\`\`\`json ... \`\`\`` before parsing.
- Snippet truncation (200 chars) in prompts prevents bloat with large documents.
- Graceful degradation pattern: log warnings for individual failures, don't fail the whole batch/document.
- `--dry-run` should prevent state changes (hash updates) but still show results.
- Link graph invalidation: when doc A changes, clear cross_check_hash for docs that link TO A (not from A).
- `McpServer::new` parameter additions go at the end for backward compatibility.
- `blocking_tool!` macro can be replaced with direct async calls when the tool function becomes async.

### Commits
d24731d, 2696326, 18db85c, 41aac04, 21171e1, d8bd664, d899ec5, 27b2f7b

## Phase 41: MCP Transport Compliance (COMPLETE)

Spec: [tasks/mcp-transport.md](mcp-transport.md)

### Summary
Added both standard MCP transports (stdio and Streamable HTTP) so MCP clients like kiro-cli can connect to factbase.

### Tasks Completed
- **Task 1**: Stdio transport — `factbase mcp` subcommand in `src/mcp/stdio.rs` + `src/commands/mcp.rs`. Reads newline-delimited JSON-RPC from stdin, writes to stdout. Handles `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping`. Logging to stderr only.
- **Task 2**: Streamable HTTP transport — Upgraded `factbase serve` endpoint. Added `initialize` handling, notification support (202 Accepted), GET 405, session management (`Mcp-Session-Id` header with UUID, 409 on mismatch). Made `McpRequest.id` optional (`Option<Value>`).

### Key Learnings
- MCP protocol: `McpRequest.id` is `Option<Value>` — notifications have no id; HTTP returns 202 for notifications; stdio skips writing response.
- Session management: `Mutex<Option<String>>` in AppState, UUID v4 via `getrandom`, `Mcp-Session-Id` header, 409 on mismatch.
- `protocol::initialize_result()` returns `serde_json::Value` — both transports wrap it in their own response format.
- Shared code between transports: `handle_tool_call()`, `tools_list()`, `initialize_result()`.

## Phase 46: Cross-Document Entity Deduplication (COMPLETE)

Spec: [tasks/phase46.md](phase46.md)

### Summary
Added cross-document entity deduplication to detect the same entity appearing as entries within multiple parent documents (e.g., Jane Smith listed under both `companies/acme.md` and `companies/globex.md`). Determines which entries are stale and generates review questions.

### Tasks Completed
- **Task 1**: Entity entry extraction — `extract_entity_entries()` in `organize/detect/entity_entries.rs`. Two patterns: H3+ headings under H2 sections, and bold-name list items. Entries without child facts filtered out. 9 unit tests.
- **Task 2**: Cross-document entry matching — `detect_duplicate_entries()` in `organize/detect/duplicate_entries.rs`. Two-phase matching: exact normalized name grouping + embedding-based fuzzy matching (0.85 threshold). Filters cross-references, self-mentions, and authoritative doc entries. 13 unit tests.
- **Task 3**: Staleness determination — `assess_staleness()` in `organize/detect/staleness.rs`. Temporal tag recency + `file_modified_at` fallback. `generate_stale_entry_questions()` for review questions. 14 unit tests.
- **Task 4**: Integration — Wired into `factbase organize analyze` with inline `[CURRENT]`/`[STALE]` tags. Added `get_duplicate_entries` MCP tool (19th tool). Web UI types and rendering added but detection requires embedding provider (not available in web server — same limitation as split detection).

### Key Learnings
- Two extraction patterns: H3+ headings under H2 sections, and bold-name list items (`- **Name** - desc`).
- Consecutive bold-name list items require finalizing previous entry before starting new one.
- Two-phase matching: exact normalized name grouping first, then embedding-based fuzzy matching (0.85 cosine threshold) for singletons.
- Three-layer filtering: cross-reference-only entries, self-mentions, authoritative doc exclusion.
- Staleness: Ongoing → today, LastSeen/PointInTime → start_date, Range/Historical → end_date; fall back to `file_modified_at`. Same-date entries not flagged; no-date entries skipped.
- `models::temporal` and `models::question` are private modules — import via `crate::models::TemporalTagType` re-export path.
- `Serialize` derive needed on new structs that appear in JSON output.
- New fields on `AnalysisResults` require updating existing tests that construct the struct.
- Web server lacks embedding provider — features requiring embeddings (split detection, duplicate entry detection) only work via MCP/CLI.
- Mock embedding: `HashEmbedding` produces deterministic vectors from text hash for unit tests.
- `upsert_document()` hardcodes `is_deleted = FALSE` — test deleted docs by calling `upsert` then `mark_deleted()`.

### Commits
373fbfd, c3a6db0, f34b359, 342253c, ab02da2, 8fac147, aac845d, fce3d54
