# Phase 50: Web UI Hardening

## Problem

The web UI exists but needs three things before it's ready for real use:
1. No live-usage tests — only unit tests for serialization
2. Some API endpoints diverge from MCP/CLI code paths
3. The frontend doesn't mirror the MCP workflow patterns for HITL

## Current State

### What's good (API alignment)
- `review.rs`: Calls MCP `get_review_queue`, `answer_question`, `bulk_answer_questions` directly ✅
- `documents.rs`: Calls MCP `get_entity`, `list_repositories` directly ✅
- `organize.rs`: Calls shared `detect_merge_candidates`, `detect_misplaced`, `load_orphan_entries` ✅

### What diverges
- `stats.rs` `compute_review_stats`: Missing `deferred` count (MCP `get_review_queue` returns it)
- `stats.rs` `compute_organize_stats`: Calls detection functions directly instead of through a shared path; `duplicate_entry_count` is always 0 (needs embedding provider)
- `review/status` endpoint: Missing `deferred` field
- `organize.rs`: Has its own orphan file I/O instead of using shared organize functions
- No `apply_review_answers` endpoint — the most critical HITL action
- No `check_repository` endpoint — can't trigger checks from the UI
- No `scan_repository` endpoint — can't trigger scans from the UI

### Frontend gaps for HITL workflow
- No workflow guidance — user sees raw questions without context on how to resolve them
- No deferred question visibility
- No "apply answers" button — can answer questions but can't apply them
- No scan/check triggers
- No archive folder awareness in document views

---

## Task 1: Fix API alignment gaps

- [x] 1.1 Add `deferred` field to `ReviewStatsResponse` and `get_review_status`. Extract from the existing `get_review_queue` response which already returns it.
- [x] 1.2 Add `POST /api/apply` endpoint that calls the shared `apply_all_review_answers`. Accepts `{ repo?: string, doc_id?: string, dry_run?: bool }`. Returns the same `ApplyResult` structure as MCP.
- [x] 1.3 Add `POST /api/scan` endpoint that triggers `scan_repository` (or returns instructions to use CLI, since scan requires embedding provider which the web server may not have).
- [x] 1.4 Add `POST /api/check` endpoint that triggers `check_repository` for local checks (no deep_check from web — that requires LLM). Returns the same response as MCP `check_repository`.

## Task 2: Add Playwright E2E tests

- [x] 2.1 Add Playwright to `web/` devDependencies. Create `web/e2e/` directory with config.
- [x] 2.2 Create test fixture: starts `factbase serve` with a temp repo containing 5-10 test documents (some with review questions, some without). Waits for web server to be ready on port 3001.
- [x] 2.3 Dashboard test: navigate to `/`, verify stats cards render with correct counts, verify auto-refresh works.
- [x] 2.4 Review Queue test: navigate to review page, verify questions render, filter by type, answer a question inline, verify answer persists on reload.
- [x] 2.5 Organize test: navigate to organize page, verify merge/misplaced suggestions render (if any), dismiss a suggestion.
- [x] 2.6 Keyboard navigation test: verify j/k moves between items, Enter opens detail, Escape closes, ? shows help.
- [x] 2.7 Apply test: answer a question, click apply, verify the document content is updated (check via API).

## Task 3: Frontend workflow alignment

- [x] 3.1 Add a workflow banner/stepper at the top of the Review Queue page showing the resolve workflow steps: "1. Review questions → 2. Answer/defer → 3. Apply answers → 4. Verify". Highlight current step based on state (unanswered > 0 → step 1, answered > 0 → step 2, etc.).
- [x] 3.2 Add "Apply Answers" button to the Review Queue page. Shows count of answered questions ready to apply. Calls `POST /api/apply` with `dry_run=true` first to preview, then confirms and applies. Shows per-document results.
- [x] 3.3 Add deferred questions section — prominent card/banner showing "N items need human attention" with filter to show only deferred items. Match the MCP `get_deferred_items` behavior.
- [x] 3.4 Add answer type hints in the AnswerForm component. When the user types, show a hint: "Looks like a source citation — will add footnote automatically" or "Looks like a confirmation — will update @t[~] date". Based on the same `classify_answer` logic.
- [x] 3.5 Add "Scan" and "Check" buttons to the Dashboard. Scan triggers `POST /api/scan`, Check triggers `POST /api/check`. Show progress/results inline.
- [x] 3.6 Add archive badge to document cards — if a document's path contains `/archive/`, show a subtle "archived" badge and explain it's excluded from checks.

---

## Notes

- The web server currently doesn't have an embedding or LLM provider — it only has the Database. Features requiring inference (deep check, scan with embeddings, duplicate entry detection) need to either: (a) be triggered via CLI/MCP instead, or (b) have the web server accept provider config. For now, option (a) is simpler — the web UI shows a "Run via CLI" message for these.
- Playwright tests need the full binary built with `--features web` and a running server. They should be in a separate test target that's not run by default (`cargo test` shouldn't require a running server).
- The frontend is vanilla TypeScript — no React/Vue. Components are functions that return DOM elements. Keep it that way for simplicity.
