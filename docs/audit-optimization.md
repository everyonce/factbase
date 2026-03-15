# Optimization Audit — Hot Paths, DB Queries, Redundant Work

Audit date: 2026-03-15. Read-only analysis; no code changes made.

---

## Summary Table

| # | Location | Issue | Estimated impact | Effort |
|---|----------|-------|-----------------|--------|
| 1 | `scanner/orchestration/mod.rs:319,396` | `derive_type()` called twice per new file | Low–Medium | Low |
| 2 | `scanner/orchestration/mod.rs:357` | `extract_title()` called after content already parsed | Low | Low |
| 3 | `scanner/orchestration/embedding.rs:151–153` | `fs::metadata()` called again inside embedding phase (already read in pre-read) | Low | Low |
| 4 | `scanner/orchestration/mod.rs:597` | `get_links_to()` called in a loop per changed doc (N queries) | Medium | Low |
| 5 | `organize/detect/merge.rs:59–62` | 4 separate `get_links_from/to` calls per candidate pair | Medium | Low |
| 6 | `question_generator/check.rs:266,428` | Each document read from disk twice in `check_all_documents` (async phase + write phase) | Medium | Low |
| 7 | `question_generator/check.rs` | `run_generators()` called twice per document (full pass + stripped pass for suppression count) | Medium | Medium |
| 8 | `database/documents/crud.rs:upsert_document` | `prepare()` (not `prepare_cached`) used for stale-chunk cleanup queries on every upsert | Medium | Low |
| 9 | `database/documents/crud.rs:purge_deleted_documents` | `prepare()` inside a loop — new statement compiled per deleted doc | Medium | Low |
| 10 | `scanner/orchestration/links.rs` | `get_documents_for_repo()` loads full content of all docs for keyword matching; only title/id needed | High | Medium |
| 11 | `scanner/orchestration/mod.rs` | `get_documents_for_repo()` called at scan start loads full content of every doc into memory | High | Medium |
| 12 | `database/schema.rs` | No index on `review_questions(doc_id, status)` composite — status-filtered queries scan all rows for a doc | Low–Medium | Low |
| 13 | `question_generator/check.rs` | `collect_defined_terms_with_types()` iterates all docs once; called once globally — already good, but result is cloned into each async closure via `defined_terms_ref` borrow | Low | — |
| 14 | `scanner/orchestration/mod.rs` | `has_review_section()` + `parse_review_queue()` called on every changed doc during scan, then again during full review sync pass over `known` | Low–Medium | Low |

---

## Detailed Findings

### 1. `derive_type()` called twice per new file
**Location:** `src/scanner/orchestration/mod.rs` lines 319 and 396.

For a new file (no existing ID), `derive_type()` is called at line 319 to inject the ID header, then called again at line 396 for the `PendingDoc`. The result is identical both times (path hasn't changed). The first result could be stored in a local variable and reused.

**Impact:** Low–Medium — proportional to the number of new files per scan.

---

### 2. `extract_title()` called after content already parsed
**Location:** `src/scanner/orchestration/mod.rs` line 357.

`extract_title()` scans the content string for the first H1 heading. This is called after the content has already been read and hashed. For large documents this is a redundant linear scan. The title could be extracted once during pre-read or combined with the hash pass.

**Impact:** Low — `extract_title` is fast, but it runs on every changed document.

---

### 3. `fs::metadata()` called again inside embedding phase
**Location:** `src/scanner/orchestration/embedding.rs` lines 151–153.

`file_modified_at` is populated by calling `fs::metadata(&doc.path)` inside `run_embedding_phase`. The pre-read phase (`pre_read_files`) already calls `fs::metadata` and stores `modified_at` in `PreReadFile`. However, `modified_at` is not threaded through to `PendingDoc`, so the embedding phase re-reads it from disk.

**Impact:** Low — one extra syscall per changed document, but syscalls are cheap on warm cache.

---

### 4. `get_links_to()` called in a loop per changed doc (N queries)
**Location:** `src/scanner/orchestration/mod.rs` lines 594–603.

After scanning, the code iterates `changed_ids` and calls `db.get_links_to(id)` for each one to find documents that need cross-check hash invalidation. With N changed documents this is N separate SQL queries. A single `WHERE target_id IN (...)` query would replace all of them.

**Impact:** Medium — significant on large incremental scans with many changed docs.

---

### 5. 4 separate link queries per merge candidate pair
**Location:** `src/organize/detect/merge.rs` lines 59–62.

For each candidate pair `(doc, similar)`, four separate queries are issued:
- `get_links_from(doc.id)`
- `get_links_to(doc.id)`
- `get_links_from(similar_id)`
- `get_links_to(similar_id)`

These could be batched into two `IN (...)` queries covering all candidate IDs at once, or the link counts could be pre-fetched for all docs before the loop.

**Impact:** Medium — proportional to the number of merge candidates.

---

### 6. Each document read from disk twice in `check_all_documents`
**Location:** `src/question_generator/check.rs` lines 266 and 428.

In the async question-generation phase (line 266), each document's file is read from disk. Then in the sequential write phase (line 428), the same file is read again to get fresh content before writing. The disk content from the async phase could be passed through the result tuple to avoid the second read.

**Impact:** Medium — one extra `fs::read_to_string` per document that needs a write (i.e., every document with new or pruned questions).

---

### 7. `run_generators()` called twice per document
**Location:** `src/question_generator/check.rs` (async closure, ~line 295 and ~line 310).

For every document, `run_generators()` is called once with the full body (to get the question list), then called again with `strip_reviewed_markers(body)` to count suppressed questions. The second call re-runs all generators on a slightly modified string. The suppression count could instead be computed by diffing the first result against a filtered version, avoiding the second full generator pass.

**Impact:** Medium — doubles generator CPU time per document. Generators include regex scans over the full content.

---

### 8. `prepare()` instead of `prepare_cached()` for stale-chunk cleanup in `upsert_document`
**Location:** `src/database/documents/crud.rs`, `upsert_document` function.

The stale-chunk and stale-fact cleanup queries use `conn.prepare(...)` rather than `conn.prepare_cached(...)`. Since `upsert_document` is called for every changed document during a scan, these statements are compiled fresh on every call. Switching to `prepare_cached` would amortize statement compilation across the scan.

**Impact:** Medium — statement compilation overhead multiplied by the number of upserted documents.

---

### 9. `prepare()` inside a loop in `purge_deleted_documents`
**Location:** `src/database/documents/crud.rs`, `purge_deleted_documents` function.

Inside the `for id in &ids` loop, `conn.prepare("SELECT id FROM fact_metadata WHERE document_id = ?1")` is called on every iteration. This compiles the same SQL statement once per deleted document. The fact IDs for all deleted documents could be fetched in a single `WHERE document_id IN (...)` query before the loop.

**Impact:** Medium — proportional to the number of deleted documents per scan.

---

### 10. `get_documents_for_repo()` loads full content for link keyword matching
**Location:** `src/scanner/orchestration/links.rs`, `run_link_detection_phase`.

`all_docs = db.get_documents_for_repo(repo_id)` loads the full content of every document (including compressed/decompressed content) to build the keyword-match filter (`doc.content.to_lowercase().contains(kw)`). For keyword matching, only the document content is needed — not the full `Document` struct with all metadata. A lighter query returning only `(id, title, content)` would reduce memory pressure and decompression work.

**Impact:** High — for a 1000-document repo, this loads and decompresses all content into memory on every scan that has any changes.

---

### 11. `get_documents_for_repo()` at scan start loads all content
**Location:** `src/scanner/orchestration/mod.rs`, `full_scan` function (early in the function).

`known = db.get_documents_for_repo(&repo.id)` loads the full content of every document at the start of every scan. This is used for:
- Hash comparison (`d.file_hash != hash`) — only `file_hash` needed
- Path comparison (`d.file_path != relative`) — only `file_path` needed
- Review queue merge (`merge_review_queue`) — content needed only for changed docs

A lighter initial query fetching only `(id, file_hash, file_path)` would avoid loading and decompressing all document content upfront. Content could be fetched on-demand only for documents that are actually changed.

**Impact:** High — this is the single largest memory allocation in a typical scan. For a 500-document repo with 10KB average content, this is ~5MB of strings loaded before any file is processed.

---

### 12. Missing composite index on `review_questions(doc_id, status)`
**Location:** `src/database/schema.rs`.

The schema has separate indexes on `review_questions(doc_id)` and `review_questions(status)`, but no composite index on `(doc_id, status)`. Queries that filter by both (e.g., "open questions for document X") must use one index and filter the other in memory. A composite index `(doc_id, status)` would make these queries index-only.

**Impact:** Low–Medium — depends on review queue size. Becomes more significant as the review table grows.

---

### 13. `collect_defined_terms_with_types()` — already efficient
**Location:** `src/question_generator/check.rs`.

This is called once before the document loop and the result is shared via reference. No issue here.

---

### 14. Double `has_review_section()` + `parse_review_queue()` during scan
**Location:** `src/scanner/orchestration/mod.rs` and `src/scanner/orchestration/embedding.rs`.

During the embedding phase, `has_review_section()` and `parse_review_queue()` are called for each changed document to sync review questions. Later, the full review sync pass iterates `known` and calls the same functions again on all documents with review sections. For documents that were just changed and synced, this is redundant work. The set of already-synced IDs could be tracked to skip them in the full sync pass.

**Impact:** Low–Medium — only triggered when the review table is under-populated (migration case), so not on every scan.

---

## Parallelization Opportunities

| Location | Opportunity | Notes |
|----------|-------------|-------|
| `check_all_documents` write phase | Currently sequential for filesystem safety; could batch DB writes while keeping file writes sequential | Medium effort |
| `purge_deleted_documents` | All cleanup queries for a single doc are sequential; could use a single transaction with batched `IN (...)` deletes | Low effort |
| `organize/detect/merge.rs` | Link count queries for all candidates could be fetched in one pass | Low effort |

---

## Priority Order

1. **#11** — Lazy content loading in `get_documents_for_repo` at scan start (highest memory impact)
2. **#10** — Lighter query for link keyword matching (high memory, every scan with changes)
3. **#4** — Batch `get_links_to` for cross-check invalidation (N→1 queries)
4. **#6** — Avoid double disk read in `check_all_documents`
5. **#7** — Avoid double `run_generators` pass
6. **#8, #9** — `prepare_cached` and batched deletes in CRUD
7. **#1, #3** — Minor redundant calls in scan loop
8. **#12** — Composite index on `review_questions`
