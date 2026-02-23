# Phase 49: Consolidation & Documentation Sync

Depends on: Phase 48 (complete).

## Problem

After completing Phase 48, several consolidation opportunities exist:

1. **Deferred question detection duplicated** — The pattern `!q.answered && q.answer.is_some()` appears in 4 files (queue.rs, workflow.rs, lint.rs, status.rs). This is a semantic concept ("is this question deferred?") that should be a method on `ReviewQuestion`.

2. **MCP tool count documentation drift** — Tool counts are hardcoded in README.md (21), architecture.md (18), current-state.md (21), and schema.rs (20). After adding `get_deferred_items`, the actual count is 20 but docs are inconsistent.

3. **Deferred counting duplicated across MCP tools** — `count_deferred()` in workflow.rs and the inline counting in lint.rs both iterate all docs with review queues and count deferred items. This pattern could be a `Database` method.

---

## Task 1: Add `is_deferred()` method to ReviewQuestion

Consolidate the deferred detection pattern into a single method.

- [x] 1.1 Add `pub fn is_deferred(&self) -> bool` method to `ReviewQuestion` in `models/question.rs`: returns `!self.answered && self.answer.is_some()`. Replace all 4 occurrences of the inline pattern in queue.rs, workflow.rs, lint.rs, and status.rs with `q.is_deferred()`.
- [x] 1.2 Unit test: `is_deferred()` returns true for unchecked question with answer, false for answered, false for unanswered without answer.

## Task 2: Sync MCP tool documentation

Fix all hardcoded tool counts to match the actual schema.

- [x] 2.1 Update README.md MCP tools table: add `get_deferred_items` entry, update count to 20, remove `search_temporal` (merged into `search_knowledge` in Phase 36). Verify table matches `tools_list()` output exactly.
- [x] 2.2 Update `.kiro/steering/architecture.md`: change "18 tools" to "20 tools" in the MCP Server box.
- [x] 2.3 Update `.kiro/steering/current-state.md`: update MCP Tools section to show 20 total, add `get_deferred_items` to the table.

## Task 3: Extract `count_deferred_questions` as Database helper

Consolidate deferred counting into a reusable function.

- [x] 3.1 Add `pub fn count_deferred_questions(&self, repo_id: Option<&str>) -> Result<usize, FactbaseError>` to `Database` (in `database/stats/basic.rs` or similar). Uses `get_documents_with_review_queue` + `parse_review_queue` + `is_deferred()` filter. Replace inline counting in `workflow.rs::count_deferred()` and `lint.rs` with `db.count_deferred_questions()`.
- [x] 3.2 Unit test: count_deferred_questions returns correct count with mixed question states.

---

## Outcomes

### Task 1: Add `is_deferred()` method to ReviewQuestion (commit fa6b989)
- **Summary**: Added `pub fn is_deferred(&self) -> bool` to `ReviewQuestion` in `models/question.rs`. Replaced all 4 inline occurrences of `!q.answered && q.answer.is_some()` across `queue.rs`, `workflow.rs`, `lint.rs`, and `status.rs`. Added 3 unit tests covering deferred, answered, and unanswered states.
- **Key considerations**: Each call site used the pattern slightly differently (some as filter closures, one as a `let` binding, one in an `if` condition), so replacements were tailored per site.
- **Difficulties**: None — straightforward mechanical refactor. All 1057 lib + 351 bin tests pass.

### Task 2: Sync MCP tool documentation (commit 1b513e9)
- **Summary**: Rebuilt README.md MCP tools table from scratch to match the 20 tools in `schema.rs`. Removed 7 stale entries (get_document_stats, answer_question, bulk_answer_questions, generate_questions, workflow_start, workflow_next, search_temporal). Added 6 missing entries (init_repository, apply_review_answers, get_deferred_items, get_authoring_guide, answer_questions, workflow). Updated architecture.md from "18 tools" to "20 tools" and removed search_temporal reference. Cleaned up current-state.md header (tool list was already correct).
- **Key considerations**: Verified exact match between README tool names and schema.rs `tools_list()` output using sorted diff — zero discrepancies. The current-state.md already had the correct 20-tool list from Phase 48 work, only the header annotation needed cleanup.
- **Difficulties**: None — the README was significantly out of date (7 removed/renamed tools, 6 missing), but the ground truth in schema.rs was clear. All 1057 lib + 351 bin tests pass.

### Task 3: Extract `count_deferred_questions` as Database helper (commit 1971dae)
- **Summary**: Added `pub fn count_deferred_questions(&self, repo_id: Option<&str>) -> Result<usize, FactbaseError>` to `Database` in `database/stats/basic.rs`. Removed standalone `count_deferred()` function from `workflow.rs` and replaced with `db.count_deferred_questions().unwrap_or(0)`. Replaced 5-line inline counting in `lint.rs` with the same one-liner. Added unit test with 3 question states (deferred, answered, unanswered) plus repo filtering and nonexistent repo edge case.
- **Key considerations**: The method lives on `Database` but delegates to `crate::processor::parse_review_queue` — this is acceptable since both are in the same crate and the database module already references processor patterns (e.g., `REVIEW_QUEUE_MARKER` in crud.rs). Used `unwrap_or(0)` at call sites to match the original error-swallowing behavior of the standalone function.
- **Difficulties**: None — straightforward extraction. All 1058 lib + 351 bin tests pass.
