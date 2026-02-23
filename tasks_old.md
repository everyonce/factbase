# Factbase Tasks - Historical Archive

This file contains completed task history. See `tasks.md` for active tasks.

---

## Phases 1-6 Summary

### Phase 1: Core Infrastructure (17 tasks)
- Basic file scanning and database storage
- Document ID injection (`<!-- factbase:XXXXXX -->`)
- Title extraction from H1, type from folder name
- SQLite with Arc<Mutex<Connection>>
- 31 unit tests, 12 integration tests

### Phase 2: Embedding & Search (14 tasks)
- Ollama embeddings (nomic-embed-text, 768 dims)
- sqlite-vec for vector search
- Two-phase scanning: index + LLM link detection
- Search CLI with type/repo filters

### Phase 3: File Watching & MCP Server (13 tasks)
- notify crate with 500ms debounce
- MCP server on localhost:3000 (4 tools)
- `factbase serve` command
- Graceful shutdown on Ctrl+C

### Phase 4: Multi-Repo & Polish (15 tasks)
- Multi-repository support
- repo add/remove/list commands
- GitHub Actions CI
- 47 unit tests, 6 multi-repo tests, 9 E2E tests

### Phase 5: Comprehensive E2E Testing (16 tasks)
- 12 new test files
- TESTING.md documentation
- All tests require Ollama (no skip behavior)
- 83 unit tests + integration suite

### Phase 6: Embedding Model Upgrade (13 tasks)
- qwen3-embedding:0.6b (1024 dims, 32K context)
- Document chunking for >100K chars
- Database schema migration
- Release: v0.2.0

---

## Phase 7: Fact Document Format & Review System (v0.3.0)

**19 main tasks, all complete**

Key Features:
- Temporal tags (`@t[...]`) with 6 tag types
- Source footnotes (`[^N]`) for provenance
- Review system with 6 question types
- `lint --review` and `review --apply` commands
- Temporal-aware search flags
- 3 new MCP tools for review operations

---

## Post-Phase 7 Optimizations (Tasks 16-151)

### Performance Improvements
- Batch embedding (10 docs per request), batch link detection (5 docs per request)
- Parallel file I/O via rayon, query embedding cache (LRU), document metadata cache, stats cache

### Database Enhancements
- r2d2 connection pooling (1-32 connections), spawn_blocking for MCP handlers
- zstd compression support, health check endpoint

### CLI Additions (30+ flags)
- `--json`, `--quiet`, `--verbose`, `--dry-run`, `--stats`, `--since`, `--watch`
- `--filter`, `--exclude`, `--sort`, `--check-duplicates`, `--min-similarity`

### MCP Expansion (4 → 17 tools)
- search_knowledge, search_content, search_temporal
- get_entity, list_entities, get_perspective, list_repositories, get_document_stats
- create_document, update_document, delete_document, bulk_create_documents
- get_review_queue, answer_question, bulk_answer_questions, generate_questions

### Code Quality
- Eliminated all `process::exit()` calls, reduced `unwrap()` to test code only
- Consolidated helpers in `src/output.rs`, errors in `src/error.rs`
- TestContext helper for tests, shared `run_scan()` helper

### Build Optimization
- Binary size: 15MB → 11MB (27% reduction)
- Feature flags: full, progress, compression, mcp
- CI feature matrix (5 combinations)

### Documentation
- CHANGELOG.md, examples/perspective.yaml, README badges, troubleshooting section

---

## Phase 8: Code Organization (7 tasks)

**Goal:** Improve code maintainability by splitting large files

| File | Before | After | Reduction | Submodules |
|------|--------|-------|-----------|------------|
| processor.rs | 2984 | 73 | 97% | 6 (core, temporal, sources, review, chunks, stats) |
| lint.rs | 1657 | 1081 | 35% | 4 (args, checks, review, output) |
| database.rs | 3407 | 484 | 86% | 8 (mod, schema, documents, repositories, links, embeddings, search, stats) |

**Pattern:** Create directory, extract one module at a time, run tests after each, use `pub use submodule::*` for re-exports. All public APIs preserved.

### Post-Phase 8 Tasks (77-82)
- Task 77: Reduce allocations in temporal validation (64% reduction in clones)
- Task 78: Reduce allocations in link detection (80% reduction in clones)
- Task 79: Add --json output to init command
- Task 81: Split doctor.rs (456 → 202 lines, 56% reduction)
- Task 82: Add unit tests for completions.rs (7 new tests)

---

## Phase 9: SQLite Performance Optimizations (16/16 subtasks)

| Task | Subtasks | Impact |
|------|----------|--------|
| 1. Batch Link Fetching | 4/4 | High - 2 queries vs 2*N for export/lint/filters |
| 2. Prepared Statement Caching | 4/4 | Medium - 10-20% on repeated queries |
| 3. Index on file_modified_at | 2/2 | Medium - faster --since filters |
| 4. Word Count Optimization | 3/3 | Medium - avoids content decompression in stats |
| 5. Documentation & Benchmarks | 3/3 | ~1.8x improvement measured with Criterion |

**Key implementations:**
- `get_links_for_documents()` batch method with HashMap return
- `prepare_cached()` on all hot paths (documents, links, embeddings, stats)
- SCHEMA_VERSION 3: idx_documents_modified index
- SCHEMA_VERSION 4: word_count column with backfill command
- `needs_X()` / `fetch_X_if_needed()` helper pattern for conditional batch fetching

---

## Phase 10: Self-Organizing Knowledge Base (26/26 subtasks)

**Goal:** Manual `factbase organize` subcommand for structural reorganization with fact-level accounting

**Design Principles:** Manual only, books must balance (Facts In = Facts Out + Orphans), orphans explicit, atomic with rollback.

| Task | Subtasks | Key Files |
|------|----------|-----------|
| 1. Fact Extraction | 3/3 | types.rs, extract.rs |
| 2. Merge | 4/4 | detect/merge.rs, plan/merge.rs, execute/merge.rs, links.rs |
| 3. Split | 3/3 | detect/split.rs, plan/split.rs, execute/split.rs |
| 4. Move/Retype | 3/3 | detect/misplaced.rs, execute/move.rs, execute/retype.rs |
| 5. Orphan Management | 2/2 | orphans.rs, review.rs |
| 6. CLI Commands | 6/6 | commands/organize/*.rs |
| 7. Audit & Safety | 3/3 | audit.rs, snapshot.rs, verify.rs |
| 8. MCP Tools | 0/2 | (optional, deferred) |

**Key patterns:** FactLedger for accounting, `write_orphans()` shared function, audit logs in `.factbase/reorg-log/`, snapshots in `.factbase/snapshots/`, verification before commit with rollback on failure.

---

## Phase 11: Web Interface for Human-in-the-Loop (28/28 subtasks)

**Goal:** Static SPA served by factbase on configurable port, reusing all existing CLI logic

| Task | Subtasks | Impact |
|------|----------|--------|
| 1. Web Server Infrastructure | 4/4 | Axum on port 3001, rust-embed, SPA routing |
| 2. API Endpoints | 4/4 | 17 JSON endpoints wrapping existing functions |
| 3. Frontend SPA | 4/4 | Vite + TypeScript + Tailwind, hash-based routing |
| 4. Review Queue UI | 4/4 | Question management with bulk mode, preview panel |
| 5. Organize Suggestions UI | 4/4 | Merge/split preview, orphan assignment |
| 6. UI Polish | 4/4 | Responsive, keyboard nav (j/k/Enter/Escape/g+d/r/o), WCAG a11y |
| 7. Build Integration | 4/4 | build.rs, CI test-web job, 56 Vitest frontend tests |

**Key patterns:** Feature-gated `web` flag, `WebConfig` (enabled: false, port: 3001), `ApiError` for consistent errors, `spawn_blocking` for DB ops, `rust-embed` for static assets, hash-based SPA routing, page lifecycle (cleanup/init), toast notifications, skeleton loaders.

---

## Phase 12: Open Source Readiness (15/15 subtasks)

**Goal:** Prepare factbase for public release — blockers first, then adoption improvements, then polish.

### 12.1 — Add LICENSE file (Complete - 2026-02-08)
- Created MIT LICENSE file with "Copyright (c) 2026 Factbase Contributors"
- README badge links to LICENSE file — confirmed match

### 12.2 — Add crates.io metadata to Cargo.toml (Complete - 2026-02-08)
- Added 8 metadata fields: description, license (MIT), repository, homepage, readme, keywords (5), categories (3), authors
- `cargo package --list --allow-dirty` runs clean — no metadata warnings
- Repository/homepage use placeholder `github.com/example/factbase` (task 12.3 will replace)
- Keywords: knowledge-base, semantic-search, mcp, markdown, ai
- Categories: command-line-utilities, database, text-processing
- All 737 lib + 347 bin tests pass
- Commit: ac2ea56

### 12.3 — Replace placeholder GitHub URLs (Complete - 2026-02-08)
- Replaced `github.com/example/factbase` with `gitea.home.everyonce.com/daniel/factbase` in 5 files (README.md, Cargo.toml, CONTRIBUTING.md, docs/quickstart.md, src/main.rs)
- Removed non-functional GitHub Actions badge and placeholder test count badge from README (CI runs on Gitea, not GitHub)
- Kept static Rust version and License badges
- Used Gitea-style URL for blob links (`/src/branch/main/` instead of `/blob/main/`)
- Git remote was already correctly set — no change needed
- `cargo check`, `cargo package --list --allow-dirty`, and all 1084 tests pass
- `tasks_old.md` historical references left as-is (archival record)

### 12.4 — Update examples/config.yaml to default to Bedrock (Complete - 2026-02-08)
- Changed default provider from Ollama to Bedrock with `amazon.titan-embed-text-v2:0` (embedding) and `us.anthropic.claude-3-5-haiku-20241022-v1:0` (LLM)
- Replaced individual `.git/**`/`.factbase/**` ignore patterns with single `.*/**` glob
- Added commented-out Ollama config block referencing `docs/inference-providers.md`
- Removed `llm.max_content_length` and `llm.batch_size` which were not actual config fields
- `base_url` field overloaded for AWS region and HTTP URL — task 12.10 will add `region` alias
- All 57 config tests pass
- Commit: 5cdd6e2

### 12.5 — Make default model names feature-aware (Complete - 2026-02-08)
- Used `cfg!(feature = "bedrock")` in `default_provider()`, `default_base_url()`, `default_embedding_model()`, and `default_llm_model()` functions
- Both `EmbeddingConfig::default()` and `LlmConfig::default()` call these functions, keeping serde defaults and Default impls in sync
- With bedrock feature: provider=bedrock, base_url=us-east-1, models=Titan/Claude. Without: provider=ollama, base_url=localhost:11434, models=qwen3/rnj-1
- Fixed `review_model_default_uses_llm_model` test to compare against `config.llm.model` for feature-independence
- Doctor command still Ollama-specific but runs gracefully with bedrock defaults
- Commit: 6879d53

### 12.6 — Add `bedrock` to the `full` feature set (Complete - 2026-02-08)
- Added `bedrock` to `full` feature list: `full = ["progress", "compression", "mcp", "bedrock"]`
- README Installation simplified: `cargo build --release` now includes Bedrock by default (no `--features bedrock` needed)
- README feature table updated: `full` description says "(includes Bedrock)", bedrock row kept for visibility, no-features row clarified as "CLI-only with Ollama backend"
- `--no-default-features` confirmed: no AWS deps, 654 lib tests pass
- Default build: 742 lib + 347 bin tests pass (1089 total)

### 12.7 — Add bedrock feature to CI (Complete - 2026-02-08)
- Added `"--no-default-features --features bedrock"` to `test-features` matrix in `.github/workflows/ci.yml`
- This tests bedrock in isolation (without progress, compression, mcp) — 659 lib tests pass
- `"--features full"` already in matrix covers the combined `full+bedrock` case (since 12.6 made `full` include `bedrock`)
- Default-features jobs (`check`, `clippy`, `build`) also compile with bedrock since `default = ["full"]`
- Clippy clean with `--no-default-features --features bedrock`
- No difficulties — straightforward single-line addition to the matrix

### 12.8 — Trim README, extract CLI reference (Complete - 2026-02-08)
- Created `docs/cli-reference.md` (453 lines) with full CLI command reference including all commands, flags, examples, and review workflow
- README trimmed from 1055 → 182 lines (83% reduction, well under 300-line target)
- Removed from README: detailed CLI flags/examples, Review Workflow section, Review Queue section, Web UI section, detailed Configuration (pool_size guide), Benchmarks section, detailed Document Format (temporal tags table, source attribution)
- Kept in README: Features, Prerequisites, Installation (with feature flags table), Quick Start, CLI summary table, abbreviated Configuration, MCP Integration table, Document Format (brief), Troubleshooting (4 key issues), Architecture, License
- All 1089 tests pass (742 lib + 347 bin)

### 12.9 — Clarify that custom syntax is optional (Complete - 2026-02-08)
- Added "Plain markdown is all you need" section to `docs/quickstart.md` between "Keep it updated" and "What just happened?"
- README features list already had core/optional distinction from task 12.8 — no changes needed
- Verified plain markdown path: `extract_id` returns None → ID injected, `extract_title` falls back to filename, `derive_type` defaults to "document", temporal/source/review parsers return empty on plain content
- All 742 lib tests pass, 16 processor core tests + 11 scanner tests confirm plain markdown handling
- Commit: 4482d00

### 12.10 — Rename `base_url` to `region` for Bedrock config (Complete - 2026-02-08)
- Added `region: Option<String>` field to both `EmbeddingConfig` and `LlmConfig` with `#[serde(default, skip_serializing_if = "Option::is_none")]`
- Added `effective_base_url()` method on both structs: returns `region` if set, else falls back to `base_url`
- Updated all consumers in `setup.rs` (3 provider setup functions), `doctor/mod.rs`, `serve.rs` to use `effective_base_url()`
- Updated docs: `examples/config.yaml`, `README.md`, `docs/quickstart.md`, `docs/inference-providers.md`, `.kiro/steering/coding-conventions.md`
- Backward compatible: existing configs with `base_url` continue to work unchanged
- 3 new tests: region override, fallback to base_url, YAML deserialization of region field
- 745 lib tests + 347 binary tests pass (1092 total)

### 12.11 — Improve first-run experience (Complete - 2026-02-08)
- Added `Config::config_file_exists()` method and `print_first_run_notice()` in setup.rs
- Notice prints to stderr when no config file found: shows path, default provider, and suggests `factbase doctor`
- Called from `setup_database()` and `find_repo_with_config()` — covers scan, search, serve, status, etc.
- Doctor now detects Bedrock provider and skips Ollama checks; prints model/region info and Bedrock console URL for model access
- Updated doctor about text and module docs to be provider-agnostic
- Added `--config` flag to `factbase init` that generates `~/.config/factbase/config.yaml` with defaults
- JSON output includes `config_created` field when `--config` used
- All existing tests updated for new `config` field; 745 lib + 347 binary tests pass
- Builds clean with `--features full`, `--no-default-features --features bedrock`, and `--no-default-features`

### 12.12 — Add `cargo install` instructions (Complete - 2026-02-08)
- Verified both `cargo install --path .` (full features including bedrock) and `cargo install --path . --no-default-features` (Ollama-only) succeed
- README Installation section now uses `cargo install --path .` instead of `cargo build --release`, with `### From source` subheading
- Quickstart Install section updated similarly, showing both full and minimal install commands
- Kept `<!-- Once published: cargo install factbase --features full -->` comment for future crates.io publish
- Feature flag examples also updated from `cargo build` to `cargo install`
- 745 lib + 347 binary tests pass (1092 total)
- Commit: 5a45bbf

### 12.13 — Inbox integration for review --apply (Complete - 2026-02-08)
- New `src/answer_processor/inbox.rs` module: `extract_inbox_blocks()`, `strip_inbox_blocks()`, `build_inbox_prompt()`, `apply_inbox_integration()`
- `InboxBlock` struct tracks content and line positions for each block
- `cmd_review_apply` now has a second pass after review questions: collects documents with inbox blocks via `collect_inbox_documents()`, processes each with LLM
- Re-reads files from disk before inbox processing (in case review question pass modified them)
- `--dry-run` shows inbox block content and line ranges without calling LLM
- `--verbose` shows full inbox content before LLM call
- Multiple inbox blocks per document supported (combined into single LLM prompt)
- Respects `--since` and `--repo` filters for inbox documents
- 10 new unit tests: single/multiple/empty/unclosed/multiline block parsing, stripping, prompt building
- Documented in: quickstart.md, authoring-guide.md, agent-authoring-guide.md, fact-document-format.md
- 755 lib tests + 347 binary tests pass (1102 total)
- Commit: 297479e

### 12.14 — Clean up test badge count (Complete - 2026-02-08)
- Badge had been previously removed from README; re-added static shields.io badge with accurate count: 1102 passing
- Verified actual counts: 755 lib + 347 bin = 1102 (without web); 816 lib + 354 bin = 1170 (with web)
- Updated test counts in current-state.md and tasks.md (web feature counts were stale: 782→816 lib, 347→354 bin, 1129→1170 total)
- Used static badge since CI runs on Gitea, not GitHub Actions (dynamic shields.io badges not straightforward)

### 12.15 — Add SECURITY.md (Complete - 2026-02-08)
- Created `SECURITY.md` at project root with five sections: Reporting Vulnerabilities, Data Storage, Network Communication, MCP Server, File System Access
- Vulnerability reporting directs to private repo issues with 72-hour acknowledgment commitment
- Plaintext SQLite warning includes mitigation advice (filesystem permissions, full-disk encryption)
- Bedrock HTTPS via AWS SDK documented; Ollama localhost HTTP noted with remote endpoint caveat
- MCP/web server localhost-only binding and lack of authentication documented
- File modification behavior (`--dry-run` recommendation) included
- No difficulties — straightforward documentation task
- All 1102 tests pass (755 lib + 347 bin)

**Key Learnings from Phase 12:**
- `cfg!(feature = "X")` in default functions enables compile-time provider switching without runtime overhead
- Deprecation pattern: add new `Option<T>` field → `effective_*()` method checks new first, falls back to old → update all consumers
- `.*/**` glob catches all hidden directories cleanly (`.git`, `.factbase`, `.obsidian`)
- Gitea-style blob links use `/src/branch/main/` not GitHub's `/blob/main/`
- Static shields.io badges work when CI platform differs from GitHub
- Multi-pass processing pattern: re-read files from disk between passes when earlier pass may modify content
- `print_first_run_notice()` to stderr so it doesn't break JSON/piped output
- Provider-agnostic doctor: detect provider from config, run appropriate checks

---

## Phase 13: Review Question Quality — Reduce NO_CHANGE Waste (4/4 Complete - 2026-02-08)

**Goal:** Targeted fixes to question generators based on analysis of ~500 NO_CHANGE answers. Each task addresses a specific pattern where the review agent searched, confirmed nothing changed, and dismissed the question.

### 13.1 — Temporal: skip if line has recent @t[~] verification (Complete - 2026-02-08)
- Added `has_recent_verification(line, today)` helper in `src/question_generator/temporal.rs` — scans for `@t[~DATE]` tags within 180 days using `TEMPORAL_TAG_FULL_REGEX`
- Added `parse_verification_date()` supporting all date formats (YYYY, YYYY-MM, YYYY-MM-DD, YYYY-QN) with generous end-of-period interpretation
- Integrated check into stale ongoing branch: `is_stale_ongoing() && !has_recent_verification()`
- 7 new tests, lib tests: 762 (up from 755)

### 13.2 — Stale: cross-check @t[~] before flagging source age (Complete - 2026-02-08)
- Made `has_recent_verification()` `pub(crate)` and imported in stale.rs
- Added `!has_recent_verification(line, today)` guard to source-date staleness check
- Reuses same 180-day threshold from task 13.1
- 3 new tests, lib tests: 765 (up from 762)

### 13.3 — Conflict: skip roster lines with cross-references (Complete - 2026-02-08)
- Early return in `facts_may_conflict()` when either fact contains `[[id]]` link pattern
- Reuses existing `MANUAL_LINK_REGEX` from `src/patterns.rs`
- 3 new tests, lib tests: 768 (up from 765)

### 13.4 — Build, test, and verify reduction (Complete - 2026-02-08)
- Fixed clap `--verbose` global arg conflict: renamed to `--detailed` in review and organize commands
- All 1115 tests passing (768 lib + 347 binary)
- `lint --review` on 73-doc knowledge base: 10 questions generated
- Phase 13 suppressions verified: ~17 temporal, ~239 stale, and roster conflict lines properly suppressed
- Without suppressions, question count would have been significantly higher (estimated 30-50+ additional questions)

**Key Learnings:**
- Before generating a question, check if the line already has a recent `@t[~DATE]` verification tag (within 180 days)
- `has_recent_verification()` is `pub(crate)` so multiple generators can reuse it
- Reuse existing regex patterns from `src/patterns.rs` rather than writing new ones
- Lines with `[[id]]` cross-references are roster entries, not conflicting facts

---

## Phase 14: Code Deduplication & Cleanup (12/12 Complete - 2026-02-08)

**Goal:** Reduce repetition across the codebase by extracting shared helpers and consolidating duplicated patterns.

| Task | Summary | Net Lines |
|------|---------|-----------|
| 14.1 | Deduplicate test_db() — 7 lib copies removed, 2 binary kept | -35 |
| 14.2 | Extract resolve_bedrock_region() in setup.rs | -5 |
| 14.3 | Document::test_default() for test construction | -28 |
| 14.4 | Deduplicate test_repo() — 6 organize copies → test_repo_in_db | -70 |
| 14.5 | Shared lint test helpers (make_test_doc, make_test_doc_with_id) | -27 |
| 14.6 | Fix all clippy warnings (zero warnings on --all-features) | ~0 |
| 14.7 | Extract DOC_ID_REGEX to patterns.rs | ~0 |
| 14.8 | Extract iter_fact_lines() for question generators | -14 |
| 14.9 | Consolidate binary-crate test_db() into commands/test_helpers.rs | 0 |
| 14.10 | Consolidate binary-crate Repository helpers | +2 |
| 14.11 | Consolidate binary-crate Document helpers | +6 |
| 14.12 | Add doc comments to 128 public API items in core modules | +128 |

**Key patterns:**
- Canonical lib-crate test helpers in `database/mod.rs` (`test_db`, `test_repo_in_db`, `Document::test_default`)
- Binary-crate shared helpers in `commands/test_helpers.rs` (`test_db`, `make_test_repo`, `make_test_doc`)
- Lint-specific helpers in `commands/lint/execute/test_helpers.rs` (`make_test_doc`, `make_test_doc_with_id`)
- Binary crate CANNOT access `pub(crate)` items from lib crate — separate copies required
- `use ... as alias` pattern to avoid changing call sites when consolidating
- `iter_fact_lines()` eliminates FACT_LINE_REGEX + extract_fact_text boilerplate in 4 generators
- `RUSTFLAGS="-W missing-docs" cargo check --lib` to find undocumented public items

**Final test counts:** 774 lib + 347 binary = 1121 (without web); 835 lib + 354 binary = 1189 (with web); 73+ integration; 56 frontend.

---

## Phase 15: Code Quality & Robustness (14/14 Complete - 2026-02-08)

**Goal:** Improve code quality by replacing `process::exit()` calls, reducing MCP tool dispatch boilerplate, eliminating production `expect()` calls, consolidating duplicated setup patterns, cleaning up error-swallowing patterns, and extracting shared constants/helpers.

### 15.1 — Replace process::exit() calls with proper error propagation (Complete)
- Replaced all 7 `std::process::exit(1)` calls across 4 command files with `anyhow::bail!` or `anyhow::anyhow!`
- Files: `serve.rs` (4 calls), `scan/mod.rs` (1), `scan/stats.rs` (1), `scan/verify.rs` (1)
- `serve.rs` health check was most complex — 4 exit paths collapsed into linear flow with early returns

### 15.2 — Reduce MCP tool dispatch boilerplate with a macro (Complete)
- Added `blocking_tool!` macro in `src/mcp/tools/mod.rs` with two arms: `($db, $args, $fn)` for 12 tools, `($db, $fn)` for `list_repositories`
- 2 async tools left as-is since they need the embedding provider

### 15.3 — Replace production expect() calls with proper error handling (Complete)
- `src/mcp/server.rs` and `src/mcp/tools/review/answer.rs`: replaced `.expect()` with `.ok_or(...)?`
- Remaining `expect()` calls are infallible by construction (LazyLock regex, NonZeroUsize with known constant, etc.)

### 15.4 — Extract `open_database` helper to deduplicate Database setup (Complete)
- Added `DatabaseConfig::is_compression_enabled()` and `Config::open_database(path)` — updated 6 call sites, net -28 lines

### 15.5 — Deduplicate LLM provider creation in setup.rs (Complete)
- Extracted shared `create_llm()` helper, net -18 lines

### 15.6 — Extract dynamic SQL parameter builder to eliminate combinatorial match dispatch (Complete)
- Replaced 5 combinatorial `match` blocks (28 arms total) with `Vec<&dyn ToSql>` parameter building
- Files: title.rs, semantic.rs, content.rs, documents.rs
- `embedding.as_bytes()` temporary lifetime required extracting to a named binding

### 15.7 — Extract `parse_rfc3339_utc` helpers to deduplicate timestamp parsing (Complete)
- Added `parse_rfc3339_utc(s)` and `parse_rfc3339_utc_opt(s)` in `database/mod.rs`, replaced 10 inline chains
- Nested submodules use `crate::database::helper_fn`, siblings use `super::helper_fn`

### 15.8 — Replace `unwrap_or_default()` with `?` in search row mapping (Complete)
- Replaced 13 `row.get(N).unwrap_or_default()` calls with `row.get(N)?` in title.rs, semantic.rs, content.rs
- Changed `row_to_search_result_with_chunk` return type to `Result<SearchResult, FactbaseError>`

**Key Learnings:**
- `anyhow::bail!` is the cleanest replacement for `process::exit()` when `main()` returns `anyhow::Result<()>`
- Declarative macros with multiple arms handle slight variations in repeated patterns (db+args vs db-only)
- Use regular comments (not `///`) on macros to avoid clippy `doc_list_item_without_indentation` warning
- Encapsulate repeated multi-step setup patterns into `Config` methods (e.g., `Config::open_database()`)
- Dynamic SQL params: inline `Vec<&dyn ToSql>` building is clearer than combinatorial match dispatch
- Shared timestamp helpers: nested submodules use full `crate::` path, siblings use `super::`
- `unwrap_or_default()` on `row.get()` silently swallows DB errors — always use `?` instead

**Test counts at 15.8 completion:** 774 lib + 347 binary = 1121 (without web); 835 lib + 354 binary = 1189 (with web). Zero clippy warnings.

### 15.9 — Replace `unwrap_or_default()` with `?` in `database/links.rs` row mapping (Complete - 2026-02-08)
- Changed `row_to_link()` return type from `Link` to `Result<Link, FactbaseError>`, replaced 3 `unwrap_or_default()` with `?`
- Switched `get_links_from`/`get_links_to` from `query_map` + `filter_map` to `while let` loop with `?` — same pattern as `get_links_for_documents` already used
- No difficulties — straightforward application of the same pattern from 15.8
- All 774 lib + 347 binary tests pass, zero clippy warnings
- Commit: 4112916

### 15.10 — Extract shared `generate_snippet()` helper for search modules (Complete - 2026-02-08)
- Added `pub(crate) fn generate_snippet(content: &str) -> String` in `database/search/mod.rs`
- Replaced inline snippet logic in `title.rs` and `semantic.rs` with calls to the shared helper
- semantic.rs chunk-aware slicing kept in-place, only the snippet generation delegated
- All 774 lib tests + 347 binary tests pass, zero clippy warnings

### 15.11 — Consolidate `decode_content` + `unwrap_or` pattern into helper (Complete - 2026-02-08)
- Added `decode_content_lossy(stored: String) -> String` helper in `database/mod.rs`
- Replaced 7 inline `decode_content(&x).unwrap_or(x)` patterns across search modules (title.rs, semantic.rs, content.rs) and stats modules (detailed.rs x2, temporal.rs, sources.rs)
- `documents.rs` uses `decode_content` with `?` (error propagation) — different use case, left unchanged
- Added unit test; 775 lib tests pass, zero clippy warnings
- Commit: 9273627

### 15.12 — Extract `DOCUMENT_COLUMNS` constant in `database/documents.rs` (Complete - 2026-02-08)
- Added `DOCUMENT_COLUMNS` constant for the 10-column SELECT list used by `row_to_document()`
- Replaced 4 inline column lists in SELECT queries with `format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE ...")`
- INSERT query retains inline list since it includes extra `word_count` column
- 836 lib + 354 binary tests pass, zero clippy warnings

### 15.13 — Add `Database::require_document()` to consolidate get+unwrap pattern (Complete - 2026-02-08)
- Added `pub fn require_document(&self, id: &str) -> Result<Document, FactbaseError>` on `Database`
- Replaced 15 instances of `get_document(id)?.ok_or_else(|| ...)` across 14 files
- Removed 5 now-unused `doc_not_found` imports
- Net -11 lines (39 added, 50 removed)
- 836 lib + 354 binary tests pass, zero clippy warnings
- Commit: f6e3ffa

### 15.14 — Extract `SEARCH_COLUMNS` constant in `database/search/mod.rs` (Complete - 2026-02-08)
- Added `pub(crate) const SEARCH_COLUMNS: &str = "id, title, doc_type, file_path, content"` in `database/search/mod.rs`
- Replaced inline column list in `title.rs` only — `content.rs` adds `repo_id` (different columns), `semantic.rs` uses joins (different structure)
- Column constants only apply cleanly when column list is an exact match
- 836 lib + 354 binary tests pass, zero clippy warnings
- Commit: 7cd883f

**Updated Key Learnings (Phase 15 complete):**
- `anyhow::bail!` is the cleanest replacement for `process::exit()` when `main()` returns `anyhow::Result<()>`
- Declarative macros with multiple arms handle slight variations in repeated patterns (db+args vs db-only)
- Use regular comments (not `///`) on macros to avoid clippy `doc_list_item_without_indentation` warning
- Encapsulate repeated multi-step setup patterns into `Config` methods (e.g., `Config::open_database()`)
- Dynamic SQL params: inline `Vec<&dyn ToSql>` building is clearer than combinatorial match dispatch
- Shared timestamp helpers: nested submodules use full `crate::` path, siblings use `super::`
- `unwrap_or_default()` on `row.get()` silently swallows DB errors — always use `?` instead
- `decode_content_lossy()` consolidates fallback decode pattern — check stats modules too, not just search modules
- Extract column constants (`DOCUMENT_COLUMNS`, `SEARCH_COLUMNS`) to prevent mismatch bugs — only when column list is an exact match
- `require_document(id)` consolidates `get_document(id)?.ok_or_else(|| ...)` — replaced 15 instances across 14 files

**Test counts at Phase 15 completion:** 836 lib + 354 binary = 1190 (with all features). Zero clippy warnings.

---

## Phase 16: Code Deduplication & Consolidation (18/20 subtasks complete - 2026-02-08)

**Goal:** Reduce code repetition by extracting shared helpers for duplicated patterns found across MCP search tools, question generator modules, and CLI commands.

| Task | Summary | Impact |
|------|---------|--------|
| 16.1 | Extract shared temporal filter helpers in MCP search tools | -31 lines, 3 duplicated blocks eliminated |
| 16.2 | Add `ReviewQuestion::new()` constructor | Replaced 9 struct literals across 7 files |
| 16.3 | Consolidate `since_filter` parsing to one-liner style | -6 lines in review commands |
| 16.4 | Add `QuestionType::as_str()`, delete 2 duplicate functions | -18 lines, single source of truth |
| 16.5 | Add `OutputFormat::resolve()` for --json flag handling | Replaced 11 identical 5-line blocks |
| 16.6 | Add `setup_database_only()` to eliminate config discards | Replaced 12 `let (_config, db)` sites |
| 16.7 | Extract `resolve_repos()` helper | Replaced 2 identical 8-line blocks |
| 16.8 | Add `Database::require_repository()` | Replaced 6 get+ok_or sites across 4 files |
| 16.9 | Eliminate redundant `repo_not_found_error()` | Deleted anyhow wrapper, 4 sites updated |
| 16.10 | Serde rename + `to_value()` for search results | Replaced manual JSON building in 2 search tools |
| 16.11 | `Document::to_summary_json()` for entity.rs | Replaced manual 4-field JSON in 2 MCP tools |
| 16.12 | `collect_active_documents()` helper | Replaced 3 identical doc-fetching blocks in organize detect modules |
| 16.13 | `cosine_similarity()` shared helper | Extracted from split.rs and misplaced.rs into detect/mod.rs |
| 16.14 | `get_document_embedding()` shared helper | Moved from misplaced.rs to detect/mod.rs for reuse |
| 16.15 | `REPOSITORY_COLUMNS` constant | Replaced 3 inline column lists in repositories.rs |
| 16.16 | `prepare_cached()` in repositories.rs | Replaced 4 `prepare()` calls on hot paths |
| 16.17 | `compute_centroid()` shared helper | Extracted from misplaced.rs to detect/mod.rs |
| 16.18 | `prepare_cached()` in stats/ and documents.rs | Replaced `prepare()` with `prepare_cached()` for static SQL |

### 16.1 — Extract shared temporal filter helpers in MCP search tools (Complete - 2026-02-08)
- Extracted 3 shared helpers into `mcp/tools/search/mod.rs`: `parse_during_range()`, `fetch_docs_content()`, `apply_temporal_filter()`
- Replaced duplicated inline logic in both `search_knowledge.rs` and `search_temporal.rs`
- Net change: -31 lines. Commit: 0999b27

### 16.2 — Add ReviewQuestion::new() constructor (Complete - 2026-02-08)
- Added `ReviewQuestion::new(question_type, line_ref, description)` in `models/question.rs`
- Replaced all 9 struct literal construction sites across 7 files in `question_generator/`
- 1191 tests passing. Commit: 4931089

### 16.3 — Consolidate since_filter parsing pattern in commands (Complete - 2026-02-08)
- Replaced Style B `if let Some(ref since_str)` blocks with Style A one-liner in `review/apply.rs` and `review/status.rs`
- `lint/mod.rs` and `scan/mod.rs` left as-is (have side effects in the block)
- Net -6 lines. Commit: 5c09a09

### 16.11 — Add Document::to_summary_json() for entity.rs (Complete - 2026-02-08)
- Added `Document::to_summary_json()` method returning `{id, title, type, file_path}` JSON
- Replaced manual JSON building in `list_entities` and `get_document_stats` in `entity.rs`
- `get_document_stats` gained `file_path` field (additive, backward-compatible)
- 841 lib + 358 binary = 1199 tests passing. Commit: 7d9670c

### 16.12 — Extract `collect_active_documents()` helper for organize detect modules (Complete - 2026-02-08)
- Extracted shared helper into `organize/detect/mod.rs` for fetching active (non-deleted) documents
- Replaced 3 identical doc-fetching blocks in `merge.rs`, `split.rs`, and `misplaced.rs`
- 841 lib + 358 binary = 1199 tests passing. Commit: 213b0ee

### 16.13 — Extract shared `cosine_similarity()` from split.rs and misplaced.rs into detect/mod.rs (Complete - 2026-02-08)
- Extracted shared `cosine_similarity()` function into `organize/detect/mod.rs`
- Replaced duplicate implementations in `split.rs` and `misplaced.rs`
- 836 lib + 358 binary = 1194 tests passing. Commit: 88c5f26

### 16.14 — Move `get_document_embedding()` from misplaced.rs to detect/mod.rs for reuse (Complete - 2026-02-08)
- Moved general-purpose embedding fetch from `misplaced.rs` to `detect/mod.rs` as `pub(crate)`
- Available for reuse by other detect modules (merge, split) if they need document embeddings
- 836 lib + 358 binary = 1194 tests passing. Commit: 6bc5564

### 16.15 — Extract `REPOSITORY_COLUMNS` constant in `database/repositories.rs` (Complete - 2026-02-08)
- Added `REPOSITORY_COLUMNS` constant for the 7-column SELECT list used by `row_to_repository()`
- Replaced 3 inline column lists in SELECT queries with `format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE ...")`
- Follows same pattern as `DOCUMENT_COLUMNS` (Phase 15.12) and `SEARCH_COLUMNS` (Phase 15.14)
- 836 lib + 358 binary = 1194 tests passing. Commit: 6fc0f17

### 16.16 — Use `prepare_cached()` for repeated queries in `database/repositories.rs` (Complete - 2026-02-08)
- Replaced 4 `prepare()` calls with `prepare_cached()`: `get_repository()`, `list_repositories()`, `remove_repository()` (doc ID lookup), `get_repository_by_path()`
- `get_repository()` is the hottest path since `require_repository()` delegates to it
- Follows established pattern from documents.rs, stats/, links.rs, embeddings.rs
- 836 lib + 358 binary = 1194 tests passing. Commit: 250c207

### 16.17 — Extract `compute_centroid()` from misplaced.rs to detect/mod.rs (Complete - 2026-02-08)
- General vector math utility; completes consolidation of shared vector operations
- Follows same pattern as 16.13 (cosine_similarity) and 16.14 (get_document_embedding)
- 836 lib + 358 binary = 1194 tests passing. Commit: b8dfc45

### 16.18 — Use `prepare_cached()` for remaining static SQL in `database/stats/` and `documents.rs` (Complete - 2026-02-08)
- Replaced `prepare()` with `prepare_cached()` in `temporal.rs`, `sources.rs`, `compression.rs` (stats modules) and `documents.rs` `backfill_word_counts()`
- Follows established pattern from Phase 9 and task 16.16
- 836 lib + 358 binary = 1194 tests passing

**Key patterns:**
- `::new()` constructors for structs with repeated default fields (ReviewQuestion)
- `as_str() -> &'static str` methods to replace standalone conversion functions
- Static `resolve()` methods for repeated flag-handling patterns (OutputFormat)
- Wrapper functions that discard unused return values (`setup_database_only`)
- `require_*()` methods that consolidate get+ok_or patterns (require_document, require_repository)
- `#[serde(rename)]` + `to_value()` to replace manual JSON building
- `to_summary_json()` methods on data structs for standard JSON representations used across multiple tools
- Eliminate redundant anyhow wrappers when thiserror provides auto-conversion
- Extract shared data-fetching helpers when multiple detect modules fetch the same data the same way
- Move general-purpose utilities (embedding fetch, vector math, centroid computation) to parent module for cross-module reuse

**Test counts at task 16.18:** 836 lib + 358 binary = 1194 (with all features). Zero clippy warnings.

### Task 16.19 — Extract shared document content query helper in database/stats/ (2026-02-08)

**Summary:** Added `fetch_active_doc_content()` helper function and `CONTENT_ONLY_QUERY` constant in `database/stats/mod.rs`. The helper consolidates the repeated query + row iteration + decode + metadata computation pattern from `temporal.rs` and `sources.rs`. The constant consolidates the content-only query from `compression.rs` and `detailed.rs`.

**Files modified:**
- `src/database/stats/mod.rs` — Added `DocContent` struct, `fetch_active_doc_content()` helper, and `CONTENT_ONLY_QUERY` constant (+32 lines)
- `src/database/stats/temporal.rs` — Replaced 12-line query+iteration block with 2-line helper call (-6 lines net)
- `src/database/stats/sources.rs` — Replaced 12-line query+iteration block with 2-line helper call (-8 lines net)
- `src/database/stats/compression.rs` — Replaced inline query string with `super::CONTENT_ONLY_QUERY` (-2 lines net)
- `src/database/stats/detailed.rs` — Replaced inline query string in word count fallback with `super::CONTENT_ONLY_QUERY` (-2 lines net)

**Key considerations:**
- `DocContent` struct has only `decoded` and `metadata` fields — `id` was initially included but removed since neither consumer uses it (caught by clippy)
- `sources.rs` needs `doc.decoded` for `count_facts_with_sources()`, so the helper returns decoded content alongside metadata
- The helper uses `prepare_cached()` consistent with task 16.18's changes
- `detailed.rs` `_since` variant has an additional `AND file_modified_at >= ?2` clause, so it can't use `CONTENT_ONLY_QUERY` — left as-is
- Net change: +8 lines (48 added, 40 removed)

**Difficulties:** Initial implementation included an `id` field in `DocContent` that triggered a clippy `field never read` warning — removed in the same pass.

**Test results:** 836 lib, 358 binary — all 1194 tests passing (with all features). Zero clippy warnings. Commit: 546fbbd.

### Task 16.20 — Use `prepare_cached()` for 2 queries in `database/search/semantic.rs` (2026-02-08)

**Summary:** Replaced `prepare()` with `prepare_cached()` for 2 static SQL queries in `database/search/semantic.rs`. Completes the `prepare_cached()` migration across all database modules.

**Test results:** 836 lib, 358 binary — all 1194 tests passing (with all features). Zero clippy warnings.

**Phase 16 complete (20/20).** Test counts: 836 lib + 358 binary = 1194 (with all features). Zero clippy warnings.

---

## Phase 16 Cleanup Tasks (from audit)

### Task 16.21 — Use `prepare_cached()` for remaining `prepare()` in `database/documents.rs` and `database/search/` (Complete)

**Summary:** Replaced `prepare()` with `prepare_cached()` for static SQL queries in `documents.rs` (`get_documents_for_repo()`) and search modules (`search_by_title()`, `search_content()`). The `list_documents()` query in `documents.rs` uses dynamic SQL with limited filter combinations but was also converted.

**Note:** 2 remaining `prepare()` calls in `database/links.rs` (`get_links_for_documents()`) are dynamic SQL with variable-length IN clauses — `prepare_cached()` does not apply to these.

**Test results:** 836 lib + 358 binary = 1194 tests passing. Zero clippy warnings.

### Task 16.22 — Fix `row.get().unwrap_or()` in `database/search/semantic.rs` to use `?` (Complete - 2026-02-08)

**Summary:** Replaced 4 `unwrap_or(default)` calls with `?` in `row_to_search_result_with_chunk()` (lines 246-249). Columns come from INNER JOINs so values are always present — defaults were never triggered.

**Fields fixed:** distance, chunk_index, chunk_start, chunk_end

**Test results:** All 12 semantic search tests + 101 database tests pass. Zero clippy warnings. Commit: 7c4e8f3.

### Task 16.23 — Add `use` imports for inline `std::collections::` paths in `database/search/semantic.rs` (Complete - 2026-02-08)

**Summary:** Replaced 5 inline `std::collections::HashMap`, `HashSet`, `hash_map::DefaultHasher` usages in production code with `use` imports at file top. Removed closure-level `use` for Hash/Hasher. Pure style cleanup, no behavior change.

**Test results:** 12 semantic search tests pass. Zero clippy warnings. Commit: 70f4ed5.

**Phase 16 fully complete (20 subtasks + 3 cleanup tasks = 23 total).** Test counts: 836 lib + 358 binary = 1194 (with all features). Zero clippy warnings.

---

## Phase 17: Search Module Cleanup (3/3 Complete - 2026-02-08)

**Goal:** Reduce duplication and improve performance in the search module.

### 17.1 — Extract shared SQL filter builder for `doc_type`/`repo_id` in search modules (Complete)
- Extracted `append_type_repo_filters(&mut sql, &mut param_idx, doc_type, repo_id)` and `push_type_repo_params()` into `database/search/mod.rs`
- Replaced ~30 lines of repeated logic across 4 call sites in title.rs, semantic.rs (×2), and content.rs

### 17.2 — Reduce `search_semantic` API surface from 3 to 2 entry points (Complete)
- Removed `search_semantic()` thin wrapper, updated its single caller to use `search_semantic_with_query()` directly
- Kept `search_semantic_with_query` (convenience) and `search_semantic_paginated` (full)

### 17.3 — Optimize `highlight_terms` to avoid repeated full-string lowercasing and allocation (Complete)
- Refactored to lowercase once, find all term positions, then build highlighted string in a single pass
- Preserves original casing in output

**Key learnings:**
- SQL filter builder: `append_type_repo_filters()` + `push_type_repo_params()` for dynamic WHERE clause building with indexed params across search modules
- Remove thin API wrappers that just call through with defaults — reduces API surface and maintenance burden
- `highlight_terms` single-pass optimization: lowercase once, collect positions, build output in one pass

**Test counts:** 836 lib + 358 binary = 1194 (with all features). Zero clippy warnings.

---

## Phase 18: Organize Module Test Helper Deduplication & Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Reduce test code duplication in the organize module and consolidate repeated command setup patterns. All tasks are pure refactoring with no behavior changes.

### 18.1 — Extract shared test helpers for organize module (Complete)
- Created `organize/test_helpers.rs` with two `#[cfg(test)]` gated shared helpers: `insert_test_doc()` (creates + upserts to DB) and `make_test_doc()` (returns Document without DB)
- Replaced 6 duplicate `test_doc` functions across `snapshot.rs`, `links.rs`, `execute/merge.rs`, `execute/split.rs`, `execute/move.rs`, `execute/retype.rs`
- Used `use ... as test_doc` aliasing so call sites needed zero changes beyond the import line
- Net -39 lines. Commit: e1f78d0

### 18.2 — Extract `parse_since_filter` helper (Complete)
- Added `parse_since_filter(since: &Option<String>) -> Result<Option<DateTime<Utc>>>` to `commands/utils.rs`
- Replaced repeated pattern at 5 call sites: `grep/execute.rs`, `review/status.rs`, `review/apply.rs`, `status/mod.rs`, `lint/mod.rs`
- Binary crate's `grep/execute.rs` imports via `super::` through `grep/mod.rs` — update intermediate module imports when adding shared helpers
- Net +3 lines. Commit: c4d1cda

### 18.3 — Consolidate `setup_db_and_resolve_repos` (Complete)
- Added `setup_db_and_resolve_repos(repo_filter: Option<&str>) -> Result<(Database, Vec<Repository>)>` to `commands/utils.rs`
- Only 2 of 4 listed call sites used the exact pattern (`review/status.rs`, `review/apply.rs`); `lint/mod.rs` needs Config, `organize/apply.rs` uses different setup
- Removed `resolve_repos` from `commands/mod.rs` re-exports (now internal only)
- Net +11 lines. Commit: de8733c

**Key Learnings:**
- `#[cfg(test)]` gated shared helper modules (e.g., `organize/test_helpers.rs`) for cross-module test dedup
- `use ... as alias` pattern to avoid changing call sites when consolidating helpers
- DB-inserting vs non-DB test helpers: separate functions for different signatures (`insert_test_doc` vs `make_test_doc`)
- Binary crate `grep/execute.rs` imports via `super::` through `grep/mod.rs` — update intermediate module imports when adding shared helpers
- When consolidating setup patterns, verify each call site uses the exact same sequence before replacing — slight variations (needing Config, different error handling) prevent consolidation

**Test counts:** 841 lib + 358 binary = 1199 (with all features). Zero clippy warnings.

---

## Phase 19: ScanOptions Safety & Inline Repo Filtering Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Fix a panic-causing bug in `ScanOptions::default()` and consolidate inline repo filtering and ScanOptions construction patterns.

### 19.1 — Fix `ScanOptions` Default to use safe non-zero values (Complete)
- Replaced `#[derive(Default)]` with manual `impl Default for ScanOptions` providing safe non-zero values (chunk_size: 100_000, chunk_overlap: 2_000, embedding_batch_size: 10, min_coverage: 0.8)
- Removed redundant `with_defaults()` method — `Default` is now equivalent
- Prevents `chunks(0)` panic when `Default::default()` used instead of `with_defaults()`

### 19.2 — Replace inline repo filtering with `resolve_repos` (Complete)
- Replaced duplicated inline repo filtering in `lint/mod.rs`, `scan/mod.rs`, and `links.rs` with shared `resolve_repos()` utility
- Added `resolve_repos` to `commands/mod.rs` re-exports
- Net -32 lines across 4 files. Commit: ceef279

### 19.3 — Consolidate ScanOptions construction from Config (Complete)
- Added `ScanOptions::from_config(&Config)` constructor for the 4 config-derived fields
- Replaced all 3 call sites in `serve.rs` (1) and `scan/mod.rs` (2) with `from_config` + struct update syntax
- Net -13 lines of production code. Commit: 88c6c8f

**Key Learnings:**
- `ScanOptions::default()` must provide safe non-zero values — zero values cause `chunks(0)` panic
- Prefer manual `impl Default` over `#[derive(Default)]` when zero is not a safe default for numeric fields
- `from_config(&Config)` constructors consolidate repeated field mapping from config structs
- Struct update syntax (`..ScanOptions::from_config(&config)`) cleanly overrides command-specific flags

**Test counts:** 841 lib + 358 binary = 1199 (with all features). Zero clippy warnings.

---

## Phase 20: Inline Path Cleanup & Repo Filtering Consolidation (3/3 Complete - 2026-02-09)

**Goal:** Clean up inline `std::` paths that violate the project's coding convention, and consolidate the last remaining inline repo filtering pattern.

### 20.1 — Replace inline `std::` paths with `use` imports across production code (Complete)
- Replaced ~130 inline `std::` paths across 52 source files with proper `use` imports at file top
- Added imports for `HashMap`, `HashSet`, `Path`, `PathBuf`, `fs`, `io`, `fmt`, `mem`, `OsStr`, `Ordering`, `DefaultHasher`, `IsTerminal` as needed per file
- Left idiomatic single-use paths as-is: `std::io::stderr` in tracing setup, `std::io::Error` in `#[from]` derives
- AST rewrite tools (`pattern_rewrite`) sometimes broke `use` import lines — required manual verification
- Net +111 lines (mostly `use` import additions). Commit: 37216c7

### 20.2 — Consolidate organize/apply.rs inline repo filtering to use resolve_repos (Complete)
- Replaced 11-line inline repo filtering pattern with `resolve_repos(db.list_repositories()?, args.repo.as_deref())?`
- Also replaced `find_repo_with_config` with `setup_database` since the returned repo was unused
- Net -15 lines. Commit: 2ae001c

### 20.3 — Add missing `use` imports for `HashMap` in test code (Complete)
- Added `use std::collections::HashMap;` to test module in `question_generator/fields.rs`
- Replaced 10 inline `std::collections::HashMap` paths with short `HashMap` form
- `super::*` glob re-export only covers items defined in the parent module, not the parent's own `use` imports
- Net +2 lines. Commit: a08795b

**Key Learnings:**
- AST rewrite tools can break `use` statements — always verify imports after automated rewrites
- Leave idiomatic single-use inline paths as-is (tracing setup, thiserror derives)
- `super::*` in test modules doesn't bring in parent's `use` imports — explicit imports needed
- Test module `use` imports are separate from parent module scope

**Test counts:** 841 lib + 358 binary = 1199 (with all features). Zero clippy warnings.

---

## Phase 21: Code Deduplication — require_document, resolve_repos, inline chrono (3/3 Complete - 2026-02-09)

**Goal:** Consolidate remaining `get_document + ok_or_else` patterns, eliminate duplicate repo resolution logic, and clean up inline `chrono::` paths.

### 21.1 — Migrate `get_document + ok_or_else` to `require_document` (Complete)
- Replaced 5 instances across 4 files: `commands/organize/{move,merge,split}.rs` and `mcp/tools/review/answer.rs`
- `require_document` returns `Result<Document, FactbaseError>` which auto-converts to `anyhow::Error` via `?`
- Net -14 lines. Commit: 366f356

### 21.2 — Consolidate `watch_helper::get_repos` and `serve.rs` to use `resolve_repos` (Complete)
- Replaced `WatchContext::get_repos()` in `grep/execute.rs` and `search/watch.rs` with `resolve_repos()`
- Replaced inline empty-check in `serve.rs` with `resolve_repos()`
- Removed `get_repos` method and its 4 tests (covered by `resolve_repos` tests)
- Net -76 lines. Commit: ebfc401

### 21.3 — Replace inline `chrono::` paths with `use` imports (Complete)
- Replaced 28 inline `chrono::` paths across 15 production files
- Task estimated ~60 but remaining were in test modules (left as-is per convention)
- Two struct-field-only files skipped per single-use rule
- Net +10 lines. Commit: 575bb46

**Test counts:** 841 lib + 354 binary = 1195 (with all features). Zero clippy warnings.

---

## Phase 22: Command Pattern Deduplication (3/3 Complete - 2026-02-09)

**Goal:** Extract repeated patterns in CLI command implementations into shared helpers, reducing boilerplate and improving consistency.

### 22.1 — Extract `confirm_prompt()` helper to `commands/utils.rs` (Complete)
- Created `confirm_prompt(message: &str) -> anyhow::Result<bool>` in `commands/utils.rs`
- Replaced 4 identical y/N confirmation prompt patterns across `organize/merge.rs`, `organize/split.rs`, `organize/move.rs`, and `scan/prune.rs`
- Per-item prompts inside loops (scan/verify.rs, lint/execute/links.rs) left as-is — different pattern
- Net -5 lines. Commit: 5f394bf

### 22.2 — Extract `execute_with_snapshot()` helper for organize snapshot/rollback pattern (Complete)
- Created generic helper in `commands/organize/mod.rs` encapsulating snapshot→execute→verify→rollback/cleanup
- Takes `execute_fn` and `verify_fn` closures plus `operation_name` for error messages
- Replaced ~20 lines of duplicated logic in both merge.rs and split.rs
- Simplified signature: removed `Snapshot` param from `execute_fn` since neither consumer needs it
- Net +3 lines. Commit: 8d2499c

### 22.3 — Consolidate `base64::Engine` inline imports to module-level `use` (Complete)
- Moved `use base64::Engine;` from 4 inline function-body imports to module-level in `database/mod.rs`, `database/documents.rs`, `database/stats/compression.rs`
- Follows same convention as Phase 20 (std:: paths) and Phase 21 (chrono:: paths)
- Net -1 lines

**Key Learnings:**
- `confirm_prompt(message)` helper in `commands/utils.rs` replaces 4 duplicated y/N prompt patterns
- `execute_with_snapshot()` generic helper encapsulates snapshot/rollback pattern — takes closures for execute and verify
- Module-level `use` for third-party crate traits (base64::Engine) follows same convention as std:: and chrono:: cleanup

**Test counts:** 841 lib + 354 binary = 1195 (with all features). Zero clippy warnings.

---

## Phase 23: Web API & MCP Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Move misplaced shared types to proper locations, add consistency test to prevent MCP schema/dispatch drift, and consolidate repeated filesystem operation patterns.

### 23.1 — Extract `ApiError` and `handle_error` to `web/api/errors.rs` (Complete)
- Created `src/web/api/errors.rs` containing `ApiError` struct, its impl block, and `handle_error()` function
- Updated imports in organize.rs, stats.rs, documents.rs from `super::review::ApiError` to `super::errors::ApiError`
- Moved 6 handle_error/ApiError tests from review.rs to errors.rs
- review.rs reduced from 383 to 261 lines; new errors.rs is 131 lines
- 841 lib + 354 bin tests pass. Commit: 5ac4e45

### 23.2 — Add MCP schema/dispatch consistency test (Complete)
- Added `test_schema_dispatch_consistency` in `src/mcp/tools/mod.rs`
- Extracts tool names from `tools_list()["tools"]` schema and compares against dispatch match arm names
- Reports mismatches in both directions (schema-only and dispatch-only)
- 841 lib + 354 bin tests pass. Commit: 8bcb253

### 23.3 — Consolidate repeated `fs::write` + `map_err` pattern in organize module (Complete)
- Created `src/organize/fs_helpers.rs` with `write_file()` and `remove_file()` helpers
- Replaced 8 call sites across 4 files: execute/merge.rs (2), execute/split.rs (2), links.rs (2), review.rs (2)
- Error messages standardized to "Failed to write/remove {path}: {error}"
- Net -16 lines. 842 lib + 354 bin tests pass. Commit: 7c6a5de

**Key Learnings:**
- Schema/dispatch consistency tests prevent drift between tool definitions and handlers — define expected names as a literal set in the test
- `fs::write`/`fs::remove_file` helpers with descriptive errors reduce boilerplate in file-heavy modules
- When extracting shared types from a module, re-export from parent for clean imports
- `super::*` doesn't re-export `std::fs` — test modules need explicit `use std::fs` after extraction

**Test counts:** 842 lib + 354 binary = 1196 (with all features). Zero clippy warnings.

---

## Phase 24: Pattern Consolidation & Deduplication (3/3 Complete - 2026-02-09)

**Goal:** Consolidate remaining scattered patterns into shared locations: regex patterns into `patterns.rs`, file-read helpers into `fs_helpers.rs`, and word-count computation into a shared helper.

### 24.1 — Move organize/review.rs static regex patterns to patterns.rs (Complete)
- Moved `ORPHAN_ENTRY_REGEX` and `SIMPLE_ORPHAN_REGEX` from `organize/review.rs` to `patterns.rs` under a new "Orphan review patterns" section
- Updated imports in `organize/review.rs` to use `crate::patterns::{ORPHAN_ENTRY_REGEX, SIMPLE_ORPHAN_REGEX}`
- Removed now-unused `use regex::Regex` and `use std::sync::LazyLock` from review.rs
- 842 lib + 354 bin tests pass, zero clippy warnings

### 24.2 — Add `read_file` helper to `organize/fs_helpers.rs` (Complete)
- Added `pub(crate) fn read_file(path: &Path) -> Result<String, FactbaseError>` completing the read/write/remove trio
- Replaced 6 identical `fs::read_to_string + map_err` patterns across `organize/links.rs` (2), `organize/orphans.rs` (1), and `organize/review.rs` (3)
- Left `has_pending_orphans()` `if let Ok(...)` pattern as-is (different error-handling semantics)
- Net -21 lines. 842 lib + 354 bin tests pass, zero clippy warnings

### 24.3 — Extract `word_count` helper to reduce repeated `split_whitespace().count()` (Complete)
- Added `pub fn word_count(text: &str) -> usize` free function to `src/models/document.rs`, re-exported via `models/mod.rs`
- Replaced 5 call sites across `database/documents.rs` (2), `database/stats/detailed.rs` (2), and `mcp/tools/entity.rs` (1)
- Left `processor/sources.rs` as-is (different semantics — `<= 3` check for type validation)
- Chose free function over Document method since call sites operate on raw strings, not Document fields
- 843 lib + 354 bin = 1197 tests pass, zero clippy warnings

**Key Learnings:**
- Centralize all static `LazyLock<Regex>` patterns in `src/patterns.rs` for discoverability
- `read_file()` / `write_file()` / `remove_file()` trio in `organize/fs_helpers.rs` for consistent filesystem error messages
- Prefer free functions over struct methods when call sites operate on raw data, not struct instances
- `str_replace` with too-aggressive context can collapse code blocks — use precise surrounding context

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 25: FS Helper & Orphan Dedup (3/3 Complete - 2026-02-09)

**Goal:** Consolidate remaining raw `fs::read_to_string`/`fs::write` + `map_err` patterns to use `read_file`/`write_file` helpers from `organize/fs_helpers.rs`, and deduplicate repeated orphan file loading into a shared helper.

### 25.1 — Migrate remaining fs calls to fs_helpers (Complete)
- Replaced 7 raw `fs::read_to_string`/`fs::write` call sites across 4 files with `read_file`/`write_file` from `organize/fs_helpers.rs`
- Files: `web/api/organize.rs` (3), `organize/orphans.rs` (1), `organize/verify.rs` (1), `organize/execute/retype.rs` (2)
- Removed unused `use std::fs`/`use std::io` from parent modules; added `use std::fs` to test modules that still need it
- Net -19 lines. 843 lib + 354 bin tests pass, zero clippy warnings.

### 25.2 — Extract `load_orphan_entries` helper (Complete)
- Added `load_orphan_entries(repo_path) -> Result<Vec<OrphanEntry>>` to `organize/review.rs`
- Encapsulates `orphan_file_path + exists check + read_file + parse_orphan_entries`
- Simplified `has_orphans()` (uses `is_ok_and()`), `count_orphans()`, and 3 external call sites
- Re-exported from `organize/mod.rs`
- 843 lib + 354 bin tests pass, zero clippy warnings.

### 25.3 — Consolidate `count_orphans_in_file` with `parse_orphan_entries` (Complete)
- Removed ad-hoc `count_orphans_in_file()` from verify.rs (used string matching)
- Replaced with `parse_orphan_entries(&read_file(path)?).len()` using proper regex-based parser
- Net -16 lines. 843 lib + 354 bin tests pass, zero clippy warnings. Commit: b345f2a.

**Key Learnings:**
- `load_orphan_entries(repo_path)` consolidates the repeated orphan file loading pattern (exists check + read + parse)
- Replace ad-hoc string matching with proper regex-based parsers when available (`parse_orphan_entries` vs manual `starts_with`/`contains`)
- `super::*` glob re-export only covers items defined in the parent module — test modules need explicit `use std::fs` after removing it from parent

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 26: Snapshot FS Helpers & Validation Return Types (3/3 Complete - 2026-02-09)

**Goal:** Extend `organize/fs_helpers.rs` with `copy_file`, `remove_dir`, and `create_dir` helpers to eliminate verbose error wrapping in `snapshot.rs`, and change validation functions to return `anyhow::Result` to eliminate `.map_err` boilerplate at call sites.

### 26.1 — Add `copy_file` and `remove_dir` to fs_helpers, migrate snapshot.rs (Complete)
- Added `copy_file()` and `remove_dir()` to `organize/fs_helpers.rs`
- Replaced 5 verbose `FactbaseError::Io(io::Error::new(...))` blocks in `snapshot.rs` with helper calls
- `copy_file` discards `u64` bytes-copied return since callers never use it
- Net -7 lines. Commit: 144f577

### 26.2 — Change `validate_timeout` and `validate_repo_id` to return `anyhow::Result` (Complete)
- Changed both functions to use `anyhow::bail!` instead of `Err(format!(...))`/`Err(&'static str)`
- Simplified 5 call sites from `.map_err(...)` to just `?`
- `Config::validate()` extracts error message with `.to_string()` before wrapping in `FactbaseError::Config`
- Updated 4 tests to use `.unwrap_err().to_string()`

### 26.3 — Consolidate remaining `fs::create_dir_all` error wrapping in snapshot.rs (Complete)
- Added `create_dir()` helper to `organize/fs_helpers.rs`
- Replaced 2 bare `fs::create_dir_all` calls in `snapshot.rs` with the new helper
- `fs::read_dir` left as-is (used as iterator — different pattern)
- `commands/import/formats.rs` `fs::copy` left as-is (binary crate can't access `pub(crate)` lib helpers)
- Commit: 8b36837

**Key Learnings:**
- `copy_file()`, `remove_dir()`, `create_dir()` complete the fs_helpers.rs helper set: read/write/remove/copy/remove_dir/create_dir
- Validation functions should return `anyhow::Result<()>` with `anyhow::bail!` — avoids `.map_err(anyhow::Error::msg)?` boilerplate
- When a validation function is also called from a `Result<(), FactbaseError>` context, extract the error message with `.to_string()` before wrapping
- `pub(crate)` lib helpers can't be used from binary crate — single-use patterns in binary crate don't warrant separate helpers

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 27: Stats Deduplication & Error Propagation (3/3 Complete - 2026-02-09)

**Goal:** Consolidate duplicated `_since` function variants in stats modules into single functions with `Option<&DateTime<Utc>>` parameter, and fix error-swallowing `filter_map(|r| r.ok())` patterns in database code.

### 27.1 — Consolidate stats functions with Option<since> parameter (Complete)
- Merged `compute_stats`/`compute_stats_since` and `compute_detailed_stats`/`compute_detailed_stats_since` into single functions with `Option<&DateTime<Utc>>` parameter
- Extracted `compute_word_stats()` and `query_boundary_doc()` inner helpers to reduce duplication within consolidated functions
- Unified public API: `get_stats`/`get_stats_since` → single `get_stats(since: Option<&DateTime<Utc>>)`, making Task 3 unnecessary
- Cache bypass when `since` is `Some` (filtered stats shouldn't be cached)
- Dynamic SQL with conditional `since_clause` and `Vec<&dyn ToSql>` params
- 116 lines removed. Commit: 0489ef2

### 27.2 — Replace `filter_map(|r| r.ok())` with `.collect::<Result<Vec<_>, _>>()?` (Complete)
- Fixed 5 instances in `basic.rs`, `embeddings.rs`, `repositories.rs` where database errors were silently swallowed
- Also fixed Task 1 leftover in integration tests
- Commit: 4db1708

### 27.3 — Remove `_since` wrapper functions (Complete)
- Completed as part of Task 1 (unified public API eliminated the need for separate wrappers)

**Key Learnings:**
- Consolidate `_since` function variants into single functions with `Option<&DateTime<Utc>>` parameter — dynamic SQL with conditional `since_clause` and `Vec<&dyn ToSql>` params
- Cache bypass when `since` is `Some` (filtered stats shouldn't be cached)
- Extract inner helpers (`compute_word_stats`, `query_boundary_doc`) to reduce duplication within consolidated functions

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 28: Dependency & Code Optimization (3/3 Complete - 2026-02-09)

**Goal:** Reduce unnecessary dependency weight and DRY up repeated error-wrapping patterns.

### 28.1 — Replace `tokio = { features = ["full"] }` with minimal feature set (Complete)
- Replaced `tokio = { version = "1", features = ["full"] }` with 6 specific features: `rt-multi-thread`, `macros`, `net`, `signal`, `sync`, `time`
- Removed unused features: `fs`, `io-util`, `io-std`, `process`, `parking_lot`
- Verified all tokio usage via grep to determine minimal set
- Commit: b5acd42

### 28.2 — Extract shared `io_err()` helper in `organize/fs_helpers.rs` (Complete)
- Extracted `fn io_err(e: io::Error, action: &str, path: &Path) -> FactbaseError` private helper
- Replaced 5 of 6 repeated `FactbaseError::Io(io::Error::new(...))` patterns with one-liner `.map_err(|e| io_err(e, "action", path))`
- `copy_file` retains inline error construction (references two paths — doesn't fit single-path signature)
- Net -22 lines. Commit: 33f832c

### 28.3 — Replace `rand` with `getrandom` for document ID generation (Complete)
- Replaced `rand = "0.8"` with `getrandom = "0.2"` in Cargo.toml
- Changed `processor/core.rs` `generate_id()` from `rand::thread_rng().gen()` to `getrandom::getrandom(&mut buf)`
- Added `random_port()` helper in `tests/common/mod.rs` to replace 15 instances of `rand::random::<u16>()` across 7 integration test files
- `rand 0.8` remains as transitive dependency of `governor` — but no longer a direct dependency
- Commit: 16b951e

**Key Learnings:**
- Replace `tokio = { features = ["full"] }` with only the features actually used — grep for all `tokio::` usage to determine the minimal set
- `getrandom` crate can replace `rand` when only a few random bytes are needed (it's already a transitive dependency)
- Shared error-wrapping helpers (e.g., `io_err()`) reduce boilerplate in filesystem helper modules
- Integration tests may also use dependencies you're trying to remove — check test files too

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 29: Database Module Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Clean up remaining database module inconsistencies — type aliases, dead code, and repeated constant paths.

### 29.1 — Add `DbConn` type alias (Complete)
- Added `pub(crate) type DbConn = r2d2::PooledConnection<SqliteConnectionManager>` in `database/mod.rs`
- Replaced 7 inline `PooledConnection<SqliteConnectionManager>` references across database modules
- ast-grep couldn't match generic type syntax — used `sed` instead

### 29.2 — Remove `soft_delete_document` alias, extract `repo_id_for_doc` helper (Complete)
- Removed `soft_delete_document()` alias, changed caller in `scan/prune.rs` to `mark_deleted()`
- Extracted `repo_id_for_doc(conn, id) -> Option<String>` free function in `database/documents.rs`
- Deduplicates repeated `SELECT repo_id` + cache invalidation across 3 methods

### 29.3 — Consolidate `base64::engine::general_purpose::STANDARD` into a constant (Complete)
- Defined `pub(crate) const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;` in `database/mod.rs`
- Replaced all 4 inline `base64::engine::general_purpose::STANDARD` references: 2 in `mod.rs`, 1 in `documents.rs`, 1 in `stats/compression.rs`
- Kept `use base64::Engine;` in all 3 files — trait import required for `.encode()`/`.decode()` method dispatch
- All 1197 tests pass, zero clippy warnings

**Key Learnings:**
- `pub(crate) type DbConn` type alias replaces verbose inline generic types across database modules
- ast-grep `pattern_rewrite` can't match Rust generic type syntax like `PooledConnection<SqliteConnectionManager>` — use `sed` for such replacements
- Free functions (not methods) for helpers that only need `&DbConn`, not `&self` (e.g., `repo_id_for_doc`)
- `use base64::Engine;` trait imports cannot be removed from submodules — the `Engine` trait must be in scope for `.encode()` and `.decode()` method resolution
- `const` works for `GeneralPurpose` because it implements `const` construction

**Test counts:** 843 lib + 354 binary = 1197 (with all features). Zero clippy warnings.

---

## Phase 30: Bedrock & Error Handling Cleanup (6/6 Complete - 2026-02-09)

**Goal:** Reduce verbose error construction boilerplate across the codebase by extracting shared Bedrock SDK helpers, leveraging thiserror `#[from]`, and adding constructor helpers for high-frequency `FactbaseError` variants.

### 30.1 — Shared Bedrock SDK client builder + error helper closures (Complete)
- Extracted `build_client(region)` async helper to eliminate duplicate AWS SDK config loading in `BedrockEmbedding::new()` and `BedrockLlm::new()`
- Added `embed_err()` and `llm_err()` helper functions that reduced 6 verbose `map_err` calls to concise one-liners
- Used `impl Display` parameter on error helpers for maximum flexibility

### 30.2 — `#[from] notify::Error` on Watcher variant (Complete)
- Changed `FactbaseError::Watcher` from `Watcher(String)` to `Watcher(#[from] notify::Error)`
- Replaced 3 identical `.map_err(|e| FactbaseError::Watcher(e.to_string()))?` calls with plain `?`

### 30.3 — `FactbaseError::parse()` and `::not_found()` constructor helpers (Complete)
- Added `FactbaseError::parse(impl Into<String>)` and `::not_found(impl Into<String>)` constructors
- Migrated all 26 construction sites across mcp/tools/, organize/, web/api/, and error.rs
- Left match arm destructuring unchanged (constructors only replace construction sites)

### 30.4 — `FactbaseError::internal()` and `::config()` constructor helpers (Complete)
- Added `FactbaseError::internal(impl Into<String>)` and `::config(impl Into<String>)` constructors
- Migrated 22 Internal + 25 Config construction sites
- Multi-line patterns in config/mod.rs required perl multi-line regex

### 30.5 — Declarative config validation pattern (Complete)
- Extracted `require_non_empty`, `require_positive`, `require_range` helpers into `config/validation.rs`
- Migrated 15 of 19 validation checks; `validate()` reduced from 81 to 43 lines (-47%)
- Used `u64` for unsigned ints, generic `T: PartialOrd + Display` for range checks

### 30.6 — `FactbaseError::embedding()`, `::llm()`, `::ollama()` constructors (Complete)
- Added 3 more constructor helpers, migrated all 15 construction sites
- Updated `embed_err()`/`llm_err()` helpers in bedrock.rs to use new constructors internally

**Key Learnings:**
- Extract shared SDK client builders to eliminate duplicate AWS config loading
- Error helper closures with `impl Display` parameter reduce verbose `map_err` to one-liners
- Use thiserror `#[from]` attribute to auto-generate `From<T>` impls — eliminates manual `.map_err(|e| Variant(e.to_string()))?`
- Constructor helpers with `impl Into<String>` accept both `format!(...)` and string literals
- Leave match arm destructuring unchanged — constructors only replace construction sites
- Declarative validation helpers (`require_non_empty`, `require_positive`, `require_range`) reduce repetitive validation boilerplate
- Multi-line error construction patterns need perl `-0777` multi-line regex, not single-line sed

**Test counts at Phase 30 completion:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 31: Code Quality & API Surface Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Remove remaining `unwrap()` calls from production code, reduce organize module public API surface, and consolidate duplicate test helpers.

### 31.1 — Remove remaining `unwrap()` calls from production code (Complete)
- Replaced all 15 `unwrap()` calls in non-test production code with `expect("reason")` or pattern matching
- Categories: `partial_cmp().unwrap()` → `expect("non-NaN")`, `write!` to String → `expect("write to String")`, `Response::builder().unwrap()` → `expect("valid response")`, `is_some() + unwrap()` → `matches!` pattern
- 867 lib + 354 binary tests pass. Commit: 1767fa8

### 31.2 — Reduce organize module public API surface (Complete)
- Removed `list_snapshots`, `cleanup_old_snapshots`, `snapshot_dir` from `organize/mod.rs` public re-exports
- Demoted to private in `snapshot.rs` with `#[allow(dead_code)]` (operational utilities not yet wired to CLI)
- 867 lib + 354 binary tests pass. Commit: fb700f0

### 31.3 — Consolidate duplicate `make_result` test helpers in `commands/search/` (Complete)
- Created `commands/search/test_helpers.rs` with shared `make_result()` function
- Standardized parameter order `(id, title, doc_type, score)` across 8 call sites
- 867 lib + 347 binary tests pass. Commit: b6ef0fb

**Key Learnings:**
- `matches!(val, Some(x) if condition)` is more idiomatic than `val.is_some() && val.unwrap()...` anti-pattern
- Use `#[allow(dead_code)]` for operational utilities not yet wired to CLI — keeps them available vs `#[cfg(test)]` which limits to test builds
- Shared test helper modules (`test_helpers.rs`) with `#[cfg(test)] pub(crate) mod tests` pattern for cross-module test dedup

**Test counts at Phase 31 completion:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 32: Import Consistency & Steering Doc Updates (3/3 Complete - 2026-02-09)

**Goal:** Standardize binary crate imports to use `lib.rs` re-exports and trim unused re-exports to reduce public API surface.

### 32.1 — Update `current-state.md` to reflect Phase 31 completion (Complete)
- Already completed in prior commits (6963f30, 3c83d1d)
- current-state.md already reflected Phase 31 completion and Phase 32 as active

### 32.2 — Standardize binary crate imports to use `lib.rs` re-exports (Complete)
- Added `Link` to `lib.rs` re-exports (was missing despite being used by binary crate)
- Replaced deep module paths in 10 files with flat `factbase::` imports
- Consolidated multi-line `use factbase::X; use factbase::Y;` into single `use factbase::{X, Y};`
- Zero clippy warnings, all 1221 tests pass

### 32.3 — Audit and trim `lib.rs` re-exports unused by binary crate (Complete)
- Removed 38 unused re-exports from `lib.rs`, reducing public API surface significantly
- Entire re-export blocks removed: `async_helpers::run_blocking`, all 7 `cache::` symbols, all 4 `organize::` symbols
- Largest removals by module: models (7), cache (7), processor (6), answer_processor (4), organize (4), output (3)
- Fixed 4 internal `crate::` references that relied on lib.rs re-exports to use direct module paths
- Kept `ScanResult` and `chunk_document` which are used by integration tests
- Commit: ea98687

**Key Learnings:**
- Binary crate should use `lib.rs` re-exports (`use factbase::{Database, Document}`) instead of deep module paths
- Consolidate multi-line `use factbase::X; use factbase::Y;` into single `use factbase::{X, Y};`
- If a type is used by the binary crate but not re-exported from `lib.rs`, add it to the re-exports
- Systematic grep-based audit of re-exports: check `src/main.rs`, `src/commands/`, and `tests/` for each symbol
- Internal `crate::` references should use direct module paths, not rely on lib.rs re-exports

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 33: Module Visibility & Steering Doc Updates (3/3 Complete - 2026-02-09)

**Goal:** Reduce public API surface by demoting internal-only modules and their items from `pub` to `pub(crate)`.

### 33.1 — Update `current-state.md` to reflect Phase 32 completion (Complete)
- Already completed in prior commits (ffddce3, 4de69f9)
- current-state.md already reflected Phase 32 completion and Phase 33 as active work
- No changes needed

### 33.2 — Demote 5 internal-only modules from `pub mod` to `pub(crate) mod` (Complete)
- Changed `pub mod` to `pub(crate) mod` for 5 modules in lib.rs: `async_helpers`, `cache`, `patterns`, `shutdown`, `ollama`
- Kept 3 `pub use` re-exports that are actively used by binary crate: `create_http_client`, `MANUAL_LINK_REGEX`, `init_shutdown_handler`
- Module demotion surfaced 8 dead_code warnings for items that were previously visible externally but unused
- Added `#[allow(dead_code)] // operational utility` to 12 items across 3 modules:
  - cache.rs: 5 methods (invalidate, clear, len, is_empty, capacity) + 2 functions (invalidate_document_cache, clear_document_cache)
  - shutdown.rs: 1 function (request_shutdown)
  - ollama.rs: 3 constants (DEFAULT_MAX_RETRIES, DEFAULT_RETRY_DELAY_MS, DEFAULT_TIMEOUT_SECS) + 3 methods (new, with_timeout, base_url)
- Zero clippy warnings, all 1221 tests pass (867 lib + 354 binary)

### 33.3 — Demote `pub` items to `pub(crate)` within internal-only modules (Complete)
- Demoted all `pub` items to `pub(crate)` within the 5 internal-only modules
- `async_helpers`: `run_blocking` → `pub(crate)`
- `cache`: All structs, functions, and methods → `pub(crate)`
- `patterns`: All 24 regex constants and helper functions → `pub(crate)`
- `shutdown`: `is_shutdown_requested`, `request_shutdown` → `pub(crate)`; kept `init_shutdown_handler` and `reset_shutdown_flag` as `pub` (re-exported/test usage)
- `ollama`: `OllamaClient` and methods → `pub(crate)`; kept `create_http_client` as `pub` (re-exported from lib.rs)
- Items still re-exported from lib.rs (`init_shutdown_handler`, `create_http_client`, `MANUAL_LINK_REGEX`) kept as `pub`
- Zero clippy warnings, all 1221 tests pass (867 lib + 354 binary)
- Commit: b9a73c3

**Key Learnings:**
- Demote `pub mod` to `pub(crate) mod` for modules only used within the lib crate
- Keep `pub use` re-exports for specific items still needed by binary crate even when the module itself is `pub(crate)`
- Module demotion surfaces `dead_code` warnings — add `#[allow(dead_code)] // operational utility` per project convention
- When demoting `pub` items to `pub(crate)` within internal modules, check that no re-exports in `lib.rs` reference them first

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 34: CI Hardening & Anyhow Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Consolidate redundant CI jobs for better coverage with fewer jobs, and clean up verbose `anyhow::anyhow!` patterns across command modules.

### 34.1 — Consolidate CI clippy jobs and add binary test coverage (Complete)
- Merged `clippy` + `clippy-web` into single `clippy` job using `--all-features --lib --tests`
- Added `cargo test --bin factbase` to `test` job (354 binary tests now validated in CI)
- Added `cargo test --bin factbase --features web` to `test-web` job
- Removed redundant `check` job (test already compiles) and `build` job (readme-validation does same build)
- Net: 2 fewer CI jobs (9→7), better coverage
- Commit: fca9eb5

### 34.2 — Replace `Err(anyhow::anyhow!(...))` with `bail!/context()` in commands/ (Complete)
- Replaced 9 verbose anyhow patterns across 4 files: doctor/mod.rs, doctor/fix.rs, review/import.rs, serve.rs
- `return Err(anyhow::anyhow!(...))` → `bail!(...)`
- `.map_err(|e| anyhow::anyhow!(...))` → `.with_context()`/`.context()`
- Net: -3 lines
- Commit: 65798d6

### 34.3 — Replace remaining `anyhow::anyhow!` patterns across commands/ (Complete)
- Replaced 3 patterns in organize/move.rs, links.rs, scan/verify.rs
- `errors.rs` intentionally kept — functions return `anyhow::Error` (not `Result`), so `bail!` doesn't apply
- Commit: 7e4e082

**Key Learnings:**
- `anyhow::bail!` replaces `return Err(anyhow::anyhow!(...))` — shorter and idiomatic
- `.context()`/`.with_context()` replaces `.map_err(|e| anyhow::anyhow!(...))` — preserves error chain
- Functions returning `anyhow::Error` (not `Result`) keep `anyhow::anyhow!()` — `bail!` only works in `Result`-returning functions
- CI consolidation: merge redundant jobs, ensure binary tests are validated alongside lib tests

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 35: Dead Code Cleanup & Documentation Accuracy (3/3 Complete - 2026-02-09)

**Goal:** Remove dead code behind `#[allow(dead_code)]` annotations, consolidate shared helpers, and fix stale documentation.

### 35.1 — Remove dead code behind `#[allow(dead_code)]` annotations (Complete)
- Removed 16 of 17 `#[allow(dead_code)]` annotations by gating test-only items with `#[cfg(test)]`
- Files: `cache.rs` (7 items), `ollama.rs` (6 items), `shutdown.rs` (1 item), `organize/snapshot.rs` (2 items)
- Approach: `#[cfg(test)]` gating instead of deletion — preserves test coverage
- 1 remaining `#[allow(dead_code)]` is a serde struct field in `plan/merge.rs` — intentional (deserialized but not read)
- `#[cfg(test)]` on methods works across modules since all test code compiles together

### 35.2 — Consolidate `truncate_at_word_boundary` into shared `output.rs` helper (Complete)
- Moved `truncate_at_word_boundary` from `mcp/tools/entity.rs` to `output.rs` as `pub(crate)` shared helper
- Updated `generate_preview` to delegate to the shared function
- Net: +43 lines in output.rs, -59 lines in entity.rs

### 35.3 — Update stale test counts in README badge and documentation (Complete)
- Updated README badge from `1197_passing` to `1221_passing`
- Verified: 867 lib + 354 bin = 1221

**Key Learnings:**
- When removing `#[allow(dead_code)]`, prefer `#[cfg(test)]` gating over deletion when items have test coverage
- `#[cfg(test)]` on methods works across modules since all test code compiles together
- Serde struct fields that are deserialized but not read in Rust keep `#[allow(dead_code)]` — intentional

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 36: Dependency Modernization & Code Simplification (3/3 Complete - 2026-02-09)

**Goal:** Remove unnecessary proc macro dependency, optimize binary size, and clean up duplicate dependency entries.

### 36.1 — Replace `async-trait` crate with manual BoxFuture desugaring (Complete)
- Removed `async-trait` proc macro dependency entirely
- Replaced all `#[async_trait]` annotations with manual `BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>` type alias
- Native `async fn` in traits (Rust 1.75+) doesn't support `dyn` dispatch — manual desugaring required
- Explicit `<'a>` lifetime parameters needed when both `&self` and `&str` params exist (elided `'_` is ambiguous)
- clippy `type_complexity` warning on `Pin<Box<...>>` resolved with `BoxFuture` type alias
- 8 files changed. Commit: 6f42030

### 36.2 — Reduce binary size with release profile optimization (Complete)
- Changed `lto = "thin"` to `lto = "fat"` and added `opt-level = "z"` in release profile
- Binary reduced from 25MB to 16MB (36% reduction)
- `opt-level = "z"` optimizes for size over speed — acceptable for I/O-bound apps
- `opt-level = "z"` actually builds faster than `opt-level = "s"` (3.5min vs 4min) while producing smaller binary
- `panic = "abort"` rejected — prevents Drop impls from running on panic, could leave DB connections in bad state
- Measured sizes: no-features: 6.7MB, +progress: +0.1MB, +compression: +0.6MB, +mcp: +1MB, +bedrock: +7MB, full: 16MB, +web: +1MB

### 36.3 — Consolidate duplicate chrono dependency entries (Complete)
- Removed `chrono` from `[build-dependencies]` by replacing `chrono::Utc::now()` in `build.rs` with stdlib-only date calculation
- Used Howard Hinnant's civil calendar algorithm for days-since-epoch to civil date conversion
- Commit: 66b3a97

**Key Learnings:**
- Native `async fn` in traits (Rust 1.75+) doesn't support `dyn` dispatch — use manual BoxFuture desugaring
- Explicit `<'a>` lifetime parameters needed when both `&self` and `&str` params exist in trait methods
- `lto = "fat"` + `opt-level = "z"` is effective for I/O-bound apps (36% binary size reduction)
- `panic = "abort"` should be avoided when Drop impls matter (e.g., database connections)

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 37: Dependency Deduplication & Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Reduce duplicate transitive dependencies by upgrading direct dependencies to versions that align with the rest of the dependency tree.

### 37.1 — Upgrade `reqwest` from 0.11 to 0.12 (Complete - 2026-02-09)
- Upgraded reqwest 0.11→0.12 with `rustls-tls` feature (replacing default `native-tls`), and tower-http 0.5→0.6 as bonus dedup
- reqwest 0.12 was fully API-compatible — zero code changes needed in ollama.rs or doctor/checks.rs
- Switching to `rustls-tls` aligns with AWS SDK's rustls usage and eliminates native-tls/openssl dependency
- Remaining hyper 0.14/http 0.2/rustls 0.21 duplicates come from `aws-smithy-http-client` (AWS SDK internals) — cannot be eliminated
- Duplicate lines 474→441 (33 fewer). Eliminated crates: base64 v0.21, native-tls, hyper-tls, tokio-native-tls, openssl/openssl-sys, tower-http v0.5
- Cargo.lock: 215 deletions, 143 insertions (net -72 lines)
- All 1221 tests pass, zero clippy warnings. Commit: a5364b8

### 37.2 — Upgrade `dirs` from v5 to v6 (Complete - 2026-02-09)
- Upgraded dirs v5→v6 in Cargo.toml — single line change, zero code changes needed
- API is identical between v5 and v6 (`dirs::config_dir()` unchanged). Only one usage site in `src/config/mod.rs`
- The v5/v6 duplication existed because `shellexpand v3` transitively pulls in dirs v6
- Duplicate lines 441→431 (10 fewer). Eliminated crates: dirs v5.0.1, dirs-sys v0.4.1
- Cargo.lock: -32 net lines. All 1221 tests pass, zero clippy warnings. Commit: d7925b7

### 37.3 — Remove `walkdir` crate, replace with stdlib recursive helper (Complete - 2026-02-09)
- Removed `walkdir` as a direct dependency, replaced all 3 usage sites (scanner, import command, test) with iterative `std::fs::read_dir` stack-based traversal
- All three usages were simple recursive walks with `.filter_map(|e| e.ok())` error skipping — no depth limits, symlink handling, or special walkdir features needed
- `build.rs` already had a stdlib-based recursive `walkdir()` function as precedent
- Import command required changing from iterator-based to collect-then-iterate since `entry.path()` references changed
- `walkdir` remains as transitive dependency via `notify` and `criterion` (dev-dep) — doesn't reduce Cargo.lock, but removes direct dependency
- 5 files changed, 63 insertions, 35 deletions. All 1221 tests pass, zero clippy warnings. Commit: 057fdda

**Key Learnings:**
- When upgrading `reqwest`, switch to `rustls-tls` feature to align with AWS SDK's rustls usage and eliminate native-tls/openssl dependency
- Some duplicate dependencies (e.g., hyper 0.14/http 0.2 from `aws-smithy-http-client`) are AWS SDK internals — cannot be eliminated from our side
- Check `cargo tree --duplicates` before and after to verify reduction
- Bonus dedup opportunities may appear (e.g., tower-http version alignment) — take them in the same pass
- Simple recursive file walks don't need `walkdir` crate — `std::fs::read_dir` with a stack-based loop is sufficient
- When replacing iterator-based walkdir with stdlib, collect paths first if you need owned `PathBuf` values

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 38: Dependency Elimination & Modernization (3/3 Complete - 2026-02-09)

**Goal:** Eliminate unnecessary direct dependencies by replacing them with functionality already available in the dependency tree (tokio, notify) or upgrading to latest major versions.

### 38.1 — Replace `ctrlc` crate with `tokio::signal` (Complete - 2026-02-09)
- Replaced `ctrlc::set_handler` with a spawned tokio task awaiting `tokio::signal::ctrl_c()`
- Tokio's `signal` feature was already enabled — no new dependencies needed
- Spawned task sets the same `AtomicBool` flag + prints same stderr warning
- Eliminated 3 crates: `ctrlc`, `nix`, `cfg_aliases`
- 3 files changed, 14 insertions, 73 deletions. Cargo.lock: -60 lines
- All 1221 tests pass, zero clippy warnings. Commit: 4a5cec9

### 38.2 — Replace `notify-debouncer-mini` with manual debouncing (Complete - 2026-02-09)
- Replaced `notify-debouncer-mini` with direct `notify::RecommendedWatcher` + manual debounce thread
- Debounce thread: blocks on `recv()` for first event, collects additional events via `recv_timeout()` within debounce window, deduplicates paths via `HashSet`, sends batch through second channel
- Same public API preserved: `FileWatcher::new()`, `watch_directory()`, `unwatch_directory()`, `try_recv()`
- Eliminated 1 crate: `notify-debouncer-mini`
- 3 files changed, 80 insertions, 39 deletions. Cargo.lock: -12 lines
- All 1221 tests pass, zero clippy warnings. Commit: 131141c

### 38.3 — Upgrade `thiserror` v1 → v2 (Complete - 2026-02-09)
- thiserror v2 is fully API-compatible for standard `#[derive(Error)]` usage — zero code changes needed
- v1 remains as transitive dep from `tower_governor` and `forwarded-header-value`
- 2 files changed, 2 insertions, 2 deletions
- All 1221 tests pass, zero clippy warnings. Commit: fe10218

**Key Learnings:**
- When tokio `signal` feature is already enabled, `tokio::spawn` + `ctrl_c().await` cleanly replaces `ctrlc` crate with no new dependencies
- Manual debouncing with `recv()` + `recv_timeout()` + `HashSet` dedup replaces `notify-debouncer-mini` for simple use cases
- `thiserror` v1→v2 is API-compatible for standard derive usage — zero code changes
- Check for crates already available as transitive dependencies before adding new direct dependencies

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 39: Type Safety & Dependency Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Improve type safety by converting string-typed fields to enums, remove redundant dependencies, and tighten module visibility.

### 39.1 — Convert VerifyIssue `issue_type: String` to `IssueType` enum (Complete)
- Created `IssueType` enum with 5 variants: `MissingFile`, `ReadError`, `Modified`, `MissingHeader`, `IdMismatch`
- Added `as_str()`, `icon()`, `Display` impl, and `#[serde(rename)]` for JSON backward compatibility
- Added `Clone, Copy` derives since enum is small (no heap data)
- Eliminated ~15 `.to_string()` allocations and a 6-arm string match block
- Used `sed` for bulk replacement of identical string literal patterns, manual edits for unique match blocks
- Commit: dd93f78

### 39.2 — Remove redundant direct `governor` dependency (Complete)
- Removed `governor = { version = "0.6", optional = true }` from Cargo.toml and `"dep:governor"` from `mcp` feature list
- `tower_governor` re-exports governor via `tower_governor::governor` — no code changes needed
- `rand 0.8` duplicate was NOT eliminated because governor is still pulled in transitively via tower_governor
- Commit: 1451d8a

### 39.3 — Demote organize audit types to `pub(crate)` (Complete)
- Demoted all 13 public items in `organize/audit.rs` to `pub(crate)`: 9 types + 4 functions + 4 impl methods
- Also demoted re-exports in `organize/mod.rs` from `pub use` to `pub(crate) use`
- Demotion surfaced 18 dead_code warnings — used `#![allow(dead_code)]` inner attribute at module level (entire module is operational utility planned for future MCP tools)
- Added `#[allow(unused_imports)]` on re-exports in mod.rs
- Commit: 2bf9a53

**Key Learnings:**
- Convert `String` fields to enums with `as_str() -> &'static str`, `icon()`, `Display` impl, and `#[serde(rename)]` for backward-compatible JSON serialization
- Add `Clone, Copy` derives for small enums (no heap data)
- `Display` impl delegates to `as_str()` for use in format strings
- Use `sed` for bulk replacement of identical string literal patterns, manual edits for unique match blocks
- Module-level `#![allow(dead_code)]` inner attribute works for non-crate-root modules — use for entire operational utility modules planned for future use
- When demoting re-exports from `pub use` to `pub(crate) use`, add `#[allow(unused_imports)]` if items are only used in tests
- Removing direct deps that are re-exported by other crates doesn't necessarily reduce transitive duplicates — verify with `cargo tree --duplicates`

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 40: Dependency Reduction & Import Cleanup (3/3 Complete - 2026-02-09)

**Goal:** Reduce unnecessary dependencies and clean up module re-exports for consistency.

### 40.1 — Replace `tower_governor` with simple rate limiter middleware (Complete)
- Implemented ~50-line sliding-window rate limiter using `Arc<Mutex<VecDeque<Instant>>>`
- Eliminated 12+ transitive deps, Cargo.lock reduced by 278 lines
- Duplicate crate pairs reduced from 39 to 29
- Used `Mutex` (not `tokio::Mutex`) when critical section has no async work

### 40.2 — Remove unused `cors` feature from `tower-http` (Complete)
- Changed features from `["cors", "trace"]` to `["trace"]`
- Audit dependency features for unused ones

### 40.3 — Consolidate organize module re-exports into lib.rs (Complete)
- Added 28 items to `pub use organize::{...}` in `lib.rs`
- Updated 6 files in `commands/organize/` to use `use factbase::{...}`
- No remaining `use factbase::organize::` in binary crate

**Key Learnings:**
- Simple sliding-window rate limiter with `Arc<Mutex<VecDeque<Instant>>>` can replace `tower_governor` for basic rate limiting needs
- Use `Mutex` (not `tokio::Mutex`) when critical section has no async work (trivially short lock hold)
- Audit dependency features for unused ones (e.g., `cors` in tower-http when only `trace` is used)
- `is_some_and()` replaces `map_or(false, ...)` for cleaner Option checking

**Test counts:** 867 lib + 354 binary = 1221 (with all features). Zero clippy warnings.

---

## Phase 41: MCP Transport Compliance (24/24 subtasks Complete - 2026-02-10)

**Goal:** Add stdio transport (`factbase mcp`) and upgrade HTTP transport to Streamable HTTP per MCP spec 2025-03-26.

| Task | Subtasks | Key Files |
|------|----------|-----------|
| 1. Shared MCP Protocol Types | 3/3 | protocol.rs |
| 2. Stdio Transport | 11/11 | stdio.rs, commands/mcp.rs |
| 3. Streamable HTTP Transport | 7/7 | server.rs |
| 4. Documentation Updates | 3/3 | agent-integration.md, quickstart.md |

### Task 1: Shared MCP Protocol Types (3/3)
- Created `src/mcp/protocol.rs` with `initialize_result() -> Value` helper returning MCP initialize response per spec 2025-03-26
- Made `McpRequest.id` `Option<Value>` — notifications have no `id`, requests do
- Added `is_notification()` helper method on `McpRequest`
- `handle_tool_call` returns `Option<McpResponse>` (None for notifications)
- `env!("CARGO_PKG_VERSION")` keeps server version in sync with Cargo.toml

### Task 2: Stdio Transport (11/11)
- Created `src/mcp/stdio.rs` with `run_stdio()` async function — reads newline-delimited JSON-RPC from stdin, writes responses to stdout
- Handlers: initialize, notifications/initialized, tools/list, tools/call, ping, unknown method (-32601)
- `write_response()` helper ensures single-line JSON with flush — critical for stdio protocol correctness
- Uses `serde_json::Value` for initial parsing (not `McpRequest`) — handles malformed messages gracefully
- `serde_json::from_value()` converts already-parsed `Value` into typed `McpRequest` for tools/call
- Created `src/commands/mcp.rs` with `cmd_mcp()` — loads config, creates embedding provider, calls `run_stdio()`
- Wired `Mcp` variant into `Commands` enum in main.rs (feature-gated on `mcp`)
- E2e lifecycle test: initialize → notifications/initialized → tools/list → ping via reader/writer parameterization

### Task 3: Streamable HTTP Transport (7/7)
- Added early return guards in `mcp_handler` for initialize/ping/notification before tool dispatch
- Initialize returns `protocol::initialize_result()` wrapped in JSON-RPC response
- Notifications return HTTP 202 Accepted with no body
- Ping returns `{"jsonrpc":"2.0","id":<id>,"result":{}}`
- GET /mcp returns 405 Method Not Allowed (axum method chaining: `get(handler).post(handler)`)
- Session ID: `Mutex<Option<String>>` in AppState, UUID v4 via `getrandom` on initialize, `Mcp-Session-Id` header, 409 Conflict on mismatch
- Changed `mcp_handler` return type to `Response` (from `Json<T>`) for custom headers — `.into_response()` on all paths
- 6 integration tests: initialize, tools/list, GET 405, notification 202, ping, session mismatch 409

### Task 4: Documentation Updates (3/3)
- Added stdio transport section to `docs/agent-integration.md` with MCP config example (`command`, `args`, `cwd`)
- Updated HTTP section to note Streamable HTTP per MCP spec 2025-03-26 and full lifecycle support
- Added both transport options to `docs/quickstart.md` — stdio first (recommended for local), HTTP second (shared/remote)

**Key Learnings:**
- `protocol::initialize_result()` returns `serde_json::Value` for flexibility — both transports wrap it in their own response format
- `McpRequest.id` is `Option<Value>` — notifications have no `id`, requests do
- HTTP transport returns 202 Accepted for notifications; stdio transport skips writing to stdout
- Stdio transport uses `serde_json::Value` for initial parsing — handles malformed messages gracefully
- `write_response()` helper ensures single-line JSON with flush — critical for stdio protocol correctness
- `serde_json::from_value()` converts already-parsed `Value` into typed struct — avoids double parsing
- HTTP transport: early return guards for initialize/ping/notification before tool dispatch
- Axum doesn't allow two `.route()` calls on the same path — use method chaining: `get(handler).post(handler)`
- Session ID: `Mutex<Option<String>>` in AppState, generate UUID v4 via `getrandom`, validate `Mcp-Session-Id` header (409 on mismatch)
- Axum `Response` return type needed when adding custom headers — use `.into_response()` on all return paths

**Test counts at Phase 41 completion:** 913 lib + 354 binary = 1267 (with all features). Zero clippy warnings.

---

## Phase 42: MCP Tool Consistency & Code Cleanup (3/3 Complete - 2026-02-10)

**Goal:** Fix MCP tool schema/handler argument name mismatches, consolidate repeated setup patterns, and add consistency tests to prevent future drift.

### 42.1 — Fix MCP tool schema/handler arg name mismatches (Complete)
- Standardized all MCP tool type filter parameters to `"doc_type"` across all 4 tools that support it
- Fixed 2 bugs where `search_knowledge` and `list_entities` schemas defined `"doc_type"` but handlers read `"type"`, silently dropping the type filter from agents
- Also standardized `search_temporal` (schema `"type"` → `"doc_type"`, handler `"type"` → `"doc_type"`). `search_content` was already correct
- This was a real bug — AI agents following the tool schema would send `{"doc_type": "person"}` but the handler ignored it
- Tests: 916 lib + 354 binary (916 = 913 + 3 new doc_type extraction tests). Commit: ce5b15b

### 42.2 — Consolidate repeated CachedEmbedding setup pattern (Complete)
- Extracted `setup_cached_embedding(&config, timeout_override)` helper in `commands/setup.rs`
- Combines `setup_embedding_with_timeout` + `CachedEmbedding::new` into a single call
- Replaced 3 call sites in `serve.rs`, `search/mod.rs`, and `mcp.rs`
- Helper takes `Option<u64>` timeout to cover both no-timeout and timeout cases
- Tests: 916 lib + 354 binary. Zero clippy warnings

### 42.3 — Add MCP schema/handler consistency test for arg names (Complete)
- Added `test_schema_doc_type_param_consistency` test in `src/mcp/tools/mod.rs`
- Verifies all tools with document type filtering use `"doc_type"` (not `"type"`)
- Checks 5 specific tools and scans all tools to ensure no `"type"` property has a description mentioning "document type"
- `get_review_queue` legitimately uses `"type"` for question type filtering — correctly allowed
- Tests: 917 lib + 354 binary = 1271. Zero clippy warnings

**Key Learnings:**
- MCP tool schema is the contract — when schema defines `"doc_type"` but handler reads `"type"`, agents silently get no filtering. Always fix handlers to match schema, not vice versa
- Standardize parameter names across all MCP tools (e.g., all type filter params should be `"doc_type"`, not mixed `"type"`/`"doc_type"`)
- Extract `setup_cached_embedding(&config, timeout)` to consolidate repeated `setup_embedding` → `CachedEmbedding::new` pattern across serve/search/mcp commands
- Schema/dispatch consistency tests prevent drift between tool definitions and handlers

**Test counts at Phase 42 completion:** 917 lib + 354 binary = 1271 (with all features). Zero clippy warnings.

---

## Phase 43: Code Deduplication & Test Coverage (3/3 Complete - 2026-02-10)

**Goal:** Add `to_summary_json()`/`to_json()` methods to remaining model structs for consistent JSON serialization, and add missing unit tests for MCP search tools.

### 43.1 — Add Repository::to_summary_json() (Complete)
- Added `to_summary_json(doc_count: usize)` method to `Repository` in `src/models/repository.rs`
- Replaced 2 duplicate manual `json!({...})` blocks in `src/mcp/tools/entity.rs`
- Standardized `last_indexed_at` on RFC3339 format in the method
- 918 lib + 354 binary tests pass. Commit: included in phase

### 43.2 — Add unit tests for search_content MCP tool (Complete)
- Added 5 unit tests to `src/mcp/tools/search/search_content.rs`
- Tests cover: required pattern validation, default/custom argument extraction, doc_type key correctness, response format with empty DB
- `test_db()` lives in `crate::database::tests::test_db()` (cfg(test) submodule), not on `Database` directly

### 43.3 — Add SearchResult::to_json() (Complete)
- Added `to_json() -> serde_json::Value` method to `SearchResult` in `src/models/search.rs`
- Replaced 2 duplicate `serde_json::to_value(r).unwrap_or_default()` calls in `search_knowledge.rs` and `search_temporal.rs`
- Method consumes `self` since both call sites already consume via `into_iter()`
- f32 precision: `0.95_f32` serializes to `0.949999988079071` — use `round()` comparison in tests
- 924 lib + 354 binary = 1278 tests pass. Commit: b5730ad

**Key Learnings:**
- `to_json()` / `to_summary_json()` methods on model structs provide consistent JSON serialization across MCP tools and CLI
- f32 precision in JSON: use `round()` comparison in tests, not exact equality
- `test_db()` helper lives in `crate::database::tests::test_db()` — a cfg(test) submodule, not a method on Database

**Test counts at Phase 43 completion:** 924 lib + 354 binary = 1278 (with all features). Zero clippy warnings.

---

## Phase 44: Code Deduplication & Optimization (9/9 Complete - 2026-02-10)

**Goal:** Extract shared helpers and consolidate duplicated patterns across MCP tools, question generators, and model structs.

| Task | Summary |
|------|---------|
| 44.1 | Extract `normalize_pair()` helper for duplicate detection dedup |
| 44.2 | Extract common MCP search filter parsing |
| 44.3 | Add `ContentSearchResult::to_json()` for consistency |
| 44.4 | Use `to_summary_json()` as base in `get_entity` MCP tool |
| 44.5 | Add `ReviewQuestion::to_json()` and simplify `format_question_json` |
| 44.6 | Consolidate `get_perspective` to use `Repository::to_summary_json()` |
| 44.7 | Extract `fetch_document_links` helper to deduplicate paired link fetching |
| 44.8 | Move `generate_preview` to `output.rs` as shared utility |
| 44.9 | Extract document stats analysis helpers from `get_document_stats` |

**Key patterns:** `to_json()` / `to_summary_json()` methods on model structs for consistent JSON serialization across MCP tools. Extract shared filter parsing and link fetching helpers to eliminate repeated patterns in MCP tool handlers.

**Test counts:** 924 lib + 354 binary = 1278 (with all features). Zero clippy warnings.

---

## Phase 45 Task 1: Fact Extraction Expansion (2/2 Complete - 2026-02-10)

**Summary:** Created `src/question_generator/facts.rs` with `FactLine` struct and `extract_all_facts()` function for cross-document validation. Extracts ALL list items (any indentation level) with section heading tracking. 21 unit tests.

**Key considerations:**
- `clean_fact_text()` does NOT truncate (unlike `extract_fact_text()` which caps at 80 chars) — full text needed for embedding generation
- Checkbox stripping (`[ ]`, `[x]`, `[X]`) is new
- Module is `pub(crate)` for use by `cross_validate.rs`

**Test counts:** 890 lib + 347 binary = 1237 (without web). Zero clippy warnings.

## Phase 45 Task 2: Per-Fact Semantic Search (3/3 Complete - 2026-02-10)

**Summary:** Created `src/question_generator/cross_validate.rs` with `cross_validate_document` async function. Extracts all facts, generates per-fact embeddings, searches for related documents (top 10, excluding source), filters by relevance threshold (0.3), and collects results into `FactWithContext` structs for Task 3's LLM prompt building.

**Key considerations:**
- `FactWithContext` struct holds `FactLine` + `Vec<SearchResult>` — ready for Task 3 to batch into LLM prompts
- `RELEVANCE_THRESHOLD = 0.3` filters out low-similarity results before LLM calls
- Facts with zero relevant results after filtering are skipped entirely (reduces LLM calls for unique/new topics)
- Uses `search_semantic_paginated` (same as MCP `search_knowledge` tool) for consistency
- 3 unit tests with mock embedding/LLM providers

**Test counts:** 893 lib + 347 binary = 1240 (without web). Zero clippy warnings.

## Phase 45 Task 3: LLM Conflict Detection (3/3 Complete - 2026-02-10)

**Summary:** Implemented the full LLM conflict detection pipeline in `cross_validate.rs`: prompt template (`build_prompt`), batching logic (10 facts per call), JSON response parsing (`parse_llm_response` with markdown fence stripping), and `ReviewQuestion` generation (`result_to_question` for CONFLICT/STALE results). Added `CrossCheckResult` serde struct for LLM response deserialization. 15 unit tests.

**Key considerations:**
- `build_prompt()` truncates search result snippets to 200 chars to prevent prompt bloat with large documents
- `parse_llm_response()` strips markdown code fences (`` ```json ... ``` ``) since LLMs commonly wrap JSON in fences
- LLM call failures are logged and skipped (per batch) rather than failing the entire document — graceful degradation
- `result_to_question()` uses `checked_sub(1)` for 1-based fact index safety, returning `None` for index 0 or out-of-bounds
- `extract_title()` extracts from first `# ` heading, falling back to doc_id — used in prompt for human-readable context
- Question descriptions include cross-document source citation: "Cross-check with {source_doc}: {fact} — {reason}"

**Test counts:** 905 lib + 347 binary = 1252 (without web); 966 lib + 354 binary = 1320 (with all features). Zero clippy warnings.

## Phase 45 Task 4: Integration into lint (3/3 Complete - 2026-02-10)

**Summary:** Wired `cross_validate_document` into `cmd_lint` as a separate pass after existing checks. Added `--cross-check` flag to `LintArgs`. Made `cross_validate_document` public and re-exported from `lib.rs`. Set up embedding + LLM providers when flag is used. Progress output via stderr `eprint!` for inline updates.

**Key considerations:**
- `cmd_lint` is already `async fn` — no need for `Runtime::new()` or `block_on`
- Cross-check runs as a separate pass AFTER the batch loop (not inside it) for cleaner separation
- Wired into `mod.rs` directly rather than `review.rs` since it needs async and the embedding/LLM providers
- Graceful degradation: individual document cross-check failures are logged as warnings, not fatal errors
- Respects `--dry-run`: when set, questions are printed but not written to files
- Progress uses `eprint!`/`eprintln!` with `\r` carriage return for inline updates

**Test counts:** 966 lib + 354 binary = 1320 (with all features). Zero clippy warnings.

## Phase 45 Task 5.1: Schema migration and skip logic (Complete - 2026-02-10)

**Summary:** Added `cross_check_hash` nullable TEXT column to the documents table via schema migration v5. Added three database methods: `needs_cross_check()`, `set_cross_check_hash()`, and `clear_cross_check_hashes()`. Modified lint `--cross-check` loop to skip documents where hashes match.

**Key considerations:**
- `cross_check_hash` stores the `file_hash` value (not a separate SHA256 computation) — comparing `cross_check_hash == file_hash` is sufficient
- `needs_cross_check()` returns `true` when no hash is stored (new documents) or when hashes differ (changed documents)
- `set_cross_check_hash()` uses `SET cross_check_hash = file_hash` in a single UPDATE
- `clear_cross_check_hashes()` accepts a slice of IDs for batch invalidation (used by Task 5.3 for linked documents)
- `upsert_document()` does INSERT OR REPLACE which resets `cross_check_hash` to NULL for changed documents

**Test counts:** 972 lib + 354 binary = 1326 (with all features). Zero clippy warnings.

## Phase 45 Task 5.2: Update hash after validation (Complete - 2026-02-10)

**Summary**: Added `db.set_cross_check_hash(&doc.id)?` call in the `Ok(questions)` arm of the cross-check loop in `src/commands/lint/mod.rs`. The call runs after successful cross-validation regardless of whether questions were generated.

**Key considerations:**
- Respects `--dry-run`: hash is only updated when `!args.dry_run`
- Placed outside the `!questions.is_empty()` block — documents with zero conflicts still get marked as checked
- Failed cross-validations (the `Err(e)` arm) do NOT update the hash, so those documents will be retried on the next run
- Only 4 lines of code added — the infrastructure from Task 5.1 did all the heavy lifting

**Test counts:** 972 lib + 354 binary = 1326 (with all features). Zero clippy warnings. Commit: d8bd664.

## Phase 45 Task 5.3: Linked document invalidation on change (Complete - 2026-02-10)

**Summary**: Added cross-check hash invalidation in `full_scan` (scanner/orchestration/mod.rs). After committing document changes, iterates over `changed_ids`, calls `db.get_links_to()` for each to find referencing documents, and calls `db.clear_cross_check_hashes()` on the collected source IDs. Added one integration-style test in `database/documents.rs` verifying the full pattern.

**Key considerations:**
- Placed after `commit_transaction()` so upserted documents are visible, but before duplicate check and link detection
- Excludes documents already in `changed_ids` from invalidation — they already have `cross_check_hash` reset to NULL by `upsert_document`'s INSERT OR REPLACE
- For new documents, `get_links_to` returns empty (no links exist yet), so the iteration is harmless
- For moved-only documents (content unchanged), clearing linked docs' hashes is conservative but correct
- Uses `info!` tracing to log how many linked documents were invalidated
- Only runs when `!opts.dry_run`

**Test counts:** 973 lib + 354 binary = 1327 (with all features). Zero clippy warnings. Commit: d899ec5.
