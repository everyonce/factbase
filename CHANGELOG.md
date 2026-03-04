# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Fact-level cross-document validation via pre-computed fact embeddings (`check --deep-check` / `check_repository` mode='cross_validate')
- Fact-level embeddings generated during scan, powering cross-document conflict detection
- Auto-populate fact embeddings on first scan after migration
- MCP stdio orphan detection — exit when parent process dies
- Folder placement checks via link graph analysis (moved from organize to check as review questions)
- Time-boxing support for `organize_analyze` MCP tool
- `embeddings_export`, `embeddings_import`, `embeddings_status` MCP tools
- `force_reindex` parameter for `scan_repository` MCP tool
- Reference entity support via `<!-- factbase:reference -->` marker
- Write concurrency guard for all destructive MCP operations
- Split `check_repository` into explicit modes: `questions`, `cross_validate`, `discover`
- Universal opaque `resume` token for all time-budgeted MCP operations (replaces `checked_pair_ids` / `checked_doc_ids` cursors)
- Repo parameter resolution by both ID and name in MCP tools
- Ghost file detection (duplicate ID/title in same directory) in `organize_analyze`
- Auto-resolve glossary-defined acronym questions in resolve workflow
- Domain vocabulary extraction in deep-check / discover mode
- Temporal consistency audit in organize merge/split planning

### Changed
- Cross-document validation now operates on fact pairs instead of whole documents
- MCP tool count increased from 21 to 25
- Stability tests gated behind `#[ignore]` (require Ollama)
- `checked_pair_ids` and `checked_doc_ids` parameters deprecated (kept for backward compatibility, ignored)
- Server-side pagination state replaces client-side cursor tracking

### Fixed
- WriteGuard serialization for flaky MCP tool tests
- Deep-check summary line after cross-validate completes
- Repo-local DB resolution for fact embeddings in cross_validate mode
- Questions mode `docs_processed` count uses question generation count, not cross-validation count
- LLM passed through to `check_all_documents` in check questions mode
- Cache cross-doc fact pairs to avoid O(n²) recomputation on continuation calls
- Deep-check cross-validation no longer skipped when question generation exhausts time budget
- Prevent infinite loop on continuation calls with no remaining pairs
- Skip question generation and vocab/entity discovery on continuation calls

## [0.4.3] - 2026-02-09

### Changed
- Removed `walkdir` dependency (replaced with stdlib `read_dir`)
- Removed `ctrlc` dependency (replaced with `tokio::signal`)
- Removed `async-trait` dependency (manual `BoxFuture` desugaring)
- Removed `notify-debouncer-mini` dependency (manual debouncing)
- Removed `tower_governor` dependency (simple built-in rate limiter)
- Removed `rand` dependency (replaced with `getrandom` for ID generation)
- Upgraded `thiserror` v1 → v2, `dirs` v5 → v6, `reqwest` 0.11 → 0.12
- Upgraded `zerocopy` 0.7 → 0.8, `tower-http` 0.5 → 0.6

## [0.4.2] - 2026-02-09

### Fixed
- Removed all `unwrap()` from production code
- Replaced `process::exit()` with proper error propagation via `anyhow::bail!`
- Database errors now propagate instead of being silently swallowed

### Changed
- Replaced `anyhow::anyhow!()` with `bail!`/`.context()` throughout
- Added `FactbaseError` constructor helpers (`.embedding()`, `.llm()`, `.config()`, `.parse()`, `.not_found()`, `.internal()`)
- Declarative config validation helpers (`require_non_empty`, `require_positive`, `require_range`)
- Demoted internal modules to `pub(crate)` — reduced public API surface
- Trimmed 38 unused `lib.rs` re-exports
- Consolidated duplicate patterns across organize, database, and commands modules
- Standardized imports (`use` over inline paths) across 52 source files
- Zero clippy warnings on all features

## [0.4.1] - 2026-02-08

### Fixed
- Temporal questions no longer generated when line has recent `@t[~]` verification (within 180 days)
- Stale source questions now cross-check `@t[~]` tags — recently verified facts skip staleness check
- Conflict detector skips roster lines containing `[[id]]` cross-references

## [0.4.0] - 2026-02-08

### Added
- Amazon Bedrock as default inference backend (Titan Embed V2 + Claude Haiku via Converse API)
- Nova Multimodal Embeddings support (auto-detected from model ID)
- `region` config field for Bedrock (replaces overloaded `base_url`)
- `bedrock` feature included in `full` feature set
- Inbox block integration for `review --apply` (`<!-- factbase:inbox -->`)
- First-run experience: helpful messages when no config exists
- `docs/quickstart.md` — zero-to-searching in 2 minutes
- `docs/inference-providers.md` — Bedrock and Ollama setup guide

### Changed
- Default provider changed from Ollama to Bedrock throughout docs, examples, and config
- CLI help reorganized: commands grouped logically, low-frequency commands hidden
- README trimmed with CLI reference extracted to `docs/cli-reference.md`
- `examples/config.yaml` updated to Bedrock defaults
- Ignore patterns default to `.*/**` (all dot-directories)

### Fixed
- `SUM()` NULL error on empty repository stats (COALESCE fix)
- Stale checks no longer flag facts with closed temporal ranges
- Conflict detection limited to Range/Ongoing tags only
- Duplicate questions per line deduplicated (stale subsumes temporal)
- `@t[~]` staleness requires 180-day minimum age

### Changed (Code Organization, from Phase 8)
- Split `processor.rs` (2984 lines) into 6 focused submodules
- Split `lint.rs` (1657 lines) into 4 submodules
- Split `database.rs` (3407 lines) into 8 submodules
- All public APIs preserved via re-exports

## [0.3.0] - 2026-01-29

### Added

#### Temporal Tags
- Parse `@t[...]` temporal tags from document content during scan
- Six tag types: `@t[=DATE]` (point in time), `@t[~DATE]` (last seen), `@t[DATE..DATE]` (range), `@t[DATE..]` (ongoing), `@t[..DATE]` (historical), `@t[?]` (unknown)
- Date granularity support: year, quarter (Q1-Q4), month, day
- Temporal coverage tracking during scan with configurable threshold (`temporal.min_coverage`)
- Temporal tag validation in lint: format correctness, date validity, conflict detection, illogical sequence detection
- `--check-temporal` flag for lint command

#### Source Attribution
- Parse `[^N]` footnote references and definitions from documents
- Standard source types: LinkedIn, Website, Press release, News, Filing, Direct, Email, Event, Inferred, Unverified
- Source coverage tracking with orphan detection (refs without defs, defs without refs)
- `--check-sources` flag for lint command

#### Review System
- Review Queue section in documents marked by `<!-- factbase:review -->` comment
- Six question types: `@q[temporal]`, `@q[conflict]`, `@q[missing]`, `@q[ambiguous]`, `@q[stale]`, `@q[duplicate]` (plus `@q[corruption]` and `@q[precision]` added later)
- `lint --review` generates review questions using rule-based analysis
- `review --apply` processes answered questions and updates documents via LLM
- `review --status` shows queue summary with breakdown by question type
- `--dry-run` flag for preview mode
- Separate review LLM configuration (`review.model` in config.yaml)

#### Temporal-Aware Search
- `--as-of <date>` flag filters results to facts valid at specific point in time
- `--during <range>` flag filters results to facts valid during date range
- `--exclude-unknown` flag excludes `@t[?]` and untagged facts
- `--boost-recent` flag boosts ranking of facts with recent `@t[~...]` dates
- Configurable recency window and boost factor in config.yaml

#### MCP Tools
- `get_review_queue` - list pending review questions with filtering
- `answer_question` - mark a question as answered with response
- `generate_questions` - run review on specific document
- `as_of`, `during`, `exclude_unknown` parameters for `search_knowledge` tool

#### Per-Repository Review Configuration
- `review.stale_days` in perspective.yaml overrides global threshold
- `review.required_fields` defines required fields per document type
- `review.ignore_patterns` excludes files from review

#### Statistics
- Temporal stats in `status --detailed`: coverage, tag type distribution, date range
- Source stats in `status --detailed`: coverage, source type distribution, orphan counts
- JSON/YAML output includes temporal and source statistics

### Changed
- Lint command now supports `--json` flag for machine-readable output
- Status command includes temporal and source stats when `--detailed` is used

## [0.2.0] - 2024-01-25

### Added
- Document chunking for files >100K characters with 2K overlap
- Search result deduplication by document (returns best chunk per document)
- Chunk information in search results and MCP responses
- `max_content_length` parameter for `get_entity` MCP tool
- Example `perspective.yaml` with all options documented
- README badges for build status, tests, and license

### Changed
- **BREAKING**: Embedding model upgraded from nomic-embed-text (768-dim) to qwen3-embedding:0.6b (1024-dim)
- **BREAKING**: Database schema changed for 1024-dimension embeddings (automatic migration, requires full rescan)
- Link detection limit increased from 2K to 8K characters (4x improvement)
- Improved search result snippets from matching chunk regions

### Fixed
- Foreign key constraint issues with embedding chunks
- sqlite-vec KNN query compatibility

## [0.1.0] - 2024-01-15

### Added
- Initial release
- Filesystem-based knowledge management with semantic search
- MCP server with 8 tools (search, get, list, create, update, delete, bulk_create, perspective)
- File watcher with 500ms debounce for live updates
- Multi-repository support with namespace isolation
- LLM-powered automatic link detection between documents
- Document type validation via `perspective.yaml`
- CLI commands: init, scan, search, serve, status, stats, repo, export, import, doctor, lint, db, completions
- Export/import with compression support (zstd)
- Connection pooling with configurable pool size
- Rate limiting for MCP server
- Query embedding cache for repeated searches
- Parallel file processing with rayon
- Progress bars for long scans
- Shell completion generation (bash, zsh, fish, powershell, elvish)
