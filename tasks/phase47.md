# Phase 47: Long-Running Operation Progress & Optimization

Depends on: Phase 46 (complete). Builds on existing ad-hoc progress in `scan_repository` (10s timer + `ProgressSender`) and `lint_repository` (per-doc `eprintln!` + `ProgressSender`).

## Problem

Many functions are shared between MCP tools and CLI commands but have no user feedback when operating on large datasets. At 1,000+ documents, operations like scan, lint, grep, review, organize, and export can run for minutes with no indication of progress. MCP and CLI currently use different ad-hoc progress mechanisms (`ProgressSender` + `eprintln!` vs `OptionalProgress` indicatif bars), violating DRY.

## Goal

1. Unified `ProgressReporter` abstraction usable by both MCP and CLI code paths
2. Every function that iterates over documents or makes inference calls reports incremental progress
3. Optimizations for the worst-scaling operations

---

## Task 1: ProgressReporter abstraction

Create `src/progress.rs` with a single type that both MCP and CLI callers construct differently but library code uses uniformly.

- [x] 1.1 Create `src/progress.rs` with `ProgressReporter` enum: `Cli { quiet: bool }`, `Mcp { sender: Option<ProgressSender> }`, `Silent`. Implement three methods: `report(&self, current: usize, total: usize, message: &str)` — emits progress update; `phase(&self, name: &str)` — emits phase transition; `log(&self, message: &str)` — emits general status line. CLI variant writes to stderr (not stdout, to preserve JSON output). MCP variant calls `eprintln!` AND sends JSON via `ProgressSender` if present. Silent is no-op. Register module in `lib.rs`, re-export `ProgressReporter`.
- [x] 1.2 Move `ProgressSender` type alias from `mcp/tools/mod.rs` into `progress.rs` so it's available to non-MCP code. Re-export from `mcp/tools/mod.rs` for backward compatibility.
- [x] 1.3 Unit tests: `Silent` variant produces no output. `Cli { quiet: true }` produces no output. Verify `Mcp` variant sends JSON with `progress`, `total`, `message` fields to the channel. Verify `phase()` sends a message with `phase` field.

## Task 2: Wire ProgressReporter into scan

Replace the ad-hoc progress in `full_scan` and `scan_repository` MCP tool.

- [x] 2.1 Add `progress: &ProgressReporter` parameter to `full_scan()` in `scanner/orchestration/mod.rs`. Replace the inline `info!("Progress: {}/{}", ...)` fallback logging with `progress.report()`. Keep `OptionalProgress` (indicatif) for the TTY progress bar — `ProgressReporter` handles the non-TTY / MCP path. Call `progress.phase("Indexing documents")` before pass 1 and `progress.phase("Detecting links")` before pass 2.
- [x] 2.2 Update `scan_repository` MCP tool (`mcp/tools/repository.rs`): construct `ProgressReporter::Mcp { sender: progress }` and pass to `full_scan()`. Remove the manual `tokio::select!` 10-second timer loop — `full_scan` now reports progress directly.
- [x] 2.3 Update `cmd_scan` CLI (`commands/scan/mod.rs`): construct `ProgressReporter::Cli { quiet }` and pass to `full_scan()`. The existing `OptionalProgress` bar remains for TTY; `ProgressReporter` adds non-TTY feedback.
- [x] 2.4 Similarly update `scan_all_repositories()` to accept and forward `ProgressReporter`. Add per-repo phase reporting: `progress.phase(&format!("Scanning repository '{}'", repo.id))`.

## Task 3: Wire ProgressReporter into lint

Replace the ad-hoc `eprintln!` + `ProgressSender` in lint.

- [x] 3.1 Refactor `lint_repository` MCP tool (`mcp/tools/review/lint.rs`): accept `&ProgressReporter` instead of `Option<ProgressSender>`. Replace `eprintln!("Linting [{}/{}] {}", ...)` and manual `tx.send()` with `progress.report(idx, total, &doc.title)`.
- [x] 3.2 Extract the core lint-all-documents loop from `lint_repository` into a shared function in `src/lint.rs` (or `src/question_generator/lint.rs`) that accepts `&ProgressReporter`. Both the MCP tool and `cmd_lint` CLI call this shared function.
- [x] 3.3 Update `cmd_lint` CLI (`commands/lint/mod.rs`): construct `ProgressReporter::Cli { quiet }` and pass to the shared lint function. Replace the existing `#[cfg(feature = "progress")] ProgressBar` usage with the shared function's built-in reporting.
- [x] 3.4 Add phase reporting within the shared lint function: `progress.phase("Generating review questions")`, and if cross-check enabled: `progress.phase("Cross-document validation")`.

## Task 4: Wire ProgressReporter into review apply

`apply_review_answers` (MCP) and `cmd_review_apply` (CLI) both iterate documents with answered questions and call LLM per question.

- [x] 4.1 Extract the core apply loop from `mcp/tools/review/apply.rs` into a shared function (e.g., `src/answer_processor/apply_all.rs`) that accepts `db`, `llm`, filter params, `dry_run`, and `&ProgressReporter`. Returns the same result struct.
- [x] 4.2 Wire MCP `apply_review_answers` to call the shared function with `ProgressReporter::Mcp`.
- [x] 4.3 Wire CLI `cmd_review_apply` to call the shared function with `ProgressReporter::Cli`. Remove the inline per-document printing.
- [x] 4.4 In the shared function, report: `progress.report(i, total_docs, &format!("Applying {} questions to {}", count, doc.title))` per document.

## Task 5: Wire ProgressReporter into get_review_queue

`get_review_queue` loads all documents with review queues and parses each. At scale this is slow due to content decompression + parsing.

- [x] 5.1 Add `progress: &ProgressReporter` parameter to `get_review_queue`. Report `progress.log(&format!("Processing {} documents with review queues", docs.len()))` before the loop, and `progress.report(i, total, &doc.title)` every 50 documents (avoid per-doc overhead for small sets).
- [x] 5.2 Optimization: add early termination — once `all_questions.len() >= limit` AND we've counted all status totals, stop parsing remaining documents. Currently it parses ALL docs even when paginated with `limit=10`.
- [x] 5.3 Wire through from MCP `handle_tool_call` (currently calls `get_review_queue(db, &args)?` synchronously — add progress parameter).

## Task 6: Wire ProgressReporter into search_content / grep

`search_content` does `LIKE %pattern%` which is a full table scan with content decompression.

- [x] 6.1 Add `progress: &ProgressReporter` parameter to `Database::search_content()`. Before executing the query, report `progress.log("Searching document content...")`. This is a single SQL query so per-row progress isn't practical, but the "searching..." indicator tells the user it's working.
- [x] 6.2 Wire through from MCP `search_content` tool and CLI `cmd_grep`.
- [x] 6.3 Optimization: add a `document_content_fts` FTS5 virtual table for full-text search. Create in `database/schema.rs` migration. Populate during scan (alongside document upsert). Use FTS5 `MATCH` instead of `LIKE` when the pattern is a simple word/phrase (no regex). Fall back to `LIKE` for regex patterns. This changes `search_content` from O(N) table scan to O(log N) index lookup.
- [x] 6.4 Update `mark_deleted()` and `hard_delete_document()` to remove from FTS5.

## Task 7: Wire ProgressReporter into organize analyze

`organize analyze` runs 5 detection algorithms sequentially, several of which iterate all documents.

- [x] 7.1 Add `progress: &ProgressReporter` parameter to `detect_merge_candidates()`. Report `progress.phase("Detecting merge candidates")` at start, `progress.report(i, total, &doc.title)` per document.
- [x] 7.2 Add `progress: &ProgressReporter` parameter to `detect_split_candidates()`. Report phase + per-doc progress. This is the slowest (generates embeddings per section).
- [x] 7.3 Add `progress: &ProgressReporter` parameter to `detect_misplaced()`. Report phase + per-doc progress.
- [x] 7.4 Add `progress: &ProgressReporter` parameter to `detect_duplicate_entries()`. Report phase, then per-doc progress during extraction, then "Matching N entries..." during fuzzy matching.
- [x] 7.5 Update `organize analyze` CLI command and `get_duplicate_entries` MCP tool to construct and pass `ProgressReporter`.
- [x] 7.6 Add top-level phase reporting in `organize analyze`: `progress.phase("Analysis 1/4: Merge candidates")`, etc.

## Task 8: Wire ProgressReporter into export/import

- [x] 8.1 Add `progress: &ProgressReporter` parameter to `export_json()`, `export_markdown_stdout()`, `export_markdown_dir()`. Report `progress.report(i, total, &doc.title)` per document.
- [x] 8.2 Add `progress: &ProgressReporter` parameter to `import_json()`, `import_directory()`. Report per-file progress.
- [x] 8.3 Wire through from `cmd_export` and `cmd_import` CLI commands with `ProgressReporter::Cli`.

## Task 9: Wire ProgressReporter into bulk MCP operations

- [x] 9.1 Add `progress: &ProgressReporter` to `bulk_create_documents()`. Report per-document progress during the write loop (only meaningful when approaching the 100-doc cap).
- [x] 9.2 Add `progress: &ProgressReporter` to `answer_questions()` (bulk answer). Report per-question progress.

## Task 10: Update handle_tool_call dispatch

- [x] 10.1 Update `handle_tool_call` in `mcp/tools/mod.rs` to construct `ProgressReporter::Mcp { sender: progress.clone() }` once and pass it to all tool functions that accept it. Currently only `lint_repository` and `scan_repository` receive `progress` — extend to all tools wired in Tasks 2-9.
- [x] 10.2 For tools that are called via `blocking_tool!` macro (sync functions), pass `ProgressReporter` as an additional captured variable. Update the macro or switch affected tools to direct async dispatch.

## Task 11: Cleanup and tests

- [x] 11.1 Remove the old `ProgressSender` usage from `lint_repository` and `scan_repository` — they now use `ProgressReporter` exclusively.
- [x] 11.2 Integration test: run `scan_repository` MCP tool with a `ProgressReporter::Mcp` and verify progress messages are received on the channel.
- [x] 11.3 Verify all existing tests pass (1366+ unit/binary tests). Fix any signature changes that break callers.
- [x] 11.4 Update `current-state.md` and `module-interactions.md` steering docs to document `progress.rs` module and the `ProgressReporter` pattern.

## Task 12: Introduce ContentSearchParams struct to fix clippy warnings

The three `search_content*` functions in `database/search/content.rs` each take 8 parameters, triggering `clippy::too_many_arguments`. Bundle them into a params struct.

- [x] 12.1 Create `ContentSearchParams` struct in `database/search/content.rs` with fields: `pattern`, `limit`, `doc_type`, `repo_id`, `context_lines`, `since`, `progress`. Update `search_content()`, `search_content_fts5()`, and `search_content_like()` to accept `&ContentSearchParams`. Update all callers (MCP `search_content` tool, CLI `cmd_grep`, tests).

## Task 13: Introduce ScanContext struct to reduce full_scan parameter count

`full_scan()` takes 8 parameters (repo, db, scanner, processor, embedding, link_detector, opts, progress) with `#[allow(clippy::too_many_arguments)]`. Bundle the "tools" into a context struct.

- [x] 13.1 Create `ScanContext` struct in `scanner/orchestration/mod.rs` (or `scanner/mod.rs`) bundling `scanner`, `processor`, `embedding`, `link_detector`, `opts`, `progress`. Update `full_scan()` to accept `repo`, `db`, and `&ScanContext`. Update all callers: `cmd_scan`, `scan_repository` MCP tool, `serve.rs` watcher, `scan_all_repositories()`, and 22+ integration test call sites. Remove the `#[allow(clippy::too_many_arguments)]`.

## Task 14: Fix remaining clippy suggestions

- [x] 14.1 Replace `global_idx % 25 == 0` with `global_idx.is_multiple_of(25)` in `scanner/orchestration/mod.rs` (clippy `manual_is_multiple_of` warning).

## Task 15: Fix integration test compilation errors

Integration tests (`tests/`) have 16 compilation errors from API changes in Phases 46-47 that were not propagated to integration test call sites. These are pre-existing and block `cargo test --test '*'`.

- [x] 15.1 Fix `ScanOptions` missing `link_batch_size` field in `tests/common/mod.rs` and `tests/cli/scan_tests.rs` — add the field to all `ScanOptions { ... }` initializers.
- [x] 15.2 Fix `McpServer::new()` arity mismatch in `tests/common/mod.rs`, `tests/serve_e2e.rs`, and `tests/performance.rs` — update to match current 7-argument signature in `mcp/server.rs`.
- [x] 15.3 Fix `detect_split_candidates()` and `detect_misplaced()` arity mismatches in `tests/ollama_integration.rs` — add missing `&ProgressReporter::Silent` arguments.
- [x] 15.4 Verify all integration tests compile: `cargo test --test '*' --no-run` succeeds with zero errors.

## Task 16: Consolidate duplicate `cosine_similarity` implementations

`cosine_similarity()` is defined in both `organize/detect/mod.rs` (lib crate) and `tests/common/mod.rs` (integration tests). The integration test copy should import from the lib crate instead.

- [x] 16.1 Make `cosine_similarity` in `organize/detect/mod.rs` `pub` (or re-export from `lib.rs`). Remove the duplicate from `tests/common/mod.rs` and update integration test imports.

## Task 17: Consolidate duplicate `compute_hash` / SHA256 helpers

`compute_hash()` (SHA256 of content) exists in `tests/common/mod.rs` and the same logic is inline in `scanner/orchestration/preread.rs`. Extract to a shared utility.

- [x] 17.1 Add a `pub fn content_hash(content: &str) -> String` to a shared location (e.g., `processor/core.rs` or a new `utils.rs`). Replace the inline SHA256 in `scanner/orchestration/preread.rs` and the test helper in `tests/common/mod.rs`.

## Task 18: Migrate callers from `DocumentProcessor::compute_hash` to `content_hash`

Now that `content_hash()` is a free function, migrate all call sites from the verbose `DocumentProcessor::compute_hash()` to the shorter `content_hash()`. This reduces coupling to `DocumentProcessor` for a utility that isn't processor-specific. ~14 production call sites + ~9 test call sites using `crate::processor::DocumentProcessor::compute_hash()`.

- [x] 18.1 Replace all `DocumentProcessor::compute_hash(...)` calls in production code (`scanner/`, `mcp/`, `commands/`, `question_generator/`) with `content_hash(...)`. Update imports accordingly.
- [x] 18.2 Replace all `crate::processor::DocumentProcessor::compute_hash(...)` calls in test code (`database/stats/temporal.rs`, `database/stats/sources.rs`) with `crate::content_hash(...)`.
- [x] 18.3 Deprecate or remove `DocumentProcessor::compute_hash()` method (it now just delegates to `content_hash()`). Update doc comments in `processor/mod.rs`.

## Task 19: Remove remaining `.unwrap()` calls from production code

Coding conventions say no `unwrap()` in production code. Three remain:
- `src/mcp/stdio.rs:114` — `progress_token.unwrap()`
- `src/mcp/tools/review/mod.rs:56` — `json.as_object_mut().unwrap()`
- `src/mcp/tools/review/apply.rs:46` — `json.as_object_mut().unwrap()`

- [x] 19.1 Replace each `.unwrap()` with `.expect("reason")` or proper error handling (`ok_or`/`?`).

## Task 20: Reduce verbose `crate::` paths in database stats test modules

`database/stats/temporal.rs` and `database/stats/sources.rs` tests use the verbose `crate::processor::DocumentProcessor::compute_hash(&doc.content)` pattern (9 occurrences). After Task 18, these will use `crate::content_hash()`, but the test modules could further benefit from a local `use crate::content_hash;` import at the top of the test module to eliminate even the `crate::` prefix.

- [x] 20.1 Add `use crate::content_hash;` to `#[cfg(test)]` modules in `database/stats/temporal.rs` and `database/stats/sources.rs`. Simplify all hash calls to `content_hash(&doc.content)`.

## Task 21: Remove deprecated DocumentProcessor::compute_hash method

Task 18 migrated all callers to the `content_hash()` free function and marked `DocumentProcessor::compute_hash()` as `#[deprecated]`. Zero callers remain in the codebase. Remove the deprecated method entirely.

- [x] 21.1 Remove `compute_hash` method from `DocumentProcessor` in `processor/core.rs` and its `#[deprecated]` annotation. Verify no callers exist with `cargo test --all-features`.

## Task 22: Fix redundant closures flagged by clippy pedantic

`clippy::redundant_closure` identifies ~26 closures that can be replaced with direct function references (e.g., `|e| Foo(e)` → `Foo`). Locations include `database/stats/`, `mcp/stdio.rs`, `mcp/tools/helpers.rs`, and `answer_processor/`.

- [x] 22.1 Replace redundant closures with function references across all flagged files. Run `cargo clippy --all-features -- -W clippy::redundant_closure` to verify zero remaining.

## Task 23: Use `write!`/`write_fmt` instead of `push_str(format!(...))` pattern

`clippy::format_push_string` identifies ~24 instances of `string.push_str(&format!(...))` which allocates an intermediate String. Replace with `write!(string, ...)` using `std::fmt::Write` for zero-allocation formatting.

- [x] 23.1 Replace `push_str(&format!(...))` with `write!(string, ...)` across flagged files (`database/documents.rs`, `database/search/`, `organize/`, etc.). Run `cargo clippy --all-features -- -W clippy::format_push_string` to verify zero remaining.

---

## Outcomes

### Task 19.1 — Remove remaining .unwrap() calls from production code (commit 24724e8)
- Replaced 3 production `.unwrap()` calls with descriptive `.expect()` messages:
  - `src/mcp/stdio.rs:114`: `progress_token.expect(...)` — guaranteed Some by surrounding if-let guard on tx channel
  - `src/mcp/tools/review/mod.rs:56`: `json.as_object_mut().expect(...)` — `to_json()` always returns a JSON object
  - `src/mcp/tools/review/apply.rs:46`: `json.as_object_mut().expect(...)` — `json!({})` macro always returns a JSON object
- Used `.expect("reason")` rather than full error handling since all three cases are structurally guaranteed to succeed
- 1031 lib + 355 bin tests passing, zero clippy warnings
- No difficulties encountered

### Task 17.1 — Consolidate duplicate compute_hash / SHA256 helpers
- Added `pub fn content_hash(content: &str) -> String` as a free function in `processor/core.rs`
- `DocumentProcessor::compute_hash()` now delegates to `content_hash()` — all existing call sites unchanged
- Re-exported `content_hash` from `processor/mod.rs` and `lib.rs`
- Replaced duplicate SHA256 implementation in `tests/common/mod.rs` with `factbase::content_hash()` call
- Removed unused `sha2::{Digest, Sha256}` import from `tests/common/mod.rs`
- Note: `scanner/orchestration/preread.rs` already called `DocumentProcessor::compute_hash()` (not inline SHA256), so no change needed there
- 1031 lib + 355 bin tests passing, all integration tests compile, zero clippy warnings
- No difficulties encountered

### Task 1.1+1.2+1.3 — ProgressReporter abstraction (commit f8bb6a5)
- Created `src/progress.rs` with `ProgressReporter` enum: `Cli { quiet }`, `Mcp { sender }`, `Silent`
- Three methods: `report(current, total, message)`, `phase(name)`, `log(message)` — CLI writes to stderr, MCP sends JSON via channel + eprintln, Silent is no-op
- `ProgressSender` type alias moved from `mcp/tools/mod.rs` to `progress.rs`; re-exported from `mcp/tools/mod.rs` for backward compatibility
- Both `ProgressReporter` and `ProgressSender` re-exported from `lib.rs`
- 5 unit tests: Silent no-op, Cli quiet no-op, Mcp channel send for report/phase, Mcp None sender safety
- Also fixed pre-existing web build error: added missing `answer_question` and `bulk_answer_questions` re-exports from `mcp/tools/mod.rs`
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 2.1+2.2+2.3+2.4 — Wire ProgressReporter into scan (commit 0dd6f41)
- Added `progress: &ProgressReporter` parameter to `full_scan()` and `scan_all_repositories()`
- Replaced inline `info!("Progress: {}/{}", ...)` with `progress.report()` (fires every 25 files when ≥50 total)
- Added `progress.phase("Indexing documents")` before pass 1 and `progress.phase("Detecting links")` before pass 2
- `scan_all_repositories()` reports per-repo phase: `progress.phase("Scanning repository '{id}'")`
- MCP `scan_repository`: constructs `ProgressReporter::Mcp { sender: progress }`, removed `tokio::select!` 10-second timer loop — progress now comes directly from `full_scan`
- CLI `cmd_scan`: constructs `ProgressReporter::Cli { quiet }` — existing `OptionalProgress` indicatif bar remains for TTY
- `serve.rs` watcher: uses `ProgressReporter::Silent` (background rescans don't need user feedback)
- All 22 integration test `full_scan` calls updated with `&factbase::ProgressReporter::Silent`
- Added `#[allow(clippy::too_many_arguments)]` to `full_scan` (8 params, was 7)
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 3.1+3.2+3.3+3.4 — Wire ProgressReporter into lint (commit 4ebc5ad)
- 3.1: Changed `lint_repository` MCP tool signature from `Option<ProgressSender>` to `&ProgressReporter`; replaced `eprintln!` + manual `tx.send()` with `progress.report()`
- 3.2: Extracted core lint-all-documents loop into `src/question_generator/lint.rs` with `lint_all_documents()` function and `LintConfig` struct; MCP `lint_repository` now delegates to this shared function
- 3.3: CLI `cmd_lint` constructs `ProgressReporter::Cli { quiet }` and uses `progress.report()` for per-document feedback in review mode; removed `indicatif` ProgressBar dependency from lint module entirely
- 3.4: Added `progress.phase("Generating review questions")` before question generation pass; added `progress.phase("Cross-document validation")` before cross-check pass in CLI
- MCP dispatch updated: constructs `ProgressReporter::Mcp { sender: progress }` before calling `lint_repository` (same pattern as `scan_repository`)
- CLI cross-check loop also reports per-document progress via `progress.report()`
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 4.1+4.2+4.3+4.4 — Wire ProgressReporter into review apply (commit 2627359)
- 4.1: Created `src/answer_processor/apply_all.rs` with `apply_all_review_answers()` shared function and `ApplyConfig` struct (doc_id_filter, repo_filter, dry_run, since). Returns `ApplyResult` with per-document `ApplyDocResult` (doc_id, doc_title, questions_applied, status, error)
- 4.2: MCP `apply_review_answers` now takes `&ProgressReporter` parameter; constructs `ApplyConfig` from args and delegates to shared function; MCP dispatch constructs `ProgressReporter::Mcp { sender: progress }`
- 4.3: CLI `cmd_review_apply` calls shared function with `ProgressReporter::Cli { quiet }`; removed `AnsweredQuestion`, `collect_answered_questions()`, `process_document()` — all replaced by shared function. Kept inbox block processing and `--detailed` output as CLI-specific concerns. `--since` filter now uses `Document.file_modified_at` from DB instead of filesystem stat
- 4.4: Shared function reports `progress.report(i+1, total, "Applying N question(s) to Title")` per document
- Net reduction: 352 insertions, 486 deletions (134 lines removed)
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 5.1+5.2+5.3 — Wire ProgressReporter into get_review_queue (commit b873707)
- 5.1: Added `progress: &ProgressReporter` parameter to `get_review_queue`. Reports `progress.log("Processing N documents with review queues")` before the loop, and `progress.report(i, total, &doc.title)` every 50 documents (uses `is_multiple_of(50)` per clippy)
- 5.2: Added early termination — once `all_questions.len() >= limit` (page filled), breaks out of the doc loop. Adds `has_more: true` to response when totals are approximate due to early termination. Web stats/status endpoints pass `limit: 1000000` + `status: "all"` to disable early termination and get accurate totals
- 5.3: MCP dispatch constructs `ProgressReporter::Mcp { sender: progress }` and passes to `get_review_queue`. Web API callers (`list_review_queue`, `get_document_questions`, `get_review_status`, `compute_review_stats`) pass `&ProgressReporter::Silent`
- Also fixed pre-existing clippy `needless_borrow` warning on `format_question_json` call (`&q` → `q`)
- Key consideration: web stats endpoints need accurate totals, so they bypass early termination via high limit
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 6.1+6.2 — Wire ProgressReporter into search_content/grep (commit def1111)
- 6.1: Added `progress: &ProgressReporter` parameter to `Database::search_content()` in `database/search/content.rs`; calls `progress.log("Searching document content...")` before query execution
- 6.2: MCP `search_content` tool now accepts `&ProgressReporter`; dispatch in `handle_tool_call` constructs `ProgressReporter::Mcp { sender: progress }` (replaced `blocking_tool!` macro with explicit dispatch). CLI `run_single_grep` constructs `ProgressReporter::Cli { quiet: args.quiet }`
- Updated all test call sites: 9 lib tests + 5 integration tests pass `&ProgressReporter::Silent`
- 1024 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 6.3 — FTS5 virtual table for content search (commit befd620)
- Created `document_content_fts` FTS5 virtual table with `doc_id UNINDEXED` + `content` columns
- Schema version bumped to 7; migration creates table and backfills from existing documents via `backfill_fts5()`
- Fresh databases create FTS5 table in `init_schema()` alongside other tables
- `search_content()` now dispatches: `is_fts_compatible()` checks if pattern is simple (alphanumeric, spaces, hyphens, underscores, apostrophes); if yes, uses FTS5 MATCH via `search_content_fts5()`; falls back to LIKE via `search_content_like()` for regex/special chars or on FTS5 error
- `fts5_phrase()` wraps pattern in double quotes for safe FTS5 phrase matching
- FTS5 query joins `document_content_fts` with `documents` table, applies same type/repo/since filters with `d.` column prefix
- `upsert_document()` and `update_document_content()` now keep FTS5 in sync (DELETE + INSERT on each write) — done here since "populate during scan" requires it; Task 6.4 reduced to delete-side sync only (`mark_deleted`, `hard_delete_document`)
- 6 new tests: `is_fts_compatible` (10 assertions), `fts5_phrase`, FTS5 search path, LIKE fallback for regex, content update sync, FTS5 table existence
- Key consideration: FTS5 does word-level matching (not substring like LIKE), so searching "OD" won't match "TODO" — this is an intentional trade-off for O(log N) vs O(N) performance
- 1030 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 6.4 — FTS5 delete sync in mark_deleted/hard_delete_document (commit 0e34e66)
- Added `DELETE FROM document_content_fts WHERE doc_id = ?1` to `mark_deleted()` — runs after the `UPDATE documents SET is_deleted = TRUE` statement
- Added `DELETE FROM document_content_fts WHERE doc_id = ?1` to `hard_delete_document()` — runs before `DELETE FROM documents` (alongside other cascade deletes)
- Updated `test_mark_deleted` and `test_hard_delete_document` to verify FTS5 entries are removed: queries `document_content_fts` directly and asserts count is 0
- FTS5 index now fully consistent across all write paths: upsert (INSERT), update_content (DELETE+INSERT), mark_deleted (DELETE), hard_delete (DELETE)
- 969 lib + 348 bin tests passing, zero new clippy warnings in changed files (4 pre-existing clippy issues in other files)
- No difficulties encountered

### Task 7.1+7.2+7.3+7.4+7.5+7.6 — Wire ProgressReporter into organize analyze (commits 9b7eb3a, 017011c, 0e92812)
- 7.1: Added `progress: &ProgressReporter` to `detect_merge_candidates()` — phase at start, per-doc `progress.report(i+1, total, &doc.title)` in the similarity loop
- 7.2: Added `progress: &ProgressReporter` to `detect_split_candidates()` — phase at start, per-doc progress (this is the slowest due to per-section embedding generation)
- 7.3: Added `progress: &ProgressReporter` to `detect_misplaced()` — phase at start, per-doc progress during centroid comparison
- 7.4: Added `progress: &ProgressReporter` to `detect_duplicate_entries()` — phase at start, per-doc progress during extraction, `progress.log("Matching N entries...")` before fuzzy matching phase
- 7.5: CLI `organize analyze` constructs `ProgressReporter::Cli { quiet: false }` and passes to all four detect functions. MCP `get_duplicate_entries` accepts `&ProgressReporter`, dispatch constructs `ProgressReporter::Mcp { sender: progress }`. Web API callers use `ProgressReporter::Silent`
- 7.6: Top-level phase reporting in CLI: `"Analysis 1/4: Merge candidates"`, `"Analysis 2/4: Split candidates"`, `"Analysis 3/4: Misplaced documents"`, `"Analysis 4/4: Duplicate entries"`
- Changed `for doc in docs` to `for (i, doc) in docs.iter().enumerate()` in split.rs — loop body already used `.clone()` so reference vs owned was seamless
- Web API callers (organize.rs, stats.rs) use `crate::ProgressReporter::Silent` (lib crate), CLI uses `factbase::ProgressReporter` (bin crate)
- Updated 4 test calls in `duplicate_entries.rs` to pass `&crate::ProgressReporter::Silent`
- 1030 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 8.1+8.2+8.3 — Wire ProgressReporter into export/import (commit 6499e5e)
- 8.1: Added `progress: &ProgressReporter` to `export_json()`, `export_yaml()`, `export_markdown_stdout()`, `export_markdown_single_file()`, `export_markdown_directory()`, `export_archive()`. All report `progress.report(i+1, total, &doc.title)` per document. `build_markdown_content()` also accepts and forwards progress
- 8.2: Added `progress: &ProgressReporter` to `import_json()`, `import_json_content()`, `import_json_zst()`, `import_md_zst()`, `import_tar_zst()`, `import_directory()`. JSON and directory imports use `progress.report()` with known totals. Tar archive uses `progress.log()` per file (streaming, unknown total). `import_md_zst` reports `progress.report(1, 1, filename)` for single-file import
- 8.3: `cmd_export` and `cmd_import` construct `ProgressReporter::Cli { quiet: false }` (no quiet flag on these commands, consistent with `organize analyze` pattern)
- 1030 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 9.1+9.2 — Wire ProgressReporter into bulk MCP operations (commit 6efa403)
- 9.1: Added `progress: &ProgressReporter` to `bulk_create_documents()` in `mcp/tools/document.rs`. Reports `progress.report(i+1, total, title)` per document during the write loop. Updated `#[instrument]` skip list
- 9.2: Added `progress: &ProgressReporter` to `bulk_answer_questions()` in `mcp/tools/review/answer.rs`. Reports `progress.report(i+1, total_docs, "Answering N question(s) in doc_id")` per document during the apply loop. Updated `#[instrument]` skip list. `answer_questions()` wrapper in `review/mod.rs` accepts and forwards progress to `bulk_answer_questions` (single `answer_question` doesn't need progress — it's one question)
- MCP dispatch: replaced `blocking_tool!` macro with explicit `run_blocking` dispatch for both tools, constructing `ProgressReporter::Mcp { sender: progress }` (same pattern as `search_content`)
- Web API: `bulk_answer_questions` call in `web/api/review.rs` passes `&crate::ProgressReporter::Silent`
- 1030 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 10.1+10.2 — Update handle_tool_call dispatch (commit d53dac2)
- 10.1: Construct `ProgressReporter::Mcp { sender: progress }` once at top of `tools/call` branch instead of per-tool. Async tools (`lint_repository`, `scan_repository`, `apply_review_answers`, `get_duplicate_entries`) pass `&reporter` directly. `get_review_queue` (sync, non-blocking) also passes `&reporter` directly
- 10.2: Added 3-arg `blocking_tool!` macro variant `($db, $args, $reporter, $fn)` that clones all three and moves into `run_blocking` closure. Replaced manual `run_blocking` dispatch for `bulk_create_documents`, `search_content`, and `answer_questions` with the new macro variant. Simple tools without progress (`get_entity`, `list_entities`, etc.) remain on the 2-arg macro — they don't accept `&ProgressReporter`
- Added `#[derive(Clone)]` to `ProgressReporter` — `UnboundedSender` is Clone, so all variants are cloneable
- Net reduction: 12 insertions, 20 deletions (8 lines removed)
- 1030 lib + 355 bin tests passing, zero new clippy warnings
- No difficulties encountered

### Task 11.1+11.2+11.3+11.4 — Cleanup and tests (commits d32a26f, 192348a)
- 11.1: Removed `pub use crate::progress::ProgressSender` backward-compatibility re-export from `mcp/tools/mod.rs` — no consumers imported from that path. Changed to private `use` import. Added explicit `Value` type annotation on `unbounded_channel()` in `stdio.rs` since type inference no longer flows through the re-export
- 11.2: Added `test_scan_repository_sends_progress_via_mcp_channel` async test in `mcp/tools/mod.rs` — creates temp repo with 2 markdown files, runs `scan_repository` with `MockEmbedding` and an unbounded channel, asserts progress messages (phase/log) are received
- 11.3: Verified all tests pass: 1031 lib + 355 bin = 1386 total. Zero new clippy warnings
- 11.4: Updated `current-state.md`: project status to "Phases 1-47 complete", removed active work section, updated test counts. Updated `module-interactions.md`: added `progress.rs` to file structure and Module Responsibilities section
- No difficulties encountered

### Task 12.1 — Introduce ContentSearchParams struct (commit 41abbe5)
- Created `ContentSearchParams<'a>` struct in `database/search/content.rs` with fields: `pattern`, `limit`, `doc_type`, `repo_id`, `context_lines`, `since`, `progress`
- Updated `search_content()`, `search_content_fts5()`, and `search_content_like()` to accept `&ContentSearchParams` instead of 7 individual parameters
- Eliminates all three `clippy::too_many_arguments` warnings in `database/search/content.rs`
- Re-exported `ContentSearchParams` from `database/search/mod.rs` → `database/mod.rs` → `lib.rs` for access from both lib and bin crates
- Updated callers: MCP `search_content` tool, CLI `run_single_grep`, 10 lib tests (with `params()` test helper for defaults using struct update syntax), 5 integration tests in `grep_tests.rs`, 2 bench call sites (also fixed pre-existing missing `progress` parameter)
- Renamed local `params` variable to `db_params` in FTS5/LIKE functions to avoid shadowing the struct parameter
- Pre-existing integration test compilation errors in `cli` test crate (`scan_tests.rs` missing `link_batch_size`, `common/mod.rs` wrong `McpServer::new` arity) are unrelated
- 1031 lib + 355 bin = 1386 tests passing, zero new clippy warnings
- No difficulties encountered

### Task 14.1 — Fix remaining clippy suggestions (is_multiple_of)
- Replaced `global_idx % 25 == 0` with `global_idx.is_multiple_of(25)` in `scanner/orchestration/mod.rs` line 106
- Only one instance in codebase — the other `% N == 0` in `patterns.rs` is a leap year check (not a "multiple of" pattern)
- Zero clippy warnings across all features after fix
- 1031 lib + 355 bin = 1386 tests passing
- No difficulties encountered

### Task 13.1 — Introduce ScanContext struct (commit fa9302d)
- Created `ScanContext<'a>` struct in `scanner/orchestration/mod.rs` with 6 fields: `scanner`, `processor`, `embedding`, `link_detector`, `opts`, `progress` — all borrowed references with lifetime `'a`
- Updated `full_scan()` signature from 8 params (`repo`, `db`, `scanner`, `processor`, `embedding`, `link_detector`, `opts`, `progress`) to 3 params (`repo`, `db`, `&ScanContext<'_>`)
- Updated `scan_all_repositories()` from 7 params to 2 params (`db`, `&ScanContext<'_>`)
- Removed `#[allow(clippy::too_many_arguments)]` from `full_scan`
- Updated `#[tracing::instrument]` skip list from 7 individual params to `skip(db, ctx)`
- Re-exported `ScanContext` from `scanner/mod.rs` → `lib.rs`
- Updated all callers: `cmd_scan` (2 call sites), `scan_repository` MCP tool, `serve.rs` watcher, `scan_all_repositories` internal call
- Updated 5 integration test files (17 call sites total): `full_scan_e2e.rs`, `mcp_e2e.rs`, `multi_repo_e2e.rs`, `chunking_integration.rs`, `cli/scan_tests.rs`, `common/mod.rs`
- Pre-existing integration test compilation errors (missing `link_batch_size`, wrong `McpServer::new` arity) remain unrelated
- 1031 lib + 355 bin = 1386 tests passing, zero new clippy warnings (only pre-existing `is_multiple_of` from Task 14)
- No difficulties encountered

### Task 15.1+15.2+15.3+15.4 — Fix integration test compilation errors (commit b0d9f3b)
- 15.1: Added `link_batch_size: 5,` to all `ScanOptions { ... }` initializers in 6 test files: `tests/common/mod.rs`, `tests/cli/scan_tests.rs`, `tests/full_scan_e2e.rs`, `tests/mcp_e2e.rs`, `tests/multi_repo_e2e.rs`. Files using `..Default::default()` (e.g., `chunking_integration.rs`) were already fine since `Default` impl includes the field
- 15.2: Added `None,` (llm parameter) to all 15 `McpServer::new()` calls across 7 test files: `tests/common/mod.rs`, `tests/mcp_e2e.rs`, `tests/serve_e2e.rs`, `tests/watcher_e2e.rs`, `tests/concurrent_stress.rs`, `tests/performance.rs`, `tests/stability.rs`
- 15.3: Added `&factbase::ProgressReporter::Silent` to `detect_split_candidates()` and `detect_misplaced()` calls in `tests/ollama_integration.rs`
- 15.4: All 20 integration test binaries compile successfully with `cargo test --test '*' --no-run` — zero errors
- Actual error count was higher than task description's "16": the `McpServer::new` arity mismatch affected more files than originally listed (concurrent_stress.rs, stability.rs, watcher_e2e.rs, mcp_e2e.rs had multiple call sites)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 16.1 — Consolidate duplicate cosine_similarity implementations (commit d509f5a)
- Changed `cosine_similarity` in `organize/detect/mod.rs` from `pub(crate)` to `pub`
- Added re-export through `organize/mod.rs` → `lib.rs` (alphabetical order in existing re-export lists)
- Removed duplicate from `tests/common/mod.rs` (simpler version without empty/mismatched/zero-norm guards)
- Updated `tests/ollama_integration.rs` and `tests/full_scan_e2e.rs` to import `factbase::cosine_similarity` instead of `common::cosine_similarity`
- Net: 12 insertions, 20 deletions across 6 files
- 1031 lib + 355 bin = 1386 tests passing (all features), zero clippy warnings
- No difficulties encountered

### Task 18.1+18.2+18.3 — Migrate callers from DocumentProcessor::compute_hash to content_hash (commit 968b0c8)
- 18.1: Replaced 6 production code calls across 4 files: `scanner/orchestration/preread.rs` (1), `mcp/tools/review/answer.rs` (2), `commands/scan/verify.rs` (2), `question_generator/lint.rs` (1). Updated imports: added `content_hash` import, removed unused `DocumentProcessor` import from `verify.rs`
- 18.2: Replaced 12 test code calls across 3 files: `database/stats/temporal.rs` (5), `database/stats/sources.rs` (4), `processor/core.rs` (3). Added `use crate::processor::content_hash;` to test modules in temporal.rs and sources.rs
- 18.3: Added `#[deprecated(note = "use content_hash() free function instead")]` to `DocumentProcessor::compute_hash()`. Updated `processor/mod.rs` doc comment to reference `content_hash` instead of `DocumentProcessor::compute_hash`
- Zero remaining `DocumentProcessor::compute_hash` calls in the codebase — the method exists only as a deprecated wrapper
- Integration tests' `common::compute_hash` already delegates to `factbase::content_hash()` (from Task 17), no changes needed
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 20.1 — Reduce verbose crate:: paths in database stats test modules (commit 90d4e1c)
- Changed `use crate::processor::content_hash;` to `use crate::content_hash;` in both `database/stats/temporal.rs` and `database/stats/sources.rs` test modules
- Task 18.2 had already migrated the call sites from `DocumentProcessor::compute_hash()` to `content_hash()` and added the imports — this task simplified the import path to use the `lib.rs` re-export instead of the internal `processor` module path
- No other verbose `crate::` paths in these test modules needed simplification (remaining imports are standard module paths)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 21.1 — Remove deprecated DocumentProcessor::compute_hash method (commit a6f3d60)
- Removed the `#[deprecated]` `compute_hash` method from `DocumentProcessor` in `processor/core.rs` (7 lines: doc comment + annotation + method body)
- Verified zero production callers via grep (`DocumentProcessor::compute_hash` — 0 matches in `src/`)
- Existing tests in `processor/core.rs` (`test_compute_hash`, `test_compute_hash_deterministic`, `test_compute_hash_different_content`) already used the `content_hash()` free function — only test names reference the old method
- `tests/common/mod.rs::compute_hash` is a separate test helper that delegates to `factbase::content_hash()` — unrelated
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 22.1 — Fix redundant closures flagged by clippy pedantic (commit 9bd7225)
- Replaced 26 redundant closures across 17 files (17 lib-crate + 9 bin-crate)
- The actual lint is `clippy::redundant_closure_for_method_calls` (pedantic), not `clippy::redundant_closure` (which had zero hits)
- `cargo clippy --fix` auto-applied 9 bin-crate fixes but could not auto-fix the 17 lib-crate instances — those required manual replacement
- Common patterns replaced: `|v| v.as_u64()` → `Value::as_u64`, `|s| s.to_rfc3339()` → `DateTime::to_rfc3339`, `|entry| entry.ok()` → `Result::ok`, `|s| s.to_string()` → `ToString::to_string`, `|r| r.to_json()` → `ContentSearchResult::to_json`
- One import fix needed: `ContentSearchResult` was imported via private `crate::models::search::` path — changed to `crate::models::ContentSearchResult` (the re-exported path)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings (both `redundant_closure` and `redundant_closure_for_method_calls`)
- No difficulties encountered

### Task 23.1 — Use write!/writeln! instead of push_str(format!(...)) (commit 9aa04cf)
- Replaced 24 instances of `push_str(&format!(...))` with `write!()` or `writeln!()` using `std::fmt::Write` across 10 files
- Used `writeln!` where format string ended with `\n` (clippy `write_with_newline`), `write!` for embedded newlines or no trailing newline
- Pattern: `use std::fmt::Write;` imported at narrowest scope (function body or block) to avoid polluting module namespace — `std::fmt::Write` conflicts with `std::io::Write` if both in scope
- `.expect("write to String")` on all `write!`/`writeln!` calls — writing to `String` is infallible in practice but `fmt::Write` returns `Result`
- Files: `database/documents.rs` (4), `database/search/content.rs` (2), `database/search/title.rs` (1), `organize/plan/merge.rs` (3), `organize/plan/split.rs` (2), `organize/orphans.rs` (2), `organize/review.rs` (1), `processor/review.rs` (1), `commands/export/markdown.rs` (5), `question_generator/cross_validate.rs` (3)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings (including `format_push_string` and `write_with_newline`)
- No difficulties encountered

### Task 24.1 — Introduce write_str!/writeln_str! macros (commit c7118f7)
- Created `write_str!` and `writeln_str!` macros at the top of `src/lib.rs` (before module declarations so they're available crate-wide via `#[macro_export]`)
- Each macro wraps `write!`/`writeln!` with `use std::fmt::Write as _;` and `.expect("write to String infallible")` — writing to `String` is infallible
- Replaced all 28 `.expect("write to String")` call sites across 11 source files (10 lib-crate + 1 bin-crate)
- Removed 12 scattered `use std::fmt::Write;` imports — the macros handle the import internally via `use std::fmt::Write as _;`
- Bin-crate file (`commands/export/markdown.rs`) required explicit `use factbase::writeln_str;` import since `#[macro_export]` macros need to be imported in the bin crate
- Lib-crate files needed no explicit import — `#[macro_export]` macros defined before module declarations are automatically available
- Net: 55 insertions, 53 deletions (12 files changed)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 25.1 — Reduce unnecessary .clone() calls in database search hot paths (commit 6bfcde2)
- Audited all 16 `.clone()` calls in `database/` module: 3 in search hot paths (`search/semantic.rs`, `documents.rs`), 13 in stats/cache paths
- Eliminated 3 unnecessary allocations in the two most impactful hot paths:
  - `upsert_document`: replaced `doc.content.clone()` with `&str` reference when compression is off — avoids cloning potentially large document content (called per-document during scan)
  - `update_document_content`: replaced `content.to_string()` with `&str` reference using same pattern
  - `search_semantic_paginated`: pass pre-read `doc_id` to `row_to_search_result_with_chunk` instead of reading `row.get(0)?` twice — avoids redundant String allocation per search result
- Pattern used: `let compressed; let content_to_store: &str = if self.compression { compressed = B64.encode(...); &compressed } else { &doc.content };` — the `compressed` variable outlives the reference
- Fixed 2 `needless_borrow` clippy warnings on `compress_content()` results introduced by the refactor
- Remaining 13 clones are structurally necessary: HashMap key+value dual ownership (3), cache+return dual ownership (3), reference-to-owned conversion for date tracking (6), HashSet+Vec dual ownership (1)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all integration tests compile
- No difficulties encountered

### Task 26.1 — Split database/documents.rs into focused submodules (commit 10fd21a)
- Split 877-line `database/documents.rs` into a directory module with three focused submodules:
  - `documents/mod.rs` (68 lines): module doc, submodule declarations, `DOCUMENT_COLUMNS` constant, `repo_id_for_doc` helper, `row_to_document` shared method
  - `documents/crud.rs` (373 lines): `upsert_document`, `update_document_content`, `update_document_hash`, `update_document_type`, `needs_update`, `get_document`, `require_document`, `get_document_by_path`, `mark_deleted`, `hard_delete_document` + 7 tests
  - `documents/list.rs` (218 lines): `get_documents_for_repo`, `get_documents_with_review_queue`, `list_documents` + 4 tests
  - `documents/batch.rs` (265 lines): `needs_cross_check`, `set_cross_check_hash`, `clear_cross_check_hashes`, `backfill_word_counts` + 7 tests
- Follows the same pattern used by `database/search/` and `database/stats/` submodules
- `DOCUMENT_COLUMNS` made `pub(crate)` in mod.rs so crud.rs and list.rs can reference it
- `repo_id_for_doc` kept as a private free function in mod.rs, accessible to crud.rs via `super::`
- `super::super::` paths used in submodules to reach `Database`, `compress_content`, `decode_content`, `doc_not_found`, `B64` from `database/mod.rs`
- No import changes needed in any external callers — all methods remain on `Database` impl
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all integration tests compile
- No difficulties encountered

---

28 call sites use `write!(string, ...).expect("write to String")` or `writeln!(string, ...).expect("write to String")`. Writing to `String` is infallible, so the `.expect()` is noise. A thin macro wrapper eliminates the repetition and the scattered `use std::fmt::Write` imports.

- [x] 24.1 Create `write_str!(target, ...)` and `writeln_str!(target, ...)` macros in `src/lib.rs` (or a new `src/macros.rs`) that expand to `{ use std::fmt::Write; write!(target, ...).expect("write to String infallible") }` and the `writeln!` equivalent. Replace all 28 call sites across 10 files. Remove the now-unnecessary `use std::fmt::Write` imports. Verify zero clippy warnings and all tests pass.

## Task 25: Reduce unnecessary `.clone()` calls in database search hot paths

The search modules (`database/search/semantic.rs`, `database/search/content.rs`, `database/search/title.rs`) and document listing (`database/documents.rs`) clone strings from row results that could use references or be consumed directly. Profile the 315 `.clone()` calls in production code and eliminate unnecessary ones in the most-called paths.

- [x] 25.1 Audit `.clone()` calls in `database/search/` and `database/documents.rs`. Replace with borrows, `into()`, or restructured ownership where the clone is avoidable. Focus on hot paths (search results, document listing). Verify all tests pass.

## Task 26: Split `database/documents.rs` (877 lines) into focused submodules

`database/documents.rs` is the largest file at 877 lines with 12 methods spanning CRUD, listing, filtering, and batch operations. Split into focused submodules following the pattern already used by `database/search/` and `database/stats/`.

- [x] 26.1 Split `database/documents.rs` into `database/documents/mod.rs` (re-exports), `database/documents/crud.rs` (upsert, get, delete, mark_deleted), `database/documents/list.rs` (list_documents, list_all_document_ids, get_documents_by_ids), `database/documents/batch.rs` (batch operations). Update imports. Verify all tests pass.

## Task 27: Use captured identifiers in format strings (clippy `uninlined_format_args`)

291 instances of `format!("{}", x)` that should be `format!("{x}")` (Rust 2021 captured identifiers). Auto-fixable for most cases.

- [x] 27.1 Run `cargo clippy --fix --all-features -- -W clippy::uninlined_format_args` to auto-fix bin crate instances. Manually fix remaining lib crate instances. Verify all tests pass and zero `uninlined_format_args` warnings remain.

### Task 27.1 — Use captured identifiers in format strings (commit 3a1d2c1)
- Replaced 291 instances of `format!("{}", x)` with `format!("{x}")` across 85 files (41 lib-crate + 44 bin-crate)
- `cargo clippy --fix --allow-dirty --allow-staged --bin factbase --all-features -- -W clippy::uninlined_format_args` auto-fixed bin crate
- `cargo clippy --fix --allow-dirty --allow-staged --lib --all-features -- -W clippy::uninlined_format_args` auto-fixed lib crate (contrary to prior expectation that lib crate needed manual fixes)
- Net: 300 insertions, 357 deletions (-57 lines across 85 files)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings (including `uninlined_format_args`)
- No difficulties encountered — auto-fix handled both crates cleanly

## Task 28: Replace `map().unwrap_or()` with `map_or()` and `let...else` patterns

14 instances of `map(<f>).unwrap_or(<a>)` on Option values (clippy `map_unwrap_or`) and 11 instances of `if let` that could use `let...else` (clippy `option_if_let_else` / `manual_let_else`). Both are cleaner idiomatic Rust.

- [x] 28.1 Replace `map().unwrap_or()` / `map().unwrap_or_else()` with `map_or()` / `map_or_else()` across all flagged files. Replace eligible `if let Some(x) = ... { ... } else { ... }` with `let...else` where it improves readability. Verify all tests pass.

## Task 29: Update `database/mod.rs` doc comment to reflect documents/ split

The module-level doc comment in `database/mod.rs` still lists "Document Operations (10)" as a flat list. Update to reflect the new `documents/` submodule structure (crud, list, batch).

- [x] 29.1 Update the `database/mod.rs` module doc comment to document the `documents/` submodule split (crud, list, batch). Also update `.kiro/steering/module-interactions.md` to reflect the new `database/documents/` directory structure.

### Task 28.1 — Replace map().unwrap_or() with map_or() and let...else patterns (commit e81de4b)
- Replaced 29 `map().unwrap_or()`/`unwrap_or_else()` instances with `map_or()`/`map_or_else()` across 17 files — clippy auto-fixed 15 (lib+bin), manually fixed 14 remaining bin-crate instances
- Further simplified 3 boolean cases where clippy suggested more idiomatic alternatives: `map_or(false, ..)` → `is_some_and()`/`is_ok_and()`, `map_or(true, ..)` → `is_none_or()`
- Replaced 11 `match` with `continue`/`return` patterns with `let...else` across 11 files: `apply_all.rs`, `conflict.rs`, `fields.rs`, `merge.rs`, `misplaced.rs`, `output.rs`, `scanner/mod.rs`, `watcher.rs`, `import/formats.rs`, `review/import.rs` (2 instances)
- Net: 75 insertions, 127 deletions (-52 lines across 28 files)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings (including `map_unwrap_or` and `manual_let_else`)
- No difficulties encountered

### Task 29.1 — Update database/mod.rs doc comment for documents/ split (commit bbe568b)
- Restructured "Document Operations (10)" flat list into three subsections: CRUD (10 methods), Listing (3 methods), Batch (4 methods) — matching the `documents/crud.rs`, `documents/list.rs`, `documents/batch.rs` submodule split from Task 26
- Updated module list entry from `documents` to `documents/` with "(crud, list, batch submodules)" description
- Updated total method count from 48 to 55 — the original count was already outdated; the split added 7 previously undocumented methods (update_document_content, update_document_type, get_documents_with_review_queue, needs_cross_check, set_cross_check_hash, clear_cross_check_hashes, backfill_word_counts)
- `.kiro/steering/module-interactions.md` already had the correct `documents/` directory structure from Task 26 commit — no changes needed
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings
- No difficulties encountered

### Task 30.1 — Extract compression helpers from database/mod.rs (commit abb1686)
- Created `database/compression.rs` with 5 items: `ZSTD_PREFIX` constant (cfg-gated), `compress_content()` (two cfg variants), `decompress_content()` (cfg-gated), `decode_content()`, `decode_content_lossy()`
- Re-exported all items from `database/mod.rs` via `pub(crate) use compression::{...}` — no import changes needed in any submodules (`documents/crud.rs`, `documents/batch.rs`, `documents/mod.rs`, `schema.rs`, `stats/mod.rs`, `stats/detailed.rs`, `stats/compression.rs`, `search/content.rs`, `search/title.rs`, `search/semantic.rs`) since they all resolve through `super::` paths to mod.rs re-exports
- Moved 5 compression-specific unit tests to the new module; kept `test_database_with_compression` in mod.rs since it tests the full Database roundtrip (not just compression functions)
- Updated module doc comment to list `compression` submodule
- Removed unused `use base64::Engine;` from mod.rs (now only needed in compression.rs)
- `database/mod.rs`: 553 → 444 lines (-109); `database/compression.rs`: 127 lines
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all integration tests compile
- No difficulties encountered

### Task 31.1 — Deduplicate to_json() / response building in MCP entity tools (commit 008dddd)
- Extracted 4 helper functions from `get_document_stats()` into `mcp/tools/helpers.rs`:
  - `build_temporal_stats_json(content)`: computes fact stats, temporal tags, by-type HashMap, returns JSON
  - `build_source_stats_json(content)`: computes source refs/defs, orphan detection, by-type HashMap, returns JSON
  - `build_link_stats_json(outgoing, incoming)`: simple link count JSON
  - `build_review_stats_json(content)`: parses review queue, computes totals/answered/pending, returns JSON
- `get_document_stats()` reduced from ~100 lines of inline computation+JSON to 15 lines of helper calls
- Removed 6 unused imports from `entity.rs` (`calculate_fact_stats`, `count_facts_with_sources`, `parse_review_queue`, `parse_source_definitions`, `parse_source_references`, `parse_temporal_tags`, `HashMap`, `HashSet`)
- Net: 83 insertions, 93 deletions (-10 lines across 2 files)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all integration tests compile
- No difficulties encountered

### Task 32.1 — Consolidate repeated ScanOptions::default() in integration tests (commit 7ce6517)
- Added `test_scan_options()` and `test_scan_options_skip_links()` helpers to `tests/common/mod.rs`
- `test_scan_options()` returns `ScanOptions::default()` — all verbose initializers already matched the Default impl exactly
- `test_scan_options_skip_links()` returns `ScanOptions { skip_links: true, ..Default::default() }` for future use (no current callers)
- Replaced 12 verbose `ScanOptions { ... }` initializers across 5 files: `common/mod.rs` (1), `full_scan_e2e.rs` (4), `multi_repo_e2e.rs` (4), `mcp_e2e.rs` (2), `cli/scan_tests.rs` (1)
- `cli/scan_tests.rs` uses struct update syntax: `ScanOptions { dry_run: true, ..test_scan_options() }` for its unique override
- `chunking_integration.rs` already used `..Default::default()` with custom chunk_size — left unchanged
- Removed unused `ScanOptions` import from `full_scan_e2e.rs`, `multi_repo_e2e.rs`, `mcp_e2e.rs` (still needed in `cli/scan_tests.rs` for struct update syntax)
- Net: 34 insertions, 180 deletions (-146 lines across 5 files)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all 20 integration test binaries compile
- No difficulties encountered

### Task 33.1 — Consolidate repeated full-scan setup into TestScanSetup struct (commit 2d54e5c)
- Added `TestScanSetup` struct to `tests/common/mod.rs` owning Config, Scanner, DocumentProcessor, OllamaEmbedding, LinkDetector, and ScanOptions
- `new()` constructor creates all components from `Config::default()`; `with_options(opts)` allows custom ScanOptions
- `context(&self) -> ScanContext<'_>` borrows all fields with `ProgressReporter::Silent`
- Simplified `run_scan()` helper to 2 lines using `TestScanSetup::new()`
- Replaced ~17 setup blocks + ~5 ScanContext rebuilds across 5 files: `full_scan_e2e.rs` (4), `multi_repo_e2e.rs` (4+5), `mcp_e2e.rs` (2+2), `cli/scan_tests.rs` (1), `common/mod.rs` (1)
- `cli/scan_tests.rs` uses `TestScanSetup::with_options(ScanOptions { dry_run: true, .. })` for its unique override
- `mcp_e2e.rs` `setup_indexed_repo()` moves `setup.embedding` and `setup.config` out of the struct for return (OllamaEmbedding is Clone)
- `chunking_integration.rs` left unchanged — uses `Config::load(None)` and `Scanner::new(&[])` which differ from the standard pattern
- Tests referencing `embedding` directly changed to `setup.embedding` (e.g., `setup.embedding.generate(...)`)
- Removed unused imports: Config, OllamaEmbedding, OllamaLlm, LinkDetector, DocumentProcessor, Scanner, ScanContext from files that no longer need them directly
- Net: 101 insertions, 353 deletions (-252 lines across 5 files)
- 1031 lib + 355 bin = 1386 tests passing, zero clippy warnings, all 20 integration test binaries compile
- No difficulties encountered

## Task 30: Extract compression helpers from `database/mod.rs`

`database/mod.rs` is 553 lines mixing module-level doc comment, Database struct, constructors, compression helpers (`compress_content`, `decode_content`, `decode_content_lossy`), and transaction methods. The compression functions are used by multiple submodules via `super::` paths and are logically separate from the Database struct.

- [x] 30.1 Extract `compress_content()`, `decode_content()`, and `decode_content_lossy()` (plus the `#[cfg(feature = "compression")]` conditional compilation blocks) into a new `database/compression.rs` module. Update all `super::compress_content` / `super::decode_content` / `super::decode_content_lossy` references in submodules. Verify all tests pass.

## Task 31: Deduplicate `to_json()` / response building in MCP entity tools

`mcp/tools/entity.rs` has 6 inline `serde_json::json!({...})` response blocks that manually assemble document stats (fact counts, temporal stats, source stats, link stats, review stats). Several share identical field patterns. Extract repeated stat-building into helper methods on the model structs or shared builder functions.

- [x] 31.1 Identify repeated JSON stat-building patterns in `mcp/tools/entity.rs` (e.g., `total_facts`, `facts_with_tags`, `outgoing`/`incoming` link counts). Extract into reusable helper functions (e.g., `build_fact_stats_json()`, `build_link_stats_json()`) in `mcp/tools/helpers.rs`. Update `get_entity` and `get_document_stats` to use the shared helpers. Verify all tests pass.

## Task 32: Consolidate repeated `ScanOptions::default()` field initialization in integration tests

Integration test files (`tests/full_scan_e2e.rs`, `tests/mcp_e2e.rs`, `tests/multi_repo_e2e.rs`, `tests/cli/scan_tests.rs`) each construct `ScanOptions { ... }` with identical field values (reindex: false, skip_links: false/true, link_batch_size: 5). Some use `..Default::default()` but others spell out every field. Consolidate into a shared test helper.

- [x] 32.1 Add a `test_scan_options()` helper to `tests/common/mod.rs` that returns `ScanOptions` with standard test defaults. Add a `test_scan_options_skip_links()` variant for the common skip-links case. Replace all manual `ScanOptions { ... }` initializers in integration tests. Verify all integration tests compile with `cargo test --test '*' --no-run`.

## Task 33: Consolidate repeated full-scan setup in integration tests

Integration tests repeat a ~15-line setup block (Config + Scanner + Processor + OllamaEmbedding + OllamaLlm + LinkDetector + ScanOptions + ScanContext) ~17 times across `full_scan_e2e.rs` (4), `multi_repo_e2e.rs` (4+), `mcp_e2e.rs` (2), `chunking_integration.rs` (3), `cli/scan_tests.rs` (1), `common/mod.rs` (1). Extract into a shared helper.

- [x] 33.1 Add a `TestScanSetup` struct to `tests/common/mod.rs` that owns `Config`, `Scanner`, `DocumentProcessor`, `OllamaEmbedding`, `OllamaLlm`, `LinkDetector`, and `ScanOptions`. Add a `new()` constructor and a `context(&self) -> ScanContext<'_>` method that borrows all fields. Replace repeated setup blocks across integration test files. Verify all integration tests compile with `cargo test --test '*' --no-run`.

## Task 34: Consolidate repeated `Repository { ... }` construction in integration tests

`Repository { id: "test".into(), name: "Test".into(), path: ..., perspective: None, created_at: Utc::now(), last_indexed_at: None, last_lint_at: None }` is constructed ~60+ times across integration tests with only `id`, `name`, and `path` varying. Extract into a helper.

- [x] 34.1 Add a `test_repo(id: &str, path: PathBuf) -> Repository` helper to `tests/common/mod.rs` that fills in standard defaults (`name` = capitalized `id`, `perspective: None`, `created_at: Utc::now()`, `last_indexed_at: None`, `last_lint_at: None`). Replace repeated `Repository { ... }` initializers across integration test files. Verify all integration tests compile with `cargo test --test '*' --no-run`.

## Task 35: Replace `expect("operation should succeed")` with concise `.unwrap()` in integration tests

Integration tests use `.expect("operation should succeed")` (~80+ occurrences) as a generic message that adds no diagnostic value over `.unwrap()`. In test code, `.unwrap()` is idiomatic and the panic message already includes file/line. Replace with `.unwrap()` to reduce noise.

- [x] 35.1 Replace all `.expect("operation should succeed")` with `.unwrap()` across integration test files. Verify all integration tests compile with `cargo test --test '*' --no-run`.

### Task 35.1 — Replace .expect("operation should succeed") with .unwrap() in integration tests (commit cab82b4)
- Replaced 641 `.expect("operation should succeed")` with `.unwrap()` across 20 integration test files using `sed`
- In test code, `.unwrap()` is idiomatic — panic messages already include file/line, making the generic expect string redundant
- Net: 642 insertions, 642 deletions (pure 1:1 replacement, no line count change)
- Had to `cargo clean` first due to disk full (100% usage) — freed 344GB, then compilation succeeded
- 1031 lib + 355 bin = 1386 tests passing, all 21 integration test binaries compile, zero clippy warnings
- No difficulties encountered

## Task 36: Consolidate identical match arm bodies (clippy `match_same_arms`)

10 match expressions have arms with identical bodies that can be merged using `|` patterns or by reordering the wildcard arm. Reduces code duplication and makes intent clearer.

- [x] 36.1 Merge identical match arms across flagged files: `answer_processor/temporal.rs` (month aliases), `organize/plan/merge.rs` (ORPHAN + wildcard), `patterns.rs` (6 instances: date format fallbacks, quarter end defaults, month day counts), `processor/temporal/parser.rs` (tag type matching), `processor/temporal/validation.rs` (symmetric conflict pairs). Run `cargo clippy --all-features -- -W clippy::match_same_arms` to verify zero remaining.

## Task 37: Fix case-sensitive file extension comparisons (clippy `case_sensitive_file_extension_comparisons`)

8 instances of `path.ends_with(".md")` / `.ends_with(".json")` etc. that should use case-insensitive comparison to handle `.MD`, `.Json`, etc. on case-preserving filesystems.

- [x] 37.1 Replace `path.ends_with(".ext")` with a case-insensitive helper (e.g., `path.to_ascii_lowercase().ends_with(".ext")` or `std::path::Path` extension check) across: `commands/export/mod.rs` (3), `commands/lint/mod.rs` (2), `commands/organize/move.rs` (1), `commands/review/import.rs` (2). Run `cargo clippy --all-features -- -W clippy::case_sensitive_file_extension_comparisons` to verify zero remaining.

## Task 38: Convert unused-`self` methods to free functions or associated functions

8 methods on `Database` take `&self` but never use it (they only use the `conn: &DbConn` parameter). Converting to associated functions or free functions clarifies that they don't need a `Database` instance.

- [x] 38.1 Convert the 8 unused-self methods in `database/schema.rs` (5: `get_schema_version`, `set_schema_version`, `backfill_has_review_queue`, `backfill_fts5`, `check_embedding_migration`) and `database/stats/` (3: `compression.rs`, `detailed.rs` ×2) to either associated functions (`fn foo(conn: &DbConn)` without `&self`) or private free functions. Update all call sites (`self.method(conn)` → `Self::method(conn)` or `method(conn)`). Run `cargo clippy --all-features -- -W clippy::unused_self` to verify zero remaining.

---

### Task 34.1 — Consolidate repeated Repository construction into test_repo() helper (commit 6ea1d56)
- Added `test_repo(id: &str, path: PathBuf) -> Repository` helper to `tests/common/mod.rs` — capitalizes first letter of `id` for name, fills `perspective: None`, `created_at: Utc::now()`, `last_indexed_at: None`, `last_lint_at: None`
- Replaced ~60 verbose `Repository { ... }` initializers across 18 integration test files with one-line `test_repo()` calls
- Cases with custom perspectives left as-is: `mcp_e2e.rs`, `serve_e2e.rs` (1 of 2), `compression_roundtrip.rs` (1 of 3), `cli/scan_tests.rs` (1 of 5), `common/mod.rs` TestServer
- Simplified `create_test_repo()` and `TestContext::new_with_perspective()` to use `test_repo()` internally
- Added `mod common;` to `chunking_integration.rs` and `watcher_integration.rs` (previously didn't need it)
- Fixed `common::` → `super::common::` for cli subdirectory test files
- Cleaned up unused imports: removed `Repository`, `chrono::Utc`, `models::Repository` from 12 files; fixed 2 malformed imports (`models::scanner::full_scan` → `scanner::full_scan`, `models::EmbeddingProvider` → `EmbeddingProvider`)
- Net: 99 insertions, 563 deletions (-464 lines across 19 files)
- 1031 lib + 355 bin = 1386 tests passing, all 20 integration test binaries compile, zero clippy warnings
- No difficulties encountered

### Task 36.1 — Consolidate identical match arm bodies (commit bf678e4)
- Merged 10 match arms with identical bodies across 6 files:
  - `answer_processor/temporal.rs`: "january" => 1 merged with `_ => 1` wildcard
  - `commands/search/mod.rs`: "relevance" (no-op) merged with `_` (no-op)
  - `organize/plan/merge.rs`: "ORPHAN" merged with `_` (both return Orphan)
  - `patterns.rs` (4 instances): Q1 merged with `_` default, Q4 merged with `_`, month 31-day arms merged with `_`, date length 10 merged with `_`
  - `processor/temporal/parser.rs`: `(Some("="), false, None)` merged with `_` (both PointInTime)
  - `processor/temporal/validation.rs`: two conflict pair arms `(Ongoing, Range)|(Range, Ongoing)` and `(Ongoing, Historical)|(Historical, Ongoing)` merged into single arm
- 1031 lib + 355 bin = 1386 tests passing, zero `match_same_arms` warnings
- No difficulties encountered

### Task 37.1 — Fix case-sensitive file extension comparisons (commit bf678e4)
- Added `ends_with_ext(path, ext)` helper to `commands/utils.rs` using `eq_ignore_ascii_case` for case-insensitive matching
- Replaced 8 `path.ends_with(".ext")` calls across 4 files: `commands/export/mod.rs` (3), `commands/lint/mod.rs` (2), `commands/organize/move.rs` (1), `commands/review/import.rs` (2)
- Initially added helper as local function in `export/mod.rs`, then moved to shared `commands/utils.rs` for reuse across modules
- 1031 lib + 355 bin = 1386 tests passing, zero `case_sensitive_file_extension_comparisons` warnings
- No difficulties encountered

### Task 38.1 — Convert unused-self methods to associated functions (commit bf678e4)
- Converted 9 methods total (8 original + `run_migrations` which became unused-self after its callees were converted):
  - `database/schema.rs` (6): `get_schema_version`, `set_schema_version`, `run_migrations`, `backfill_has_review_queue`, `backfill_fts5`, `check_embedding_migration`
  - `database/stats/compression.rs` (1): `compute_compression_stats`
  - `database/stats/detailed.rs` (2): `compute_word_stats`, `query_boundary_doc`
- Updated all call sites from `self.method(conn)` to `Self::method(conn)`
- Fixed 1 test in `schema.rs` that called `db.get_schema_version(&conn)` → `Database::get_schema_version(&conn)`
- 1031 lib + 355 bin = 1386 tests passing, zero `unused_self` warnings, all integration tests compile
- No difficulties encountered
