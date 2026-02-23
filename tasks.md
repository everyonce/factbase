# Factbase Tasks

## Project Status

**Phases 1-45 complete (650+ tasks)**. Phase 46 pending. Releases: v0.1.0 through v0.4.3.

## Key Learnings from Past Phases (1-45)

### Architecture & Patterns
- Filesystem is truth — markdown files on disk are authoritative, SQLite is the index
- Two-phase scanning: index documents + embeddings (pass 1), detect links via LLM (pass 2)
- `EmbeddingProvider` and `LlmProvider` traits allow swapping backends (Bedrock default, Ollama alternative)
- `cfg!(feature = "bedrock")` in default functions enables compile-time provider switching
- Database uses `r2d2` connection pool for thread-safe access across watcher thread and MCP server
- MCP server supports both stdio transport (`factbase mcp`) and Streamable HTTP (`factbase serve`)
- MCP protocol: `McpRequest.id` is `Option<Value>` (notifications have no id); HTTP returns 202 for notifications; stdio skips writing
- Session management: `Mutex<Option<String>>` in AppState, UUID v4 via `getrandom`, `Mcp-Session-Id` header, 409 on mismatch
- `protocol::initialize_result()` returns `serde_json::Value` — both transports wrap it in their own response format

### Code Quality Conventions
- `FactbaseError` enum with `thiserror` v2 — use constructor helpers: `::parse()`, `::not_found()`, `::internal()`, `::config()`, `::embedding()`, `::llm()`, `::ollama()`
- `anyhow::bail!` replaces `process::exit()` and `return Err(anyhow::anyhow!(...))`; `.context()`/`.with_context()` replaces `.map_err(|e| anyhow::anyhow!(...))`
- `unwrap_or_default()` on `row.get()` silently swallows DB errors — always use `?` instead
- `prepare_cached()` on ALL database paths (migration complete across all modules)
- `require_document(id)` and `require_repository(id)` consolidate get+ok_or patterns
- Column constants (`DOCUMENT_COLUMNS`, `SEARCH_COLUMNS`, `REPOSITORY_COLUMNS`) prevent mismatch bugs — only when column list is exact match
- `decode_content_lossy()` consolidates fallback decode pattern across search and stats modules
- Dynamic SQL params: `Vec<&dyn ToSql>` building is clearer than combinatorial match dispatch
- `append_type_repo_filters()` + `push_type_repo_params()` for dynamic WHERE clause building in search modules
- Validation functions return `anyhow::Result<()>` with `anyhow::bail!` — avoids `.map_err(anyhow::Error::msg)?` boilerplate
- Declarative config validation: `require_non_empty`, `require_positive`, `require_range` helpers in `config/validation.rs`
- `pub(crate)` for internal-only modules; `pub` only for items re-exported from `lib.rs`
- `#[cfg(test)]` gating preferred over `#[allow(dead_code)]` for test-only items
- Module-level `#![allow(dead_code)]` for entire operational utility modules planned for future use
- No `unwrap()` in production code — use `expect("reason")` or pattern matching

### Testing Patterns
- Lib-crate test helpers: `database/mod.rs` (`test_db`, `test_repo_in_db`, `Document::test_default`)
- Binary-crate test helpers: `commands/test_helpers.rs` (`test_db`, `make_test_repo`, `make_test_doc`)
- Binary crate CANNOT access `pub(crate)` items from lib crate — separate test helper copies required
- `super::*` in test modules doesn't bring in parent's `use` imports — explicit imports needed
- `#[cfg(test)]` gated shared helper modules (e.g., `organize/test_helpers.rs`) for cross-module test dedup
- `use ... as alias` pattern to avoid changing call sites when consolidating helpers
- f32 precision in JSON: use `round()` comparison in tests, not exact equality

### Deduplication Patterns
- `to_json()` / `to_summary_json()` methods on model structs for consistent JSON serialization
- `::new()` constructors for structs with repeated default fields (e.g., `ReviewQuestion::new()`)
- `as_str() -> &'static str` methods replace standalone conversion functions
- `from_config(&Config)` constructors consolidate repeated field mapping
- `setup_database_only()` eliminates config discards; `setup_db_and_resolve_repos()` for common command setup
- `setup_cached_embedding(&config, timeout)` consolidates embedding+cache setup
- `resolve_repos()` shared helper for repo filtering across commands
- `confirm_prompt()` and `execute_with_snapshot()` for CLI command patterns
- `read_file`/`write_file`/`remove_file`/`copy_file`/`create_dir`/`remove_dir` in `organize/fs_helpers.rs`
- `load_orphan_entries(repo_path)` consolidates orphan file loading pattern
- Centralize all static `LazyLock<Regex>` patterns in `src/patterns.rs`
- `word_count()` free function in `models/document.rs` for repeated `split_whitespace().count()`
- `collect_active_documents()`, `cosine_similarity()`, `get_document_embedding()`, `compute_centroid()` in `organize/detect/mod.rs`
- `fetch_active_doc_content()` and `CONTENT_ONLY_QUERY` in `database/stats/mod.rs`
- `parse_since_filter()` in `commands/utils.rs` for date filter parsing
- `DbConn` type alias and `B64` constant in `database/mod.rs`
- `blocking_tool!` macro for sync MCP tool dispatch

### Dependency Management
- `tokio` features: only `rt-multi-thread`, `macros`, `net`, `signal`, `sync`, `time` (not `full`)
- `reqwest` 0.12 with `rustls-tls` (not `native-tls`) to align with AWS SDK
- `getrandom` replaces `rand` for document ID generation
- `tokio::signal::ctrl_c()` replaces `ctrlc` crate
- Manual debouncing replaces `notify-debouncer-mini` (recv + recv_timeout + HashSet dedup)
- Manual `BoxFuture` desugaring replaces `async-trait` crate (native async fn in traits doesn't support dyn dispatch)
- `lto = "fat"` + `opt-level = "z"` for binary size optimization (36% reduction)
- Simple sliding-window rate limiter replaces `tower_governor` (Arc<Mutex<VecDeque<Instant>>>)
- `thiserror` v2 (API-compatible with v1), `dirs` v6, stdlib recursive walk replaces `walkdir`
- Check `cargo tree --duplicates` before/after dependency changes

### Review System & Cross-Validation
- Before generating a question, check `has_recent_verification()` for `@t[~DATE]` within 180 days
- Lines with `[[id]]` cross-references are roster entries, not conflicting facts
- Multi-pass processing: re-read files from disk between passes when earlier pass may modify content
- Cross-validation: `extract_all_facts()` gets ALL list items (any indent level) with section tracking
- `clean_fact_text()` does NOT truncate (unlike `extract_fact_text()` which caps at 80 chars) — full text needed for embeddings
- Per-fact semantic search with `RELEVANCE_THRESHOLD = 0.3` filters before LLM calls
- LLM conflict detection: batch 10 facts per call, truncate snippets to 200 chars, strip markdown fences from JSON responses
- Graceful degradation: log and skip malformed LLM responses per batch, don't fail entire document
- `cmd_lint` is already `async fn` — cross-check wired directly, no `Runtime::new()` needed
- Cross-check runs as separate pass AFTER existing checks for clean separation
- `cross_check_hash` stores `file_hash` value — comparing `cross_check_hash == file_hash` is sufficient for skip logic
- `upsert_document()` INSERT OR REPLACE resets `cross_check_hash` to NULL for changed documents (correct behavior)
- `set_cross_check_hash()` runs after successful validation regardless of whether questions were generated — documents with zero conflicts still get marked as checked
- Failed cross-validations do NOT update the hash, so those documents will be retried on the next run
- `--dry-run` skips hash update so users can preview cross-check results without marking documents as checked
- Task 5.3 pattern: when a document changes, use `db.get_links_to(doc_id)` to find documents linking TO it, then `clear_cross_check_hashes()` on those IDs to force re-cross-checking

### MCP Tools
- Schema is the contract — when schema defines `"doc_type"` but handler reads `"type"`, agents silently get no filtering
- Standardize all type filter params to `"doc_type"` across all tools
- Schema/dispatch consistency tests prevent drift between tool definitions and handlers

### Build & CI
- Feature flags: `full` (default) = progress + compression + mcp + bedrock; `web` separate
- CI: 7 jobs — clippy (all-features), test (default + bin), test-web, test-features matrix (5 combos), readme-validation
- Binary sizes: no-features 6.7MB, +progress +0.1MB, +compression +0.6MB, +mcp +1MB, +bedrock +7MB, full 16MB, +web +1MB

## Active Work

- [x] Phase 45: Cross-Document Fact Validation — see [tasks/phase45.md](tasks/phase45.md)
  - Task 1 complete (fact extraction expansion)
  - Task 2 complete (per-fact semantic search)
  - Task 3 complete (LLM conflict detection)
  - Task 4 complete (lint integration with `--cross-check` flag)
  - Task 5 complete (5.1 schema migration, 5.2 hash update after validation, 5.3 linked doc invalidation)
  - Task 6 complete (MCP and workflow integration)
- [ ] Phase 46: Cross-Document Entity Deduplication — see [tasks/phase46.md](tasks/phase46.md)
  - Depends on Phase 45
  - All tasks pending

## Future Considerations

- MCP tools for organize operations (deferred from Phase 10)
- Additional inference provider backends
