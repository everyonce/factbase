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

- [ ] 1.1 Add `pub fn is_deferred(&self) -> bool` method to `ReviewQuestion` in `models/question.rs`: returns `!self.answered && self.answer.is_some()`. Replace all 4 occurrences of the inline pattern in queue.rs, workflow.rs, lint.rs, and status.rs with `q.is_deferred()`.
- [ ] 1.2 Unit test: `is_deferred()` returns true for unchecked question with answer, false for answered, false for unanswered without answer.

## Task 2: Sync MCP tool documentation

Fix all hardcoded tool counts to match the actual schema.

- [ ] 2.1 Update README.md MCP tools table: add `get_deferred_items` entry, update count to 20, remove `search_temporal` (merged into `search_knowledge` in Phase 36). Verify table matches `tools_list()` output exactly.
- [ ] 2.2 Update `.kiro/steering/architecture.md`: change "18 tools" to "20 tools" in the MCP Server box.
- [ ] 2.3 Update `.kiro/steering/current-state.md`: update MCP Tools section to show 20 total, add `get_deferred_items` to the table.

## Task 3: Extract `count_deferred_questions` as Database helper

Consolidate deferred counting into a reusable function.

- [ ] 3.1 Add `pub fn count_deferred_questions(&self, repo_id: Option<&str>) -> Result<usize, FactbaseError>` to `Database` (in `database/stats/basic.rs` or similar). Uses `get_documents_with_review_queue` + `parse_review_queue` + `is_deferred()` filter. Replace inline counting in `workflow.rs::count_deferred()` and `lint.rs` with `db.count_deferred_questions()`.
- [ ] 3.2 Unit test: count_deferred_questions returns correct count with mixed question states.

---

## Outcomes

(To be filled as tasks are completed)
