# Factbase Tasks

## Project Status

**Phases 1-46 complete, Phase 47 in progress (tasks 36-38 remaining)**. Releases: v0.1.0 through v0.4.3. Current Cargo.toml version: v0.7.6.

## Key Learnings from Past Phases (1-46, Phase 47 tasks 1-35)

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
- `ProgressReporter` enum (`Cli`/`Mcp`/`Silent`) provides unified progress reporting for both MCP and CLI code paths — CLI writes to stderr, MCP sends JSON via channel
- `ProgressSender` type alias lives in `progress.rs`, re-exported from `mcp/tools/mod.rs` for backward compatibility
- Wiring pattern: MCP tools construct `ProgressReporter::Mcp { sender }`, CLI commands construct `ProgressReporter::Cli { quiet }`, background operations (watcher) use `ProgressReporter::Silent`
- Progress throttling: `report()` fires every N items (e.g., every 25 files when ≥50 total) to avoid per-item overhead
- Integration tests use `&ProgressReporter::Silent` for all `full_scan` calls
- Shared lint function: `lint_all_documents()` in `question_generator/lint.rs` with `LintConfig` struct — both MCP `lint_repository` and CLI `cmd_lint` delegate to this shared function
- Shared apply function: `apply_all_review_answers()` in `answer_processor/apply_all.rs` with `ApplyConfig` struct — both MCP and CLI delegate to this shared function
- When extracting shared functions from MCP tools, use a config struct (e.g., `LintConfig`, `ApplyConfig`) to bundle parameters cleanly
- Shared function returns a result struct (e.g., `ApplyResult` with per-document `ApplyDocResult`) so callers can format output differently (JSON for MCP, human-readable for CLI)
- Early termination pattern: when paginated results fill the limit, break out of the loop and set `has_more: true` — but web stats endpoints that need accurate totals should bypass via high limit

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
- Single `pub(crate) type BoxFuture` in `lib.rs` — manual desugaring replaces `async-trait` crate (native async fn in traits doesn't support dyn dispatch)
- Replace redundant closures with method references: `|v| v.as_u64()` → `Value::as_u64`, `|s| s.to_string()` → `ToString::to_string` (clippy pedantic `redundant_closure_for_method_calls`)
- `cargo clippy --fix` can auto-fix bin-crate closures but not lib-crate ones — manual replacement needed for lib crate

### Testing Patterns
- Lib-crate test helpers: `database/mod.rs` (`test_db`, `test_repo_in_db`, `Document::test_default`)
- Binary-crate test helpers: `commands/test_helpers.rs` (`test_db`, `make_test_repo`, `make_test_doc`)
- Binary crate CANNOT access `pub(crate)` items from lib crate — separate test helper copies required
- `super::*` in test modules doesn't bring in parent's `use` imports — explicit imports needed
- `#[cfg(test)]` gated shared helper modules (e.g., `organize/test_helpers.rs`) for cross-module test dedup
- Shared mock providers: `embedding::test_helpers` (`MockEmbedding`, `HashEmbedding`), `llm::test_helpers` (`MockLlm`)
- `use ... as alias` pattern to avoid changing call sites when consolidating helpers
- f32 precision in JSON: use `round()` comparison in tests, not exact equality
- `TestScanSetup` struct in integration tests owns Config/Scanner/Processor/Embedding/LinkDetector/ScanOptions; `context(&self) -> ScanContext<'_>` borrows all fields — replaced ~17 setup blocks (-252 lines)
- When moving fields out of a test setup struct, check if the type is `Clone` (e.g., `OllamaEmbedding` is Clone)
- `test_scan_options()` and `test_scan_options_skip_links()` helpers in `tests/common/mod.rs` replace verbose `ScanOptions { ... }` initializers
- `test_repo(id, path)` helper in `tests/common/mod.rs` replaces ~60 verbose `Repository { ... }` initializers — capitalizes first letter of id for name, fills standard defaults
- Cases with custom perspectives or non-standard fields should be left as manual construction
- In integration tests, `.unwrap()` is idiomatic — generic `.expect("operation should succeed")` adds no diagnostic value over `.unwrap()` since panic messages include file/line

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
- `blocking_tool!` macro for sync MCP tool dispatch: 2-arg `($db, $args, $fn)` for simple tools, 3-arg `($db, $args, $reporter, $fn)` for tools with progress reporting
- `ProgressReporter` derives `Clone` — `UnboundedSender` is Clone, so all variants are cloneable
- Construct `ProgressReporter::Mcp { sender: progress }` once at top of `handle_tool_call` dispatch, not per-tool
- Promote internal helpers to `pub` free functions + re-export through `lib.rs` when integration tests need them (e.g., `content_hash()`, `cosine_similarity()`)
- Use `#[deprecated(note = "use X instead")]` when keeping old method as a thin wrapper during migration — migrate all callers first, then deprecate the old method, then remove
- When extracting a free function from a struct method (e.g., `DocumentProcessor::compute_hash` → `content_hash()`), prefer free functions when the operation doesn't need `&self`

### FTS5 Full-Text Search (Phase 47)
- `document_content_fts` FTS5 virtual table with `doc_id UNINDEXED` + `content` columns for O(log N) content search
- `is_fts_compatible()` checks if pattern is simple (alphanumeric, spaces, hyphens, underscores, apostrophes); if yes, uses FTS5 MATCH; falls back to LIKE for regex/special chars
- `fts5_phrase()` wraps pattern in double quotes for safe FTS5 phrase matching
- FTS5 does word-level matching (not substring like LIKE) — searching "OD" won't match "TODO" (intentional trade-off for performance)
- Schema version 7: migration creates FTS5 table and backfills via `backfill_fts5()`
- `upsert_document()` and `update_document_content()` keep FTS5 in sync (DELETE + INSERT on each write)
- `mark_deleted()` and `hard_delete_document()` also delete from FTS5 — all write paths now keep FTS5 consistent
- `ContentSearchParams<'a>` struct bundles 7 search parameters to avoid `clippy::too_many_arguments` — pattern: bundle related function parameters into a struct when count exceeds 7
- `ScanContext<'a>` struct bundles scanner/processor/embedding/link_detector/opts/progress to reduce `full_scan` from 8 params to 3 — same bundling pattern for dependency injection

### Clone Reduction & Module Splitting (Phase 47)
- Avoid cloning large content in DB write paths: `let compressed; let content_to_store: &str = if compression { compressed = encode(...); &compressed } else { &content };` — temporary outlives the reference
- Split large files (800+ lines) into directory modules with focused submodules (crud, list, batch) following `database/search/` and `database/stats/` patterns
- `super::super::` paths in submodules reach parent module helpers; no external import changes needed since all methods remain on the same struct impl
- When extracting a submodule (e.g., `database/compression.rs`), re-export via `pub(crate) use submodule::{...}` from mod.rs so existing `super::` paths in sibling submodules continue to work unchanged

### String Formatting (Phase 47)
- `write!(string, ...)` / `writeln!(string, ...)` replaces `push_str(&format!(...))` — avoids intermediate String allocation
- `write_str!` / `writeln_str!` macros in `lib.rs` wrap `write!`/`writeln!` with `use std::fmt::Write as _;` and `.expect("write to String infallible")` — eliminates boilerplate and scattered `use std::fmt::Write` imports
- `#[macro_export]` macros defined before module declarations are automatically available in the lib crate; bin crate needs explicit `use factbase::writeln_str;` import
- `cargo clippy --fix -- -W clippy::uninlined_format_args` auto-fixes `format!("{}", x)` → `format!("{x}")` in both lib and bin crates

### Idiomatic Rust Patterns (Phase 47)
- `map_or()`/`map_or_else()` replaces `map().unwrap_or()`/`map().unwrap_or_else()` — single method call instead of chain
- `is_some_and()` replaces `map_or(false, ..)`, `is_none_or()` replaces `map_or(true, ..)` for boolean Option checks
- `let...else` replaces `match` with early `continue`/`return` for cleaner control flow
- Replace redundant closures with method references: `|v| v.as_u64()` → `Value::as_u64`, `|s| s.to_string()` → `ToString::to_string` (clippy pedantic `redundant_closure_for_method_calls`)
- `cargo clippy --fix` can auto-fix bin-crate closures but not lib-crate ones — manual replacement needed for lib crate

### Entity Deduplication (Phase 46)- Two extraction patterns for entity entries: H3+ headings under H2 sections, and bold-name list items (`- **Name** - desc`)
- Entries without child facts are filtered out (headings alone don't constitute an entity entry)
- Consecutive bold-name list items: must finalize previous entry before starting new one
- Two-phase matching: (1) exact normalized name grouping (lowercase, trim, collapse whitespace), (2) embedding-based fuzzy matching with 0.85 cosine similarity threshold
- Only flag entries appearing in 2+ different documents — same-document entries are not duplicates
- Three-layer filtering: cross-reference-only entries (`[[id]]`), self-mentions (entry name = doc title), authoritative doc exclusion (title→doc_id map)
- Staleness determination: Ongoing tags → today, LastSeen/PointInTime → start_date, Range/Historical → end_date; fall back to `file_modified_at` when no temporal tags
- `generate_stale_entry_questions()` returns `HashMap<doc_id, Vec<ReviewQuestion>>` for downstream injection
- `models::temporal` and `models::question` are private modules — import via `crate::models::TemporalTagType` re-export path
- Same-date entries are not flagged as stale (no false positives)
- Entries with no determinable date on any entry are skipped entirely

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
- Threading optional providers through MCP: `AppState` holds `Option<Box<dyn Provider>>`, handlers receive `Option<&dyn Provider>`, graceful degradation when `None`
- Async MCP tools (e.g., `generate_questions` with cross-validation) use direct async dispatch instead of `blocking_tool!` macro

### Build & CI
- Feature flags: `full` (default) = progress + compression + mcp + bedrock; `web` separate
- CI: 7 jobs — clippy (all-features), test (default + bin), test-web, test-features matrix (5 combos), readme-validation
- Binary sizes: no-features 6.7MB, +progress +0.1MB, +compression +0.6MB, +mcp +1MB, +bedrock +7MB, full 16MB, +web +1MB

## Active Work

- [ ] Phase 47: Long-Running Operation Progress & Optimization — see [tasks/phase47.md](tasks/phase47.md)
  - Depends on Phase 46 (complete)
  - Tasks 1-35: Complete
  - Tasks 36-38: Remaining (clippy pedantic cleanup)
    - Task 36: Consolidate identical match arm bodies (10 instances)
    - Task 37: Fix case-sensitive file extension comparisons (8 instances)
    - Task 38: Convert unused-`self` methods to free/associated functions (8 instances)

## Future Considerations

- MCP tools for organize operations (deferred from Phase 10)
- Additional inference provider backends
