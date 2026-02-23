# Factbase Tasks

## Project Status

**Phases 1-47 complete, Phase 48 in progress** (Tasks 1-5 complete; Task 6 pending). Releases: v0.1.0 through v0.4.3. Current Cargo.toml version: v48.5.4.

## Key Learnings from Past Phases (1-47)

### Architecture & Patterns
- Filesystem is truth — markdown files on disk are authoritative, SQLite is the index
- Two-phase scanning: index documents + embeddings (pass 1), detect links via LLM (pass 2)
- `EmbeddingProvider` and `LlmProvider` traits allow swapping backends (Bedrock default, Ollama alternative)
- `cfg!(feature = "bedrock")` in default functions enables compile-time provider switching
- Database uses `r2d2` connection pool for thread-safe access across watcher thread and MCP server
- MCP server supports both stdio transport (`factbase mcp`) and Streamable HTTP (`factbase serve`)
- MCP protocol: `McpRequest.id` is `Option<Value>` (notifications have no id); HTTP returns 202 for notifications; stdio skips writing
- Session management: `Mutex<Option<String>>` in AppState, UUID v4 via `getrandom`, `Mcp-Session-Id` header, 409 on mismatch
- `ProgressReporter` enum (`Cli`/`Mcp`/`Silent`) provides unified progress reporting — CLI writes to stderr, MCP sends JSON via channel
- `ProgressSender` type alias lives in `progress.rs`
- Wiring: MCP tools construct `ProgressReporter::Mcp { sender }`, CLI commands construct `ProgressReporter::Cli { quiet }`, background operations use `ProgressReporter::Silent`
- Progress throttling: `report()` fires every N items (e.g., every 25 files when ≥50 total)
- Shared functions: `lint_all_documents()` with `LintConfig`, `apply_all_review_answers()` with `ApplyConfig` — both MCP and CLI delegate to shared functions with config structs
- `blocking_tool!` macro: 2-arg `($db, $args, $fn)` for simple tools, 3-arg `($db, $args, $reporter, $fn)` for tools with progress
- `ProgressReporter` derives `Clone` — construct once at top of `handle_tool_call`, not per-tool

### Code Quality Conventions
- `FactbaseError` enum with `thiserror` v2 — constructor helpers: `::parse()`, `::not_found()`, `::internal()`, `::config()`, `::embedding()`, `::llm()`, `::ollama()`
- `anyhow::bail!` replaces `process::exit()` and `return Err(anyhow::anyhow!(...))`; `.context()`/`.with_context()` replaces `.map_err(|e| anyhow::anyhow!(...))`
- `unwrap_or_default()` on `row.get()` silently swallows DB errors — always use `?` instead
- `prepare_cached()` on ALL database paths (migration complete across all modules)
- `require_document(id)` and `require_repository(id)` consolidate get+ok_or patterns
- Column constants (`DOCUMENT_COLUMNS`, `SEARCH_COLUMNS`, `REPOSITORY_COLUMNS`) prevent mismatch bugs — only when column list is exact match
- `decode_content_lossy()` consolidates fallback decode pattern across search and stats modules
- Dynamic SQL params: `Vec<&dyn ToSql>` building is clearer than combinatorial match dispatch
- `append_type_repo_filters()` + `push_type_repo_params()` for dynamic WHERE clause building in search modules
- Declarative config validation: `require_non_empty`, `require_positive`, `require_range` helpers in `config/validation.rs`
- `pub(crate)` for internal-only modules; `pub` only for items re-exported from `lib.rs`
- `#[cfg(test)]` gating preferred over `#[allow(dead_code)]` for test-only items; `#![allow(dead_code)]` for entire operational utility modules planned for future use
- No `unwrap()` in production code — use `expect("reason")` or pattern matching
- Single `pub(crate) type BoxFuture` in `lib.rs` — manual desugaring replaces `async-trait` crate
- `write_str!` / `writeln_str!` macros in `lib.rs` wrap `write!`/`writeln!` for infallible String formatting
- `#[macro_export]` macros defined before module declarations are available in lib crate; bin crate needs explicit import
- `map_or()`/`map_or_else()` replaces `map().unwrap_or()`, `is_some_and()` replaces `map_or(false, ..)`, `is_none_or()` replaces `map_or(true, ..)`
- `let...else` replaces `match` with early `continue`/`return` for cleaner control flow
- Method references replace redundant closures: `|v| v.as_u64()` → `Value::as_u64`
- `ends_with_ext(path, ext)` for case-insensitive file extension checks
- Convert unused-`self` methods to associated functions when `&self` isn't used

### Database Patterns
- `DbConn` type alias and `B64` constant in `database/mod.rs`
- `database/documents/` split into crud.rs, list.rs, batch.rs submodules
- `database/compression.rs` extracted for compress/decode helpers, re-exported via `pub(crate) use` from mod.rs
- FTS5 `document_content_fts` virtual table for O(log N) content search; `is_fts_compatible()` dispatches FTS5 vs LIKE
- `ContentSearchParams<'a>` struct bundles 7 search parameters (pattern: bundle when count exceeds 7)
- `ScanContext<'a>` struct bundles scanner/processor/embedding/link_detector/opts/progress
- Schema version 7: FTS5 table + backfill; all write paths keep FTS5 in sync
- Avoid cloning large content in DB write paths: use `let compressed; let content_to_store: &str = ...` pattern

### Testing Patterns
- Lib-crate test helpers: `database/mod.rs` (`test_db`, `test_repo_in_db`, `Document::test_default`)
- Binary-crate test helpers: `commands/test_helpers.rs` (`test_db`, `make_test_repo`, `make_test_doc`)
- Binary crate CANNOT access `pub(crate)` items from lib crate — separate test helper copies required
- `#[cfg(test)]` gated shared helper modules (e.g., `organize/test_helpers.rs`) for cross-module test dedup
- Shared mock providers: `embedding::test_helpers` (`MockEmbedding`, `HashEmbedding`), `llm::test_helpers` (`MockLlm`)
- f32 precision in JSON: use `round()` comparison in tests, not exact equality
- `TestScanSetup` struct in integration tests owns Config/Scanner/Processor/Embedding/LinkDetector/ScanOptions; `context(&self) -> ScanContext<'_>` borrows all fields
- `test_scan_options()` and `test_repo(id, path)` helpers in `tests/common/mod.rs`
- In integration tests, `.unwrap()` is idiomatic — generic `.expect("operation should succeed")` adds no diagnostic value

### Deduplication Patterns
- `to_json()` / `to_summary_json()` methods on model structs for consistent JSON serialization
- `::new()` constructors for structs with repeated default fields
- `as_str() -> &'static str` methods replace standalone conversion functions
- `from_config(&Config)` constructors consolidate repeated field mapping
- `setup_database_only()` eliminates config discards; `setup_db_and_resolve_repos()` for common command setup
- `setup_cached_embedding(&config, timeout)` consolidates embedding+cache setup
- `resolve_repos()`, `confirm_prompt()`, `execute_with_snapshot()` for CLI command patterns
- `read_file`/`write_file`/`remove_file`/`copy_file`/`create_dir`/`remove_dir` in `organize/fs_helpers.rs`
- `load_orphan_entries(repo_path)` consolidates orphan file loading pattern
- Centralize all static `LazyLock<Regex>` patterns in `src/patterns.rs`
- `word_count()` free function in `models/document.rs`
- `content_hash()` free function in `processor/core.rs` (promoted from method, re-exported via lib.rs)
- `cosine_similarity()`, `get_document_embedding()`, `compute_centroid()` in `organize/detect/mod.rs`
- `fetch_active_doc_content()` and `CONTENT_ONLY_QUERY` in `database/stats/mod.rs`
- `parse_since_filter()` in `commands/utils.rs`
- Promote internal helpers to `pub` + re-export through `lib.rs` when integration tests need them

### Review System (Phase 48 — in progress)
- `<!-- reviewed:YYYY-MM-DD -->` HTML comments on fact lines track when a fact was last reviewed — invisible in rendered markdown, parseable by lint
- `REVIEWED_MARKER_REGEX` in `patterns.rs` with `extract_reviewed_date()` helper returns `Option<NaiveDate>`
- Lint generators check for reviewed markers before generating questions — prevents regeneration loops
- `REVIEWED_SKIP_DAYS` constant (180 days) used in temporal, missing, ambiguous, and stale generators
- `add_or_update_reviewed_marker(line, date)` in `patterns.rs`: if marker exists, replaces date; otherwise appends
- `stamp_reviewed_markers(section, date)` stamps all list-item lines in a section — used after LLM rewrite
- `stamp_reviewed_lines(content, line_numbers, date)` stamps specific 1-based line numbers — for dismissed-question path
- Stamping happens after `apply_changes_to_section()` returns rewritten section, before `replace_section()`
- Dismissed questions also get reviewed markers via `stamp_reviewed_lines()` before `remove_processed_questions()`
- Answer type classification (`AnswerType` enum): Dismissal, Deferral, SourceCitation, Confirmation, Correction, Deletion
- `classify_answer()` uses priority-ordered pattern matching: Dismissal → Deletion → Deferral → Correction (explicit) → Confirmation → SourceCitation → Correction (fallback)
- Source citation detection: `SOURCE_PREFIXES` + date-pattern heuristic; `has_correction_indicators()` prevents misclassification
- Only Correction needs LLM rewrite; SourceCitation and Confirmation can be handled deterministically
- `ChangeInstruction::Defer` variant added — treated same as Dismiss for fast path and reviewed-marker stamping
- Deferred questions: `uncheck_deferred_questions()` converts `[x]` → `[ ]` and strips answer lines, keeping questions in queue
- Deferred questions do NOT get reviewed markers (they need to resurface)
- `normalize_review_section()` cleanup pass after apply prevents format degradation
- Deterministic source citation: `apply_source_citations(content, sources)` finds max footnote number, assigns sequential `[^N]`, inserts before `<!-- reviewed:... -->` marker if present
- Footnote definitions placed after last existing `[^N]:` definition, or before `<!-- factbase:review -->` marker, or at end with `---` separator
- When all active instructions are `AddSource` (+ `Delete`), bypasses LLM rewrite entirely — deterministic path handles stamping, unchecking, and question removal
- Deterministic confirmation: `apply_confirmations(content, updates)` handles `UpdateTemporal` (replace old tag) and `AddTemporal` (insert new tag before markers/footnotes)
- Expanded `all_deterministic` check includes `UpdateTemporal` and `AddTemporal` alongside `AddSource` and `Delete`
- `classify_answer()` test coverage: 10 direct tests covering all 6 answer types, including edge cases for source-vs-correction disambiguation and short "yes" prefix confirmation
- `normalize_review_section()` cleanup pass after apply prevents format degradation — dedup headers, strip orphaned `@q[...]` markers, remove empty blockquotes
- `INLINE_QUESTION_MARKER` regex in `patterns.rs` for orphaned `@q[type]` markers outside review section
- `remove_processed_questions()` improved: strips `## Review Queue` heading and `---` separator when all questions removed
- Normalization wired into all 3 write paths in `apply_one_document()` (dismiss/defer, deterministic, LLM rewrite) and `append_review_questions()`
- Lint net-new reporting: `LintDocResult` expanded with `new_questions`, `existing_unanswered`, `existing_answered`, `skipped_reviewed` fields
- `generate_review_questions()` returns `(usize, Option<ExportedDocQuestions>)` tuple to expose new question count
- MCP `lint_repository` response includes: `new_unanswered`, `already_in_queue`, `skipped_reviewed`, `total_questions_generated`
- CLI lint summary: "Generated N total, M new (X already in queue, Y skipped as recently reviewed)"
- `count_reviewed_facts(content)` helper counts fact lines with reviewed markers within 180 days
- `extract_reviewed_date` and `FACT_LINE_REGEX` promoted to `pub` and re-exported through `lib.rs` for binary crate access
- `FactLine.source_refs: Vec<u32>` tracks footnote references per fact line; extracted via `SOURCE_REF_CAPTURE_REGEX` on raw line before cleaning
- `FactWithContext.source_defs: Vec<String>` holds resolved source footnote definitions; built by mapping `source_refs` against parsed `SourceDefinition` entries via `HashMap<u32, String>`
- Cross-validation prompt includes source context per fact ("Sources for this fact: ...") and entity role distinction instruction — distinguishes SUBJECT entities from SOURCE entities to prevent false stale/conflict flags when a source person is inactive

### Entity Deduplication (Phase 46)
- Two extraction patterns: H3+ headings under H2 sections, and bold-name list items
- Two-phase matching: exact normalized name grouping, then 0.85 cosine similarity fuzzy matching
- Three-layer filtering: cross-reference-only, self-mentions, authoritative doc exclusion
- Staleness: Ongoing→today, LastSeen/PointInTime→start_date, Range/Historical→end_date; fallback to `file_modified_at`

### Cross-Validation (Phase 45)
- `extract_all_facts()` gets ALL list items (any indent level) with section tracking
- `clean_fact_text()` does NOT truncate — full text needed for embeddings
- Per-fact semantic search with `RELEVANCE_THRESHOLD = 0.3` filters before LLM calls
- LLM conflict detection: batch 10 facts per call, truncate snippets to 200 chars, strip markdown fences
- Graceful degradation: log and skip malformed LLM responses per batch
- `cross_check_hash` stores `file_hash` — comparing hashes is sufficient for skip logic
- Failed cross-validations do NOT update hash (retried next run); `--dry-run` skips hash update

### Dependency Management
- `tokio` features: only `rt-multi-thread`, `macros`, `net`, `signal`, `sync`, `time` (not `full`)
- `reqwest` 0.12 with `rustls-tls` to align with AWS SDK
- `getrandom` replaces `rand` for document ID generation
- `tokio::signal::ctrl_c()` replaces `ctrlc` crate
- Manual debouncing replaces `notify-debouncer-mini`
- Manual `BoxFuture` desugaring replaces `async-trait` crate
- `lto = "fat"` + `opt-level = "z"` for binary size optimization (36% reduction)
- Simple sliding-window rate limiter replaces `tower_governor`
- `thiserror` v2, `dirs` v6, stdlib recursive walk replaces `walkdir`
- Check `cargo tree --duplicates` before/after dependency changes

### Build & CI
- Feature flags: `full` (default) = progress + compression + mcp + bedrock; `web` separate
- CI: 7 jobs — clippy (all-features), test (default + bin), test-web, test-features matrix (5 combos), readme-validation
- Binary sizes: no-features 6.7MB, +progress +0.1MB, +compression +0.6MB, +mcp +1MB, +bedrock +7MB, full 16MB, +web +1MB

## Active Work

- [x] Phase 48: Review System Robustness — see [tasks/phase48.md](tasks/phase48.md)
  - [x] Task 1: Reviewed-fact markers (1.1-1.6 complete)
  - [x] Task 2: Answer type classification (2.1-2.6 complete)
  - [x] Task 3: Review section cleanup (3.1-3.4)
  - [x] Task 4: Lint net-new reporting (4.1-4.5)
  - [x] Task 5: Cross-validation entity role distinction (5.1-5.4 complete)
  - [x] Task 6: Deferred item surfacing (6.1-6.5 complete)

## Future Considerations

- MCP tools for organize operations (deferred from Phase 10)
- Additional inference provider backends
