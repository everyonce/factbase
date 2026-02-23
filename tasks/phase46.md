# Phase 46: Cross-Document Entity Deduplication

Depends on: Phase 45 (cross-document fact validation) for the `extract_all_facts` infrastructure.

## Problem

Current merge detection (`organize/detect/merge.rs`) compares whole documents by embedding similarity. It catches `people/jane-smith.md` duplicated as `people/jane.md`. It cannot detect the same entity appearing as an entry within multiple parent documents — e.g., Jane Smith listed under both `companies/acme.md` and `companies/globex.md` with one entry being stale.

## Task 1: Entity entry extraction

- [x] 1.1 Create `src/organize/detect/entity_entries.rs` with `extract_entity_entries(content: &str, doc_id: &str) -> Vec<EntityEntry>`. An `EntityEntry` is a named block within a document: a heading + its child facts. For example, under `## Team` in a company doc, "### Jane Smith" followed by list items is one entry. Use heading hierarchy (H3 under H2, or bold list items) to identify entry boundaries. Register in `src/organize/detect/mod.rs`.
- [x] 1.2 `EntityEntry` struct: `name: String` (the heading/label), `doc_id: String` (parent document), `section: String` (parent H2), `facts: Vec<String>` (child list items), `line_start: usize`, `line_end: usize`. Add to `src/organize/types.rs` or inline in the new module.
- [x] 1.3 Unit tests: extract entries from a company doc with `## Team` → `### Person A` / `### Person B`, from a doc with bold-name list items (`- **Jane Smith** - VP Engineering`), from a doc with no sub-entries (returns empty).

## Task 2: Cross-document entry matching

- [x] 2.1 Create `detect_duplicate_entries(db: &Database, embedding: &dyn EmbeddingProvider, repo_id: Option<&str>) -> Result<Vec<DuplicateEntry>>`. For each `EntityEntry` extracted from all documents, generate an embedding of the entry name, search for similar entries across other documents. Group matches by entity name similarity.
- [x] 2.2 `DuplicateEntry` struct: `entity_name: String`, `entries: Vec<EntryLocation>` where `EntryLocation` has `doc_id`, `doc_title`, `section`, `line_start`, `facts: Vec<String>`. This represents "Jane Smith appears in these N documents."
- [x] 2.3 Filtering: skip entries that are just cross-reference links (`[[id]]`). Skip entries where the parent documents are the entity's own document (person doc mentioning themselves). Use the link graph to identify the entity's authoritative document if one exists.

## Task 3: Staleness determination

- [x] 3.1 For each `DuplicateEntry` with 2+ locations, determine which entries are current vs stale. Use temporal tags if present (entry with `@t[2024..]` is newer than `@t[2020..2023]`). If no temporal tags, use the parent document's `file_modified_at` as a proxy.
- [x] 3.2 Generate review questions: `@q[stale]` on the older entry's parent document — "Jane Smith also appears in [other_doc] with more recent information. Is this entry still current?" Include the line number range so the reviewer knows exactly which block to check.

## Task 4: Integration into organize

- [x] 4.1 Wire `detect_duplicate_entries` into `factbase organize analyze` alongside existing merge/split/misplaced detection. This requires the embedding provider (for entry name search), so it needs the same async setup as cross-validation in Phase 45.
- [x] 4.2 Add output formatting for duplicate entries in the organize analyze display — show entity name, which documents contain it, and which entry appears stale.
- [x] 4.3 Optionally also surface duplicate entries via the `get_review_queue` MCP tool or as a new organize suggestion type in the web UI.

## Outcomes

### Task 1.1 — Entity entry extraction module (commit 373fbfd)
- Created `src/organize/detect/entity_entries.rs` with `EntityEntry` struct and `extract_entity_entries()` function
- Two extraction patterns: H3+ headings under H2 sections, and bold-name list items (`- **Name** - desc`)
- Entries without child facts are filtered out (headings alone don't constitute an entity entry)
- Registered in `detect/mod.rs` and re-exported from `organize/mod.rs`
- 9 unit tests, 921 lib tests + 347 bin tests all passing
- Key difficulty: consecutive bold-name list items required finalizing the previous entry before starting a new one — initial `current_entry.is_none()` guard prevented detecting the second entry. Fixed by always checking for bold names and finalizing any open entry first.

### Tasks 2.1+2.2 — Duplicate entry detection + structs (commit c3a6db0)
- Created `src/organize/detect/duplicate_entries.rs` with `detect_duplicate_entries()` async function
- Added `DuplicateEntry` and `EntryLocation` structs to `organize/types.rs`
- Two-phase matching approach: (1) exact normalized name grouping (lowercase, trim, collapse whitespace), (2) embedding-based fuzzy matching for singleton entries with 0.85 cosine similarity threshold
- Only flags entries appearing in 2+ different documents — same-document entries are not duplicates
- Deleted documents excluded via `collect_active_documents()` filter
- Results sorted by entry count descending (most duplicated first)
- 8 unit tests with `HashEmbedding` mock that produces deterministic vectors from text hash
- Key difficulty: `upsert_document()` hardcodes `is_deleted = FALSE`, so deleted doc test needed `upsert` then `mark_deleted()` instead of setting the field before insert
- 929 lib tests + 347 bin tests all passing

### Task 2.3 — Cross-reference and self-mention filtering (commit f34b359)
- Added three filtering steps in `detect_duplicate_entries()` after entry extraction, before grouping:
  1. Skip entries whose facts are ALL `[[id]]` cross-reference links (uses `MANUAL_LINK_REGEX` from patterns.rs)
  2. Skip self-mentions where normalized entry name matches normalized parent document title
  3. Build title→doc_id map to identify authoritative documents; exclude entries from the entity's own canonical document
- Added `is_cross_reference()` helper that strips list markers and checks if remaining text is just a `[[hex]]` link
- Replaced `std::collections::HashSet` with imported `HashSet` for consistency
- 5 new tests covering: cross-reference filtering, self-mention filtering, authoritative doc exclusion, mixed facts preservation, is_cross_reference helper
- Key difficulty: test IDs must be valid 6-char hex strings (`[a-f0-9]{6}`) to match `MANUAL_LINK_REGEX` — initial test used `js1234` which contains non-hex chars
- 934 lib tests + 347 bin tests all passing

### Task 3.1 — Staleness determination (commit 342253c)
- Created `src/organize/detect/staleness.rs` with `StaleDuplicate` struct and `assess_staleness()` function
- Temporal tag recency: Ongoing → today, LastSeen/PointInTime → start_date, Range/Historical → end_date, Unknown → None
- Falls back to `file_modified_at` from database when no temporal tags present
- Entries with no determinable date on any entry are skipped (no false positives)
- Same-date entries are not flagged as stale
- `StaleDuplicate` groups entries into `current` (newest) and `stale` (older) for downstream question generation
- 11 unit tests covering: date parsing, temporal tag types, fallback to file_modified_at, no-date skip, same-date skip, multiple stale entries
- Key difficulty: `models::temporal` and `models::question` are private modules — must import via `crate::models::TemporalTagType` re-export path
- 945 lib tests + 347 bin tests all passing

### Task 3.2 — Stale entry review question generation (commit ab02da2)
- Added `generate_stale_entry_questions()` to staleness module
- Returns `HashMap<doc_id, Vec<ReviewQuestion>>` mapping stale entry parent documents to their questions
- Each question is `@q[stale]` type with `line_ref` pointing to the entry's start line
- Description includes entity name, the document with newer info (title + ID), and line number for reviewer context
- 3 new tests covering: basic question generation, multiple stale entries across docs, empty input
- 948 lib tests + 347 bin tests all passing

### Task 4.1 — Wire duplicate entry detection into organize analyze (commit 8fac147)
- Wired `detect_duplicate_entries()` and `assess_staleness()` into `factbase organize analyze` command
- Added `duplicate_entries: Vec<DuplicateEntry>` and `stale_entries: Vec<StaleDuplicate>` to `AnalysisResults` struct
- Embedding provider already available from split candidate detection — reused `&*embedding` for duplicate entry detection
- Added `Serialize` derive to `StaleDuplicate` (was missing, needed for JSON output format)
- Added table output formatting: duplicate entries show entity name, document count, and per-document details (title, ID, line, fact count); stale entries show entity name with current vs stale document locations
- Exported `DuplicateEntry`, `EntryLocation`, `StaleDuplicate`, `assess_staleness`, `detect_duplicate_entries`, `generate_stale_entry_questions` from `lib.rs`
- Updated 3 existing tests to include new `duplicate_entries` and `stale_entries` fields
- 1009 lib tests (with web) + 347 bin tests all passing, clippy clean (only pre-existing warnings)

### Task 4.2 — Enhanced duplicate entry display with inline staleness (commit aac845d)
- Cross-references duplicate entries with stale entries to show `[CURRENT]`/`[STALE]` tags inline on each entry location
- Shows section names with `§` prefix (e.g., `§Team`, `§Staff`) for each entry location
- Removed separate "Stale Entries" table section — staleness info now integrated into duplicate entries display
- Stale entries still tracked in `AnalysisResults` struct and included in JSON output for programmatic consumers
- Staleness lookup built as `HashMap<(entity_name, doc_id), &str>` for O(1) annotation during display
- 1 new test verifying inline staleness tags and section display
- 948 lib tests + 348 bin tests all passing, clippy clean

### Task 4.3 — MCP tool and web UI for duplicate entries (commit pending)
- Created `src/mcp/tools/organize.rs` with async `get_duplicate_entries` MCP tool
- Tool runs full `detect_duplicate_entries()` + `assess_staleness()` pipeline, returns JSON with `duplicates` array (entity name, entries with doc/section/facts), `stale` array (current vs stale entries), and counts
- Added tool schema to `schema.rs` with optional `repo` filter parameter
- Wired into `handle_tool_call` as async dispatch (needs embedding provider, like `search_knowledge`)
- Updated schema/dispatch consistency test: 19 tools total
- Web API: Added `duplicate_entries: Vec<DuplicateEntry>` to `SuggestionsResponse` (always empty — requires embedding provider not available in web server, same limitation as split detection)
- Web API: Added `duplicate_entry_count: usize` to `OrganizeStatsResponse` (always 0 in web, use CLI or MCP for actual detection)
- Frontend: Added `DuplicateEntry` and `EntryLocation` TypeScript types to `api.ts`
- Frontend: Added `renderDuplicateSection()` to OrganizeSuggestions page with entity name, document locations, sections, and fact counts
- Frontend: Updated `SuggestionCard.renderSuggestionTypeBadge()` to support 'duplicate' type (rose color, 👥 icon)
- Frontend: Updated Dashboard to include `duplicate_entry_count` in organize total
- 1011 lib tests (with web) + 355 bin tests + 56 frontend tests all passing, clippy clean
- Key consideration: Web server lacks embedding provider (same as split detection), so duplicate detection is only available via MCP server or CLI. Web UI shows the type but will always be empty until embedding is added to WebAppState.

## Task 5: Post-phase cleanup and optimization

- [x] 5.1 Fix 3 clippy warnings in Phase 46 modules: `iter().copied().collect()` → `to_vec()` in `duplicate_entries.rs:115`, manual char comparison → char array in `duplicate_entries.rs:178`, manual range contains → `!(3..=6).contains(&level)` in `entity_entries.rs:150`.
- [x] 5.2 Consolidate duplicate `normalize_type` logic: `processor/core.rs` (`normalize_type` + `singularize` methods) and `organize/execute/retype.rs` (`normalize_type` free function) implement identical lowercase+strip-trailing-s logic. Extract to a shared `normalize_type()` free function (e.g., in `processor/core.rs` or `models/document.rs`) and call from both sites.
- [x] 5.3 Consolidate 3 duplicate `EmbeddingProvider` test mocks: `MockEmbedding` in `embedding.rs` (configurable dim, constant vector), `MockEmbedding` in `cross_validate.rs` (fixed 1024 dim, constant vector), and `HashEmbedding` in `duplicate_entries.rs` (16 dim, hash-based deterministic vector). Extract into a shared `#[cfg(test)]` module (e.g., `embedding::test_helpers`) with both constant-vector and hash-based variants, then replace all 3 inline definitions.

### Task 5.1 — Fix 3 clippy warnings in Phase 46 modules (commit 5f76d51)
- Fixed `iter().copied().collect()` → `to_vec()` in `duplicate_entries.rs:115`
- Fixed manual char comparison `|c| c == '-' || c == '*'` → `['-', '*']` in `duplicate_entries.rs:178`
- Fixed manual range contains `level < 3 || level > 6` → `!(3..=6).contains(&level)` in `entity_entries.rs:150`
- All 22 affected tests pass, clippy now reports zero warnings with `--all-features`
- No difficulties encountered — straightforward mechanical fixes

### Task 5.2 — Consolidate duplicate normalize_type logic (commit 3d81ffe)
- Extracted `normalize_type` from `DocumentProcessor` method + `singularize` helper into a single `pub(crate)` free function in `processor/core.rs`
- Re-exported via `processor/mod.rs` as `pub(crate) use core::normalize_type`
- `organize/execute/retype.rs` now imports from `crate::processor::normalize_type` instead of defining its own copy
- Net reduction of 10 lines across 3 files
- Existing tests in both modules pass without modification (retype's `test_normalize_type` and core's `test_derive_type_singularizes`)
- 950 lib + 348 bin tests passing, zero clippy warnings
- No difficulties encountered — straightforward extraction

### Task 5.3 — Consolidate 3 duplicate EmbeddingProvider test mocks (commit 84b3c4a)
- Created `#[cfg(test)] pub(crate) mod test_helpers` in `embedding.rs` with two shared mock types:
  - `MockEmbedding`: configurable dimension via `::new(dim)`, returns constant `vec![0.1; dim]`
  - `HashEmbedding`: 16-dim, deterministic vector from text hash (identical inputs → identical embeddings)
- Replaced inline `MockEmbedding` in `embedding.rs` tests with import from `test_helpers`
- Replaced inline `MockEmbedding` in `cross_validate.rs` tests — changed `&MockEmbedding` (unit struct) to `&MockEmbedding::new(1024)` at 3 call sites; kept local `BoxFuture` alias needed by `MockLlm`
- Replaced inline `HashEmbedding` in `duplicate_entries.rs` tests, removed 3 unused imports (`FactbaseError`, `Future`, `Pin`)
- Net reduction of 11 lines across 3 files
- 1011 lib tests (all features) + 348 bin tests passing, zero clippy warnings
- Key consideration: `cross_validate.rs` still needs its own `BoxFuture` type alias for the `MockLlm` impl since the parent module doesn't import it — only the `EmbeddingProvider` mock was consolidated, not the `LlmProvider` mock
