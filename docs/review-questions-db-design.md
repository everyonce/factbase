# Review Questions DB Table — Design Document

**Status:** Implemented (schema v16, migration complete)  
**Table:** `review_questions`  
**Purpose:** Fast indexed access to review questions without parsing markdown files on every query.

---

## Fundamental Constraint: File is Source of Truth

Markdown files are the canonical location for review questions. The DB table is an **index**, not the authority.

- External edits (Obsidian, vim, git pull) must be detected and synced on next scan
- If file and DB disagree, **file wins**
- The DB index enables O(1) queue queries instead of O(n) file parsing

---

## Schema

```sql
CREATE TABLE review_questions (
    id INTEGER PRIMARY KEY,
    doc_id TEXT NOT NULL,
    question_index INTEGER NOT NULL,   -- 0-based position in the review section
    question_type TEXT NOT NULL,       -- temporal, conflict, missing, etc.
    description TEXT NOT NULL,         -- question text (without line ref prefix)
    line_ref INTEGER,                  -- source line number in document body
    answer TEXT,                       -- blockquote answer text (if any)
    status TEXT NOT NULL DEFAULT 'open',  -- open | verified | deferred | believed | dismissed
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (doc_id) REFERENCES documents(id),
    UNIQUE(doc_id, question_index)
);
```

**Status values:**
- `open` — unanswered `[ ]` question with no blockquote answer
- `verified` — answered `[x]` question with blockquote answer
- `deferred` — unanswered `[ ]` with `defer:` blockquote answer
- `believed` — unanswered `[ ]` with `believed:` blockquote answer
- `dismissed` — DB-only status; question is suppressed from future syncs by description match

---

## Code Path Audit

### Writers — Create or Modify Questions in Files

#### 1. `check_repository` / question generators
**Location:** `src/question_generator/check.rs:471`  
**What it does now:** Generates new questions, appends them to the review section in the file, then calls `db.sync_review_questions(&doc.id, &questions)`.  
**DB interaction:** Full sync after writing. Questions written to file first, then DB is updated from the parsed result.  
**Edge cases:**
- If the file write succeeds but the DB sync fails (e.g., connection error), the DB is stale until next scan. The `let _ =` suppresses the error — acceptable since scan will re-sync.
- Questions are deduplicated by description before appending (`append_review_questions` checks existing descriptions).

#### 2. `answer_question` / `bulk_answer_questions`
**Location:** `src/services/review/answer.rs`  
**What it does now:**
- Single answer: writes to file, calls `db.update_document_content`, then calls `db.update_review_question_status` with the new status.
- Bulk answer: writes all files, updates DB content in a transaction, then calls `db.sync_review_questions` for each written document.
**DB interaction:**
- Single: targeted `UPDATE review_questions SET status=?, answer=? WHERE doc_id=? AND question_index=?`
- Bulk: full `sync_review_questions` (DELETE + INSERT) after writing
**Edge cases:**
- Single answer uses `update_review_question_status` which updates by `(doc_id, question_index)`. If the question_index shifted due to a concurrent file edit, the wrong row gets updated. The next scan will re-sync and correct this.
- `let _ =` on the DB update — if it fails, the file is already written. Next scan corrects the DB.

#### 3. `correct` workflow step 3
**Location:** `src/mcp/tools/workflow/` (correct workflow)  
**What it does now:** Calls `update_document` which rewrites file content. May remove or modify questions as a side effect of the LLM rewrite.  
**DB interaction:** `update_document` calls `db.update_document_content` which updates the `documents` table content. The `review_questions` table is **not** updated here.  
**Gap:** After a correct workflow rewrite, the `review_questions` table may be stale until the next scan triggers `sync_review_questions`. This is acceptable — the file is truth.

#### 4. `normalize_review_section`
**Location:** `src/processor/review/normalize.rs`  
**What it does now:** Deduplicates questions, strips orphaned markers/blockquotes, removes empty sections. Called as part of `append_review_questions`.  
**DB interaction:** None directly. Normalization happens before the DB sync in `check_repository`.  
**Edge cases:** If normalization changes question indices (e.g., removes a duplicate), the DB sync that follows will correctly re-index from the normalized content.

#### 5. `prune_stale_questions`
**Location:** `src/processor/review/prune.rs`  
**What it does now:** Removes unanswered questions whose trigger conditions no longer exist. Called during `check_repository` before appending new questions.  
**DB interaction:** None directly. The subsequent `sync_review_questions` call in `check_repository` handles the DB update.

#### 6. `strip_answered_questions`
**Location:** `src/processor/review/prune.rs`  
**What it does now:** Removes `[x]` questions from files during scan (Pass 1 in `full_scan`).  
**DB interaction:** None at strip time. The document is then upserted to DB and `sync_review_questions` is called during the embedding phase.  
**Edge cases:** Strip happens before the document is added to `pending`. The stripped content is what gets stored in the DB and synced. Answered questions are removed from both file and DB index.

---

### Readers — Parse Questions from Files or DB

#### 7. `parse_review_queue`
**Location:** `src/processor/review/parse.rs`  
**What it does now:** Parses `@q[...]` questions from markdown content. Returns `Vec<ReviewQuestion>`.  
**DB interaction:** None. Pure file parsing. Used as input to `sync_review_questions`.

#### 8. `get_review_queue` / `factbase(op=review_queue)`
**Location:** `src/services/review/queue.rs`  
**What it does now:** Reads from `review_questions` DB table via `query_review_questions_db`. Does **not** parse files.  
**DB interaction:** SELECT with filters (repo, doc_id, type, status, limit, offset).  
**Edge cases:**
- If DB is stale (file edited externally), returns stale data until next scan.
- The `load_review_docs_from_disk` function in `workflow/helpers.rs` is used by the resolve workflow as a fallback — it reads disk content directly when the DB `has_review_queue` flag may be stale.

#### 9. `get_deferred_items` / `factbase(op=deferred)`
**Location:** `src/services/review/queue.rs`  
**What it does now:** Delegates to `get_review_queue` with `status=deferred`. Reads from DB.  
**DB interaction:** Same as #8, filtered to `status IN ('deferred', 'believed')`.

#### 10. Completion gate (resolve workflow)
**Location:** `src/mcp/tools/workflow/helpers.rs` — `compute_type_distribution`, `load_review_docs_from_disk`  
**What it does now:** Counts unanswered questions per type to determine if the resolve loop is complete. Uses `load_review_docs_from_disk` which reads disk files directly (not the DB index) to avoid stale `has_review_queue` flag issues.  
**DB interaction:** Reads disk files, not the `review_questions` table. This is intentional — the resolve workflow needs filesystem truth.  
**Note:** This is a deliberate bypass of the DB index for correctness. The DB index is used for fast queries; the resolve workflow uses disk for accuracy.

---

### Modifiers — Change File Structure Around Questions

#### 11. `update_document` / `factbase(op=update)`
**Location:** `src/services/entity.rs` (or similar)  
**What it does now:** Rewrites file content. May affect the review section if the new content includes or excludes it.  
**DB interaction:** Updates `documents.content`. Does **not** update `review_questions`.  
**Gap:** After `update_document`, the `review_questions` table is stale. Corrected on next scan.

#### 12. `organize merge/split/move`
**Location:** `src/organize/execute/`  
**What it does now:** Moves files, merges content, splits documents. Review sections travel with the document content.  
**DB interaction:** Updates `documents` table (new path, new content). Does **not** update `review_questions`.  
**Gap:** After organize operations, `review_questions` rows for the old doc_id may be orphaned or stale. The next scan corrects this via `sync_review_questions`.  
**Note:** For merge, the source document is deleted (`mark_deleted`). The `review_questions` FK constraint means rows for deleted docs remain but are excluded from queries via `JOIN documents d ON ... AND d.is_deleted = FALSE`.

#### 13. Callout wrapping/unwrapping
**Location:** `src/processor/review/callout.rs`  
**What it does now:** Converts between plain `## Review Queue` and Obsidian `> [!review]- Review Queue` formats. Pure content transformation.  
**DB interaction:** None. The `review_questions` table stores parsed question data, not the raw markdown format. Format changes are transparent to the DB index.

---

## Sync Points Summary

| Event | DB Sync Method | Timing |
|-------|---------------|--------|
| Scan (embedding phase) | `sync_review_questions` (full replace) | Per document, during scan |
| Scan (skip_embeddings mode) | `sync_review_questions` (full replace) | Per document, during scan |
| `check_repository` | `sync_review_questions` (full replace) | After writing questions to file |
| `answer_question` (single) | `update_review_question_status` (targeted) | After writing to file |
| `bulk_answer_questions` | `sync_review_questions` (full replace) | After writing all files |
| `auto_dismiss_question` | `update_document_content` only | No review_questions sync |
| Migration v16 | `backfill_review_questions` | One-time on upgrade |

---

## Design Questions — Answers

### 1. When does DB sync happen? Only on scan? Or on every write?

**Answer:** Both. The DB is synced:
- On every scan (full replace per document)
- On `check_repository` (full replace after writing questions)
- On `answer_question` (targeted status update)
- On `bulk_answer_questions` (full replace after writing)

Operations that rewrite file content without going through the review service (e.g., `update_document`, organize operations) do **not** sync the DB. The next scan corrects this.

### 2. How to handle the "believed" status?

**Answer:** `believed` is a distinct status in the DB. The parser detects it via `ReviewQuestion::is_believed()` which checks if the answer text starts with `"believed: "`. The `sync_review_questions` function maps this to the `believed` status. The `query_review_questions_db` function groups `deferred` and `believed` together for the deferred count.

### 3. Should the DB track question text or just a hash?

**Answer:** Full text is stored. The description is used for:
- Dismissed status preservation across re-syncs (matched by description, not index)
- Bulk dismiss by description pattern (`bulk_update_review_question_status` with `LIKE`)
- Display in `get_review_queue` responses

Text changes on normalize (e.g., line ref stripping) are handled by `extract_line_ref_and_strip` which normalizes descriptions before storage. The `description` column stores the stripped form (without `Line N:` prefix).

### 4. What happens when a question is answered in the file but DB says 'open'?

**Answer:** The next scan calls `sync_review_questions` which does a full DELETE + INSERT for the document. The new insert reads the answered state from the file and sets status to `verified`. The DB converges to file truth on every scan.

For the `answer_question` service path, the DB is updated immediately after the file write via `update_review_question_status`. The file-first, then DB pattern ensures the file is always the authority.

### 5. Performance: scanning 5,462 questions on every scan — acceptable?

**Answer:** Yes. `sync_review_questions` does:
1. One SELECT to get dismissed descriptions (indexed by `doc_id`)
2. One DELETE by `doc_id` (indexed)
3. N INSERTs for the document's questions

For a document with 10 questions, this is ~12 DB operations. For 5,462 questions across ~546 documents (avg 10 each), the total is ~6,500 DB operations during a full scan. SQLite handles this in milliseconds with WAL mode.

The `query_review_questions_db` path (used by `get_review_queue`) is a single indexed SELECT — O(1) regardless of total question count.

### 6. Migration: how to populate the table from existing files on first scan?

**Answer:** Migration v16 includes `backfill_review_questions` which:
1. Reads all non-deleted documents from the `documents` table
2. Parses each document's content for review questions
3. Inserts rows with `INSERT OR IGNORE` (idempotent)

This runs once when upgrading from schema v15 to v16. After migration, the normal scan path keeps the table in sync.

---

## Known Gaps and Edge Cases

### Gap 1: `update_document` does not sync `review_questions`

When an agent calls `factbase(op=update)` to rewrite a document, the `review_questions` table is not updated. If the rewrite removes or adds questions, the DB is stale until the next scan.

**Mitigation:** The next scan corrects this. For the resolve workflow, `load_review_docs_from_disk` reads disk directly, bypassing the stale DB.

**Future improvement:** `update_document` could call `sync_review_questions` after writing. Low priority since scans are frequent.

### Gap 2: `auto_dismiss_question` does not sync `review_questions`

`auto_dismiss_question` in `workflow/helpers.rs` writes to the file and calls `update_document_content` but does not call `sync_review_questions` or `update_review_question_status`.

**Impact:** The dismissed question remains `open` in the DB until next scan.

**Fix candidate:** Add `db.update_review_question_status(doc_id, question_index, "dismissed", None)` after the file write.

### Gap 3: Organize operations leave orphaned rows

When `organize merge` deletes a source document, its `review_questions` rows remain in the DB. They are excluded from queries via `JOIN documents d ON ... AND d.is_deleted = FALSE`, so they don't surface to users. However, they accumulate as dead rows.

**Mitigation:** The FK constraint does not cascade delete (SQLite default). A periodic `DELETE FROM review_questions WHERE doc_id NOT IN (SELECT id FROM documents WHERE is_deleted = FALSE)` could clean these up, but is not currently implemented.

### Gap 4: `question_index` is positional, not stable

The `question_index` is the 0-based position of the question in the review section. If questions are reordered (e.g., by normalize), the index shifts. `update_review_question_status` uses `(doc_id, question_index)` to target a specific row.

**Impact:** If a question is answered via `answer_question` and then the file is edited to reorder questions before the next scan, the DB status update may have targeted the wrong question. The next scan's `sync_review_questions` corrects this by re-deriving status from the file.

**Mitigation:** The `dismissed` status is preserved by description match (not index), so dismissals survive reordering. Other statuses are re-derived from file content on sync.

### Gap 5: `--since` filter skips unchanged files

When `factbase scan --since <date>` is used, only recently modified files are processed. Documents not in the scan window do not get `sync_review_questions` called. If those documents were edited externally (e.g., via Obsidian), their `review_questions` rows remain stale.

**Mitigation:** A full scan (without `--since`) corrects all documents. The `--since` filter is an optimization for incremental updates.

---

## Implementation Notes

### `sync_review_questions` is the canonical sync function

All paths that write questions to files should eventually call `sync_review_questions`. The function:
1. Preserves `dismissed` status by description (survives re-syncs)
2. Derives status from the `ReviewQuestion` struct fields (`answered`, `is_believed()`, `is_deferred()`)
3. Does a full DELETE + INSERT (not upsert) to handle question removal correctly

### The `dismissed` status is DB-only

`dismissed` has no representation in the markdown file. It is a DB-side suppression mechanism. When `sync_review_questions` runs, it reads the current dismissed descriptions and re-applies the status to matching questions in the new sync.

This means: if a user dismisses a question via the DB, then the file is edited to change the question description, the dismissed status is lost on the next sync (description no longer matches). This is intentional — a changed description is a new question.

### `load_review_docs_from_disk` bypasses the DB index

The resolve workflow uses `load_review_docs_from_disk` which reads disk files directly. This is a deliberate design choice to handle the case where `has_review_queue` is stale (e.g., `check_repository` wrote questions to disk but the DB flag wasn't updated). The disk read ensures the workflow sees the filesystem truth.

This function loads ALL documents (not just `has_review_queue=TRUE`) and filters by `REVIEW_QUEUE_MARKER` presence in the disk content. It is O(n) in document count but only runs during the resolve workflow step, not on every query.
