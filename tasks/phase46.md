# Phase 46: Cross-Document Entity Deduplication

Depends on: Phase 45 (cross-document fact validation) for the `extract_all_facts` infrastructure.

## Problem

Current merge detection (`organize/detect/merge.rs`) compares whole documents by embedding similarity. It catches `people/jane-smith.md` duplicated as `people/jane.md`. It cannot detect the same entity appearing as an entry within multiple parent documents — e.g., Jane Smith listed under both `companies/acme.md` and `companies/globex.md` with one entry being stale.

## Task 1: Entity entry extraction

- [ ] 1.1 Create `src/organize/detect/entity_entries.rs` with `extract_entity_entries(content: &str, doc_id: &str) -> Vec<EntityEntry>`. An `EntityEntry` is a named block within a document: a heading + its child facts. For example, under `## Team` in a company doc, "### Jane Smith" followed by list items is one entry. Use heading hierarchy (H3 under H2, or bold list items) to identify entry boundaries. Register in `src/organize/detect/mod.rs`.
- [ ] 1.2 `EntityEntry` struct: `name: String` (the heading/label), `doc_id: String` (parent document), `section: String` (parent H2), `facts: Vec<String>` (child list items), `line_start: usize`, `line_end: usize`. Add to `src/organize/types.rs` or inline in the new module.
- [ ] 1.3 Unit tests: extract entries from a company doc with `## Team` → `### Person A` / `### Person B`, from a doc with bold-name list items (`- **Jane Smith** - VP Engineering`), from a doc with no sub-entries (returns empty).

## Task 2: Cross-document entry matching

- [ ] 2.1 Create `detect_duplicate_entries(db: &Database, embedding: &dyn EmbeddingProvider, repo_id: Option<&str>) -> Result<Vec<DuplicateEntry>>`. For each `EntityEntry` extracted from all documents, generate an embedding of the entry name, search for similar entries across other documents. Group matches by entity name similarity.
- [ ] 2.2 `DuplicateEntry` struct: `entity_name: String`, `entries: Vec<EntryLocation>` where `EntryLocation` has `doc_id`, `doc_title`, `section`, `line_start`, `facts: Vec<String>`. This represents "Jane Smith appears in these N documents."
- [ ] 2.3 Filtering: skip entries that are just cross-reference links (`[[id]]`). Skip entries where the parent documents are the entity's own document (person doc mentioning themselves). Use the link graph to identify the entity's authoritative document if one exists.

## Task 3: Staleness determination

- [ ] 3.1 For each `DuplicateEntry` with 2+ locations, determine which entries are current vs stale. Use temporal tags if present (entry with `@t[2024..]` is newer than `@t[2020..2023]`). If no temporal tags, use the parent document's `file_modified_at` as a proxy.
- [ ] 3.2 Generate review questions: `@q[stale]` on the older entry's parent document — "Jane Smith also appears in [other_doc] with more recent information. Is this entry still current?" Include the line number range so the reviewer knows exactly which block to check.

## Task 4: Integration into organize

- [ ] 4.1 Wire `detect_duplicate_entries` into `factbase organize analyze` alongside existing merge/split/misplaced detection. This requires the embedding provider (for entry name search), so it needs the same async setup as cross-validation in Phase 45.
- [ ] 4.2 Add output formatting for duplicate entries in the organize analyze display — show entity name, which documents contain it, and which entry appears stale.
- [ ] 4.3 Optionally also surface duplicate entries via the `get_review_queue` MCP tool or as a new organize suggestion type in the web UI.

## Outcomes
