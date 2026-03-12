# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- Added `FactbaseError` constructor helpers
- Declarative config validation helpers (`require_non_empty`, `require_positive`, `require_range`)
- Demoted internal modules to `pub(crate)` — reduced public API surface
- Trimmed unused `lib.rs` re-exports
- Consolidated duplicate patterns across organize, database, and commands modules
- Standardized imports across source files
- Zero clippy warnings on all features

## [0.4.1] - 2026-02-08

### Fixed
- Temporal questions no longer generated when line has recent `@t[~]` verification (within 180 days)
- Stale source questions now cross-check `@t[~]` tags — recently verified facts skip staleness check
- Conflict detector skips roster lines containing `[[id]]` cross-references

## [0.4.0] - 2026-02-08

### Added
- Amazon Bedrock as inference backend (Titan Embed V2)
- Nova Multimodal Embeddings support (auto-detected from model ID)
- `region` config field for Bedrock (replaces overloaded `base_url`)
- `bedrock` feature included in `full` feature set
- Inbox block integration for `review --apply` (`<!-- factbase:inbox -->`)
- First-run experience: helpful messages when no config exists
- `docs/quickstart.md` — zero-to-searching in 2 minutes
- `docs/inference-providers.md` — Bedrock and Ollama setup guide
- Fact-level cross-document validation via pre-computed fact embeddings (`check --deep-check`)
- Fact-level embeddings generated during scan, powering cross-document conflict detection
- MCP stdio orphan detection — exit when parent process dies
- Folder placement checks via link graph analysis
- `embeddings export`, `embeddings import`, `embeddings status` operations
- Reference entity support via `<!-- factbase:reference -->` marker
- Write concurrency guard for all destructive MCP operations
- Universal opaque `resume` token for all time-budgeted MCP operations
- Ghost file detection (duplicate ID/title in same directory) in organize analyze
- Auto-resolve glossary-defined acronym questions in resolve workflow
- Domain vocabulary extraction in deep-check / discover mode

### Changed
- Default provider changed from Ollama to local CPU embeddings throughout docs and config
- CLI help reorganized: commands grouped logically, low-frequency commands hidden
- README trimmed with CLI reference extracted to `docs/cli-reference.md`
- `examples/config.yaml` updated to reflect current defaults
- Ignore patterns default to `.*/**` (all dot-directories)
- All server-side LLM usage removed — reasoning is now agent-driven via MCP
- Cross-document validation operates on fact pairs instead of whole documents

### Fixed
- `SUM()` NULL error on empty repository stats (COALESCE fix)
- Stale checks no longer flag facts with closed temporal ranges
- Conflict detection limited to Range/Ongoing tags only
- Duplicate questions per line deduplicated (stale subsumes temporal)
- `@t[~]` staleness requires 180-day minimum age

## [0.3.0] - 2026-01-29

### Added

#### Temporal Tags
- Parse `@t[...]` temporal tags from document content during scan
- Six tag types: `@t[=DATE]` (point in time), `@t[~DATE]` (last seen), `@t[DATE..DATE]` (range), `@t[DATE..]` (ongoing), `@t[..DATE]` (historical), `@t[?]` (unknown)
- Date granularity support: year, quarter (Q1-Q4), month, day
- Temporal coverage tracking during scan with configurable threshold
- Temporal tag validation: format correctness, date validity, conflict detection

#### Source Attribution
- Parse `[^N]` footnote references and definitions from documents
- Standard source types: LinkedIn, Website, Press release, News, Filing, Direct, Email, Event, Inferred, Unverified
- Source coverage tracking with orphan detection (refs without defs, defs without refs)

#### Review System
- Review Queue section in documents marked by `<!-- factbase:review -->` comment
- Question types: `@q[temporal]`, `@q[conflict]`, `@q[missing]`, `@q[ambiguous]`, `@q[stale]`, `@q[duplicate]`, `@q[corruption]`, `@q[precision]`
- `factbase check` generates review questions using rule-based analysis
- `review --apply` processes answered questions and updates documents
- `review --status` shows queue summary with breakdown by question type
- `--dry-run` flag for preview mode

#### Temporal-Aware Search
- `--as-of <date>` flag filters results to facts valid at specific point in time
- `--during <range>` flag filters results to facts valid during date range
- `--exclude-unknown` flag excludes `@t[?]` and untagged facts
- `--boost-recent` flag boosts ranking of facts with recent `@t[~...]` dates

#### Per-Repository Review Configuration
- `review.stale_days` in perspective.yaml overrides global threshold
- `review.required_fields` defines required fields per document type
- `review.ignore_patterns` excludes files from review

#### Statistics
- Temporal stats in `status --detailed`: coverage, tag type distribution, date range
- Source stats in `status --detailed`: coverage, source type distribution, orphan counts

## [0.2.0] - 2024-01-25

### Added
- Document chunking for files >100K characters with 2K overlap
- Search result deduplication by document (returns best chunk per document)
- Chunk information in search results and MCP responses
- `max_content_length` parameter for `get_entity` MCP operation
- Example `perspective.yaml` with all options documented

### Changed
- **BREAKING**: Embedding model upgraded (requires full rescan after upgrade)
- Link detection limit increased (4x improvement)
- Improved search result snippets from matching chunk regions

### Fixed
- Foreign key constraint issues with embedding chunks
- sqlite-vec KNN query compatibility

## [0.1.0] - 2024-01-15

### Added
- Initial release
- Filesystem-based knowledge management with semantic search
- MCP server with tools for search, CRUD, review, and workflow operations
- File watcher with 500ms debounce for live updates
- Automatic link detection between documents via string matching
- Document type validation via `perspective.yaml`
- CLI commands: scan, search, serve, status, stats, repo, export, import, doctor, check, db, completions
- Export/import with compression support (zstd)
- Connection pooling with configurable pool size
- Query embedding cache for repeated searches
- Parallel file processing
- Progress bars for long scans
- Shell completion generation (bash, zsh, fish, powershell, elvish)
