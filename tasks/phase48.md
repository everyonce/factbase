# Phase 48: Review System Robustness

Depends on: Phase 47 (complete).

## Problem

Six user-reported issues all stem from gaps in the review lifecycle:

1. **Lint reports inflated totals** — `total_questions_generated: 4154` but most duplicate already-answered questions. No breakdown of net-new vs existing.
2. **Apply triggers re-linting loops** — answering a `@t[~2026-02]` stale question rewrites it to `@t[=2026]`, lint sees a "new" fact and re-asks the same question.
3. **Cross-validation conflates entity roles** — product capabilities flagged as "stale" because a source footnote's person is inactive. The product fact has nothing to do with that person.
4. **Deferred questions lack visibility** — deferred items blend into the queue with no prominent surfacing.
5. **Answers lack structure** — "Phonetool lookup, 2026-02-10" and "Needs re-verification" go through the same processing path. No typed handling.
6. **Review queue section gets messy** — duplicate `## Review Queue` headers, leftover `@q[...]` markers after apply.

## Root Causes

These map to five architectural gaps:

| Gap | Feedback Items |
|-----|---------------|
| A. No reviewed-fact memory — apply removes questions, lint regenerates them | #1, #2 |
| B. Answer interpretation too coarse — everything goes through LLM rewrite | #2, #5 |
| C. Review section format fragile — no cleanup pass after apply | #6 |
| D. Cross-validation lacks entity role awareness | #3 |
| E. Deferred items not surfaced prominently | #4 |

## Design Decisions

### Reviewed-fact tracking via `<!-- reviewed:YYYY-MM-DD -->` HTML comments

After apply processes a question for a fact line, append an invisible HTML comment to that line:
```markdown
- VP of Engineering at Acme @t[~2026-02] [^1] <!-- reviewed:2026-02-15 -->
```

This is:
- Invisible in rendered markdown
- Survives file moves and renames (travels with the line)
- Parseable by lint to skip recently-reviewed facts
- Consistent with the existing `<!-- factbase:ID -->` pattern

Lint skips generating questions for facts with a `<!-- reviewed:YYYY-MM-DD -->` marker newer than `stale_days` (default 180 days). This prevents the regeneration loop.

### Answer type classification

Classify answers before processing:

| Answer Pattern | Type | Apply Behavior |
|---------------|------|---------------|
| "dismiss", "ignore" | Dismissal | Remove question, no changes |
| "defer", "later", "needs ..." | Deferral | Keep question, mark deferred |
| Source name + optional date | SourceCitation | Add/update footnote, update `@t[~date]`, add reviewed marker |
| "confirmed", "still accurate", "yes" | Confirmation | Update `@t[~today]`, add reviewed marker |
| "correct: ..." or explicit correction | Correction | LLM rewrite of the fact |
| "delete", "remove" | Deletion | Remove the fact line |

Key insight: only Correction and complex cases need LLM rewrite. SourceCitation and Confirmation can be handled deterministically, which avoids the LLM changing temporal tag formats.

### Cross-validation with entity role context

Enrich the cross-validation prompt with source footnote context per fact. Instruct the LLM:
> "A source person becoming inactive does not make the FACT stale — it means the source may need re-verification. Only flag CONFLICT or STALE when the subject entity or the factual claim itself is contradicted."

### Review section cleanup

Add a `normalize_review_section()` pass that runs after every apply:
- Deduplicate `## Review Queue` headers
- Remove orphaned `@q[...]` markers outside the review queue section
- Strip empty blockquote lines (`> `) left from removed answers
- Remove the entire section if no questions remain (already done, but verify edge cases)

---

## Task 1: Reviewed-fact markers

Add `<!-- reviewed:YYYY-MM-DD -->` tracking to prevent lint regeneration loops.

- [x] 1.1 Add `REVIEWED_MARKER_REGEX` to `patterns.rs`: matches `<!-- reviewed:(\d{4}-\d{2}-\d{2}) -->` and captures the date. Add `extract_reviewed_date(line: &str) -> Option<NaiveDate>` helper.
- [x] 1.2 In `generate_stale_questions()` (`question_generator/stale.rs`): before generating a stale question for a fact line, check for a reviewed marker. If the marker date is within `max_age_days` of today, skip the fact. This prevents re-asking about recently confirmed facts.
- [x] 1.3 In `generate_temporal_questions()`, `generate_missing_questions()`, and `generate_ambiguous_questions()`: similarly skip facts with a reviewed marker newer than `max_age_days`. These generators don't currently check for reviewed status.
- [x] 1.4 In `apply_one_document()` (`answer_processor/apply_all.rs`): after successfully applying changes to a fact line, append `<!-- reviewed:YYYY-MM-DD -->` (today's date) to the affected line. If a marker already exists on the line, update its date.
- [x] 1.5 In `remove_processed_questions()`: when removing a dismissed question, also add a reviewed marker to the referenced fact line (if identifiable via `line_ref`). Dismissal means "human looked at it."
- [x] 1.6 Unit tests: `extract_reviewed_date` parses valid/invalid markers. Stale generator skips recently-reviewed facts. Stale generator still flags facts with old reviewed markers.

## Task 2: Answer type classification

Introduce structured answer types so apply can handle different answers deterministically.

- [x] 2.1 Add `AnswerType` enum to `answer_processor/mod.rs`: `Dismissal`, `Deferral`, `SourceCitation { source: String, date: Option<String> }`, `Confirmation`, `Correction { detail: String }`, `Deletion`. Add `classify_answer(answer: &str) -> AnswerType` function that pattern-matches on the answer text.
- [x] 2.2 Refactor `interpret_answer()` to call `classify_answer()` first, then map `AnswerType` → `ChangeInstruction`. `SourceCitation` → `AddSource` + `UpdateTemporal` (last-seen date). `Confirmation` → `UpdateTemporal` (refresh last-seen to today). `Deferral` → new `ChangeInstruction::Defer` variant (keeps question, unchecks checkbox).
- [x] 2.3 Add `ChangeInstruction::Defer` variant. In `apply_one_document()`, when all instructions are Defer, uncheck the `[x]` checkboxes but keep the questions and answers in the review queue (converting answered → deferred).
- [x] 2.4 For `SourceCitation` answers: implement deterministic footnote addition. Parse existing footnotes to find the next number, append `[^N]` to the fact line, add `[^N]: {source}, {date}` to the footnotes section. No LLM needed.
- [x] 2.5 For `Confirmation` answers: deterministic `@t[~]` date update. If fact has `@t[~DATE]`, update DATE to today (or answer-provided date). If no temporal tag, add `@t[~today]`. Preserve the `~` prefix — never convert to `=`. No LLM needed.
- [x] 2.6 Unit tests: `classify_answer` correctly identifies each type. Source citation patterns: "LinkedIn, 2026-01", "Phonetool lookup 2026-02-10", "per annual report". Confirmation patterns: "confirmed", "still accurate", "yes, verified". Deferral patterns: "defer", "needs re-verification", "check later".

## Task 3: Review section cleanup

Harden the review queue section handling to prevent format degradation.

- [x] 3.1 Add `normalize_review_section(content: &str) -> String` to `processor/review.rs`. This function: (a) merges duplicate `## Review Queue` headers into one, (b) removes orphaned `@q[...]` markers that appear outside the review queue section, (c) strips empty blockquote lines (`>` with only whitespace) that aren't part of an answer, (d) normalizes whitespace around the review queue marker.
- [x] 3.2 Call `normalize_review_section()` at the end of `apply_one_document()` after all changes are written, and at the end of `append_review_questions()` before returning.
- [x] 3.3 In `remove_processed_questions()`: after removing questions, if the remaining queue section has only whitespace/empty lines (no actual questions), remove the entire `## Review Queue` section including the marker. Currently this check exists but may miss edge cases with leftover blank lines or markers.
- [x] 3.4 Unit tests: duplicate headers merged, orphaned `@q[stale]` markers removed, empty blockquotes stripped, full section removed when last question is processed.

## Task 4: Lint net-new reporting

Report the breakdown of new vs existing questions so users know actual work ahead.

- [x] 4.1 Expand `LintDocResult` to include: `existing_unanswered: usize` (questions already in queue, not yet answered), `existing_answered: usize` (questions already answered/dismissed), `skipped_reviewed: usize` (facts skipped due to reviewed markers). Current `questions_added` becomes `new_questions`.
- [x] 4.2 In `lint_all_documents()`: before the dedup step, count how many generated questions match existing ones (currently silently filtered). Track the counts in the expanded `LintDocResult`.
- [x] 4.3 Update MCP `lint_repository` response to include aggregate totals: `new_unanswered`, `already_in_queue`, `skipped_reviewed`, `total_generated` (raw count before dedup). The existing `total_questions_generated` becomes `total_generated` and `new_unanswered` shows the actionable count.
- [x] 4.4 Update CLI `cmd_lint` output to show the breakdown: "Generated 4154 total, 97 new (4057 already in queue, 12 skipped as recently reviewed)".
- [x] 4.5 Unit tests: lint on a doc with existing answered questions reports correct breakdown. Lint on a doc with reviewed markers reports `skipped_reviewed` count.

## Task 5: Cross-validation entity role distinction

Prevent cross-validation from conflating source entities with subject entities.

- [x] 5.1 In `extract_all_facts()` (`question_generator/facts.rs`): for each fact line, also extract any source footnote references (`[^N]`). Add `source_refs: Vec<u32>` field to `FactLine`.
- [x] 5.2 In `cross_validate_document()`: after extracting facts, also parse source definitions from the document. For each `FactWithContext`, attach the source definition text for any footnotes referenced by that fact.
- [x] 5.3 Update `build_prompt()` to include source context per fact. After the fact text, add: `Sources for this fact: [^1] LinkedIn profile, scraped 2024-01-15`. Add instruction: "IMPORTANT: Distinguish between the SUBJECT of a fact (the entity the fact describes) and entities mentioned only as SOURCES (people who provided or verified the information). A source person becoming inactive or changing roles does NOT make the fact itself stale — it only means the source may need re-verification. Only flag CONFLICT or STALE when the factual claim itself is contradicted by other documents."
- [x] 5.4 Unit tests: prompt includes source context. Integration test (if feasible): fact about a product with a stale source person is not flagged as stale.

## Task 6: Deferred item surfacing

Make deferred questions prominently visible as human action items.

- [x] 6.1 Add `get_deferred_items` MCP tool that calls `get_review_queue` with `status: "deferred"` and returns a focused response: `{ deferred_items: [...], total_deferred: N, summary: "6 items need human attention" }`. Register in tool schema and routing.
- [x] 6.2 In `workflow_start` (resolve workflow): include deferred item count in the initial assessment. If deferred items exist, the first workflow step should surface them: "You have 6 deferred items that need human attention before proceeding."
- [x] 6.3 In MCP `lint_repository` response: add `deferred_count` field showing how many existing deferred questions exist across the repository.
- [x] 6.4 In CLI `factbase review --status`: add a "Deferred" row to the summary table showing deferred count prominently.
- [x] 6.5 Unit tests: `get_deferred_items` returns only deferred questions. Workflow start includes deferred count.

---

## Outcomes

### Task 1.1 — Add REVIEWED_MARKER_REGEX and extract_reviewed_date (commit af6c473)
- Added `REVIEWED_MARKER_REGEX` static `LazyLock<Regex>` to `patterns.rs` matching `<!-- reviewed:(\d{4}-\d{2}-\d{2}) -->` with date capture group
- Added `extract_reviewed_date(line: &str) -> Option<chrono::NaiveDate>` helper that combines regex capture + `NaiveDate::parse_from_str` validation
- Used `chrono::NaiveDate` qualified path to avoid adding a top-level import (patterns.rs only uses chrono here)
- 4 unit tests: valid date extraction, no marker returns None, invalid date (2026-13-45) returns None, regex captures on mid-line marker
- 2 dead-code warnings expected (REVIEWED_MARKER_REGEX, extract_reviewed_date) — will resolve when Tasks 1.2-1.5 wire them in
- 1035 lib + 355 bin = 1390 tests passing, zero clippy errors
- No difficulties encountered

### Task 1.2 — Skip stale questions for recently-reviewed facts (commit 4b3cdf0)
- Added `extract_reviewed_date` import and reviewed-marker check in both staleness loops of `generate_stale_questions()`
- Source-date staleness loop: `continue` early if reviewed marker is within `max_age_days` of today (before source ref iteration)
- `@t[~...]` LastSeen loop: same check after retrieving the line but before `FACT_LINE_REGEX` match
- Uses `is_some_and()` for concise Option<NaiveDate> → bool check
- 3 new tests: recent marker suppresses source staleness, recent marker suppresses `@t[~]` staleness, old marker (2020-01-01) still generates questions
- 977 lib + 348 bin = 1325 tests passing, zero clippy warnings
- No difficulties encountered
- Added `REVIEWED_MARKER_REGEX` static `LazyLock<Regex>` to `patterns.rs` matching `<!-- reviewed:(\d{4}-\d{2}-\d{2}) -->` with date capture group
- Added `extract_reviewed_date(line: &str) -> Option<chrono::NaiveDate>` helper that combines regex capture + `NaiveDate::parse_from_str` validation
- Used `chrono::NaiveDate` qualified path to avoid adding a top-level import (patterns.rs only uses chrono here)
- 4 unit tests: valid date extraction, no marker returns None, invalid date (2026-13-45) returns None, regex captures on mid-line marker
- 2 dead-code warnings expected (REVIEWED_MARKER_REGEX, extract_reviewed_date) — will resolve when Tasks 1.2-1.5 wire them in
- 1035 lib + 355 bin = 1390 tests passing, zero clippy errors
- No difficulties encountered

### Task 1.3 — Skip reviewed facts in temporal, missing, and ambiguous generators (commit dda67ef)
- Added `extract_reviewed_date` import and `REVIEWED_SKIP_DAYS` constant (180 days) to all three generators
- `temporal.rs`: `continue` early before both missing-tag and stale-ongoing checks if reviewed marker is recent
- `missing.rs`: added `is_none_or()` filter in the functional chain (clippy suggested simplification from `!is_some_and()`)
- `ambiguous.rs`: `continue` early before ambiguity detection; changed destructuring from `(line_number, _, fact_text)` to `(line_number, line, fact_text)` to access the line
- Used 180-day constant (matching `has_recent_verification` threshold) since these generators don't have a configurable `max_age_days` parameter
- 7 new tests: 3 for temporal (missing-tag suppression, old marker pass-through, stale-ongoing suppression), 2 for missing, 2 for ambiguous
- 984 lib + 348 bin = 1332 tests passing, zero clippy warnings
- No difficulties encountered

### Task 1.4 — Stamp reviewed markers after apply (commit 642ee90)
- Added `add_or_update_reviewed_marker(line, date)` to `patterns.rs`: if marker exists, replaces date via regex; otherwise appends `<!-- reviewed:YYYY-MM-DD -->` to line
- Added `stamp_reviewed_markers(section, date)` in `apply.rs`: stamps all list-item lines (`- ...`) in a section — used after LLM rewrite since line numbers may shift
- Added `stamp_reviewed_lines(content, line_numbers, date)` in `apply.rs`: stamps specific 1-based line numbers (only if they're list items) — for future use in dismissed-question path (Task 1.5)
- In `apply_one_document()`: after `apply_changes_to_section()` returns the rewritten section, calls `stamp_reviewed_markers()` with today's date before `replace_section()` — all fact lines in the rewritten section get stamped since the entire section was reviewed
- Re-exported both stamp functions through `mod.rs` and `lib.rs`
- 8 new tests: 3 for `add_or_update_reviewed_marker` (new marker, update existing, no existing tags), 3 for `stamp_reviewed_markers` (stamps list items, updates existing, empty section), 2 for `stamp_reviewed_lines` (specific lines, skips non-list items)
- 992 lib + 348 bin = 1340 tests passing, zero clippy warnings
- No difficulties encountered — chose to stamp all fact lines in the rewritten section rather than tracking individual line_refs through LLM rewrite (line numbers shift unpredictably)

### Task 1.5 — Stamp reviewed markers on dismissed questions (commit 9ec6ec6)
- Modified `apply_one_document()` in `apply_all.rs` to stamp reviewed markers when questions are dismissed
- All-dismissed path: collects `line_ref` values from dismissed questions, calls `stamp_reviewed_lines()` on content before `remove_processed_questions()` — ensures fact lines are marked as human-reviewed
- Mixed batch path: after section rewrite + `stamp_reviewed_markers()`, collects `line_ref` values from dismissed questions (filtered by `ChangeInstruction::Dismiss`) and stamps those specific lines via `stamp_reviewed_lines()`
- Added `stamp_reviewed_lines` to the import list in `apply_all.rs`
- Implementation at call site rather than inside `remove_processed_questions()` — cleaner separation since that function doesn't know about line_refs or dates
- 992 lib + 348 bin = 1340 tests passing, zero clippy warnings
- No difficulties encountered

### Task 1.6 — Unit tests for reviewed-fact markers (already complete)
- All tests requested by 1.6 were already written as part of tasks 1.1-1.5
- 4 tests in `patterns.rs` for `extract_reviewed_date` (valid, no marker, invalid date, mid-line capture)
- 3 tests in `stale.rs` (recent marker suppresses source staleness, suppresses @t[~] staleness, old marker still generates)
- 10 additional tests across temporal.rs, missing.rs, ambiguous.rs, and apply.rs for reviewed markers
- Total: 17 reviewed-marker tests, all passing
- No new code needed — marked complete

### Task 2.1 — AnswerType enum and classify_answer function (commit fff0484)
- Added `AnswerType` enum to `answer_processor/mod.rs` with 6 variants: `Dismissal`, `Deferral`, `SourceCitation { source, date }`, `Confirmation`, `Correction { detail }`, `Deletion`
- Added `classify_answer(answer: &str) -> AnswerType` to `interpret.rs` with priority-ordered pattern matching
- Classification order: Dismissal → Deletion → Deferral → Correction (explicit prefix) → Confirmation → SourceCitation (prefix keywords) → SourceCitation (date heuristic) → Correction (fallback)
- Source citation detection: `SOURCE_PREFIXES` ("per ", "via ", "from ", "source:") and date-pattern heuristic using `DATE_EXTRACT_REGEX`
- `has_correction_indicators()` helper prevents "No, left March 2024" from being classified as SourceCitation
- Confirmation uses `CONFIRMATION_EXACT` array plus "yes" prefix with short-answer heuristic (<30 chars)
- Correction prefix strips "correct:"/"correction:" and preserves original casing in detail
- Re-exported `AnswerType` and `classify_answer` through `mod.rs` and `lib.rs`
- 992 lib + 348 bin = 1340 tests passing, zero clippy warnings
- No difficulties encountered

### Task 2.2 — Refactor interpret_answer to use classify_answer (commit c8194bb)
- Refactored `interpret_answer()` to call `classify_answer()` first, then match on `AnswerType` to produce `ChangeInstruction`
- Added `ChangeInstruction::Defer` variant for deferral answers — handled alongside `Dismiss` in `format_changes_for_llm`, `apply_changes_to_section`, and `apply_one_document`
- Mapping: Dismissal→Dismiss, Deferral→Defer, Deletion→Delete, SourceCitation→AddSource (source_info combines source+date), Confirmation→UpdateTemporal(@t[~today]) or AddTemporal if no existing tag, Correction→split:/date extraction/Generic fallback
- `apply_one_document` treats Defer same as Dismiss for the all-dismissed fast path and reviewed-marker stamping
- Preserved existing "split:" handling within Correction branch — classify_answer returns Correction for "split:" answers, interpret_answer checks the detail prefix
- 6 new tests: deferral, needs-deferral, source citation, source with date, confirmation with existing tag, confirmation without tag
- 998 lib + 348 bin = 1346 tests passing, zero clippy warnings
- No difficulties encountered

### Task 2.3 — Defer handling: uncheck checkboxes, keep questions in queue (commit 0163c8c)
- Added `uncheck_deferred_questions(content, deferred_indices)` in `apply.rs`: iterates review queue, converts `- [x]` → `- [ ]` for deferred indices, strips answer `>` lines
- Refactored `apply_one_document()` to partition questions into dismissed vs deferred indices
- Dismissed questions: removed from queue via `remove_processed_questions()`, fact lines get reviewed markers
- Deferred questions: unchecked via `uncheck_deferred_questions()`, kept in queue, NO reviewed markers (they need to come back)
- Order matters: uncheck deferred first, then remove dismissed — both functions use original queue indices
- Both the "all no-changes" fast path and the mixed-batch path handle the partition correctly
- Re-exported `uncheck_deferred_questions` through `mod.rs` and `lib.rs`
- 4 new tests: single uncheck, preserves others, empty indices no-op, no marker no-op
- 1002 lib + 348 bin = 1350 tests passing, zero clippy warnings
- No difficulties encountered

### Task 2.4 — Deterministic source citation footnote addition (commit 22f7a8a)
- Added `apply_source_citations(content, sources)` to `apply.rs`: takes `&[(&str, &str)]` pairs of (line_text, source_info)
- Finds max existing footnote number via `SOURCE_DEF_REGEX`, assigns sequential numbers starting from max+1
- Appends `[^N]` to matching fact lines — inserts before `<!-- reviewed:... -->` marker if present, otherwise appends to end
- Footnote definitions placed after last existing `[^N]:` definition, or before `<!-- factbase:review -->` marker, or at end with `---` separator
- Wired into `apply_one_document`: when all active instructions are `AddSource` (+ `Delete`), bypasses LLM rewrite entirely
- Deterministic path also handles reviewed-marker stamping, deferred unchecking, and question removal — same post-processing as LLM path
- Re-exported through `mod.rs` and `lib.rs`
- 7 new tests: new footnote without existing, append after existing, multiple citations, before reviewed marker, before review queue, empty sources no-op, line not found no-op
- 1009 lib + 348 bin = 1357 tests passing, zero clippy warnings
- No difficulties encountered

### Task 2.5 — Deterministic confirmation @t[~] date update (commit 6b30177)
- Added `apply_confirmations(content, updates)` to `apply.rs`: takes `&[(&str, Option<&str>, &str)]` triples of (line_text, old_tag, new_tag)
- `UpdateTemporal` (old_tag is Some): replaces first occurrence of old_tag with new_tag on the matching line
- `AddTemporal` (old_tag is None): inserts new_tag before `<!-- reviewed:... -->` marker, before `[^N]` footnote refs, or at end of line
- Expanded `all_deterministic` check in `apply_one_document` to include `UpdateTemporal` and `AddTemporal` alongside `AddSource` and `Delete`
- Deterministic path builds `confirmation_updates` vec from active instructions and calls `apply_confirmations` after `apply_source_citations`
- Re-exported through `mod.rs` and `lib.rs`
- 7 new tests: update existing tag, add new tag, before footnote, before reviewed marker, empty no-op, line not found no-op, multiple updates
- 1016 lib + 348 bin = 1364 tests passing, zero clippy warnings
- No difficulties encountered

### Task 2.6 — Unit tests for classify_answer patterns (commit 40ca63a)
- Added 10 direct `classify_answer()` tests covering all 6 answer types
- Dismissal: exact match "dismiss"/"ignore", case-insensitive, whitespace-trimmed
- Deletion: exact match "delete"/"remove"
- Deferral: "defer", "later", "needs ...", "check later", "defer ..." prefix
- Confirmation: all 8 CONFIRMATION_EXACT values, "yes" prefix with short answer (<30 chars)
- SourceCitation with prefixes: "per annual report", "via LinkedIn", "from internal wiki", "source: ..."
- SourceCitation with date heuristic: "LinkedIn, 2026-01", "Phonetool lookup 2026-02-10", "per LinkedIn profile, 2026-01"
- Correction explicit prefix: "correct: ..." and "correction: ..." with detail preservation
- Correction indicators prevent source misclassification: "No, left March 2024", "Actually changed...", "No, moved..."
- Correction fallback: long free-text without matching patterns
- 1026 lib + 348 bin = 1374 tests passing, zero clippy warnings
- No difficulties encountered

### Task 3 — Review section cleanup (commit b3309f8)
- Added `normalize_review_section(content: &str) -> String` to `processor/review.rs` with 4 helper functions:
  - `find_review_section_start()`: locates `## Review Queue` heading + `---` separator, handles duplicate headers in backwards scan
  - `strip_orphaned_markers()`: removes inline `@q[...]` markers from document body (outside review section) via `INLINE_QUESTION_MARKER` regex
  - `dedup_review_headers()`: keeps only the first `## Review Queue` heading, drops duplicates
  - `strip_orphaned_blockquotes()`: removes empty `>` lines not immediately following a question line
- Added `INLINE_QUESTION_MARKER` static regex to `patterns.rs`: matches `` `@q[type]` `` with optional backticks
- Wired `normalize_review_section()` into all 3 write paths in `apply_one_document()` (dismiss/defer, deterministic, LLM rewrite)
- Wired into `append_review_questions()` — called on result before returning
- Improved `remove_processed_questions()`: when all questions removed, now strips `## Review Queue` heading and `---` separator (previously left them behind)
- Re-exported through `processor/mod.rs` and `lib.rs`
- 7 new tests: 6 for normalize (duplicate headers, orphaned markers, orphaned blockquotes, empty section removal, no-section orphan strip, valid section preservation) + 1 for improved remove_processed_questions
- Initial bug: `find_review_section_start` didn't scan past duplicate `## Review Queue` headers when looking backwards — fixed by including `## Review Queue` in the backwards-scan loop condition
- 1033 lib + 348 bin = 1381 tests passing, zero clippy warnings
- No other difficulties encountered

### Task 4 — Lint net-new reporting (commits d4e9933, e4ac1aa)
- Expanded `LintDocResult` with 4 new fields: `new_questions` (renamed from `questions_added`), `existing_unanswered`, `existing_answered`, `skipped_reviewed`
- In `lint_all_documents()`: parse existing review queue to count answered/unanswered questions, count fact lines with recent reviewed markers (within 180 days via `REVIEWED_SKIP_DAYS` constant), track all counts through the async block tuple
- Results now include docs with existing questions or skipped facts even if no new questions generated (previously only included docs with new questions)
- MCP `lint_repository` response now includes: `new_unanswered`, `already_in_queue`, `skipped_reviewed`, `total_questions_generated` (= new + existing), `details` filtered to docs with new questions only
- CLI `cmd_lint --review` prints summary: "Review: Generated N total, M new (X already in queue, Y skipped as recently reviewed)"
- Added `count_reviewed_facts(content)` helper in `commands/lint/review.rs` — counts fact lines with reviewed markers within 180 days
- Changed `generate_review_questions()` return type from `Option<ExportedDocQuestions>` to `(usize, Option<ExportedDocQuestions>)` to expose new question count
- Promoted `extract_reviewed_date` and `FACT_LINE_REGEX` from `pub(crate)` to `pub` and re-exported through `lib.rs` for binary crate access
- 3 lib tests: existing_unanswered count, existing_answered count, skipped_reviewed count
- 3 bin tests: count_reviewed_facts with recent markers, old markers, no markers
- 1036 lib + 351 bin = 1387 tests passing, zero clippy warnings
- No difficulties encountered

### Task 5.1 — Add source_refs to FactLine for footnote extraction (commit b1568da)
- Added `source_refs: Vec<u32>` field to `FactLine` struct — tracks which `[^N]` footnote references appear on each fact line
- In `extract_all_facts()`: uses `SOURCE_REF_CAPTURE_REGEX.captures_iter(line)` on the raw line (before cleaning) to extract all footnote numbers
- Updated 5 test `FactLine` constructions in `cross_validate.rs` to include `source_refs: vec![]`
- 4 new tests: single ref, multiple refs, no refs, refs with temporal tags
- 1040 lib + 351 bin = 1391 tests passing, zero clippy warnings
- No difficulties encountered

### Task 5.2 — Attach source definitions to FactWithContext (commit cc4c8fe)
- Added `source_defs: Vec<String>` field to `FactWithContext` struct — holds formatted source footnote definitions referenced by each fact
- In `cross_validate_document()`: calls `parse_source_definitions(content)` to get all `SourceDefinition` entries, builds `HashMap<u32, String>` mapping footnote number → formatted definition text
- For each fact, resolves `source_refs` against the map via `filter_map` and attaches matching definitions as `source_defs`
- Source definition text format: `[^N]: source_type context, date` (mirrors original footnote format)
- Updated 5 existing test `FactWithContext` constructions to include `source_defs: vec![]`
- 4 new tests: source map construction from parsed definitions, ref-to-def lookup with single ref, empty refs produce empty defs, multiple refs resolve correctly
- 1044 lib + 351 bin = 1395 tests passing, zero clippy errors
- One expected dead_code warning for `source_defs` field — will be consumed by Task 5.3 when `build_prompt` uses it
- No difficulties encountered

### Task 5.3 — Source context in cross-validation prompts (commit 87ce4f9)- Modified `build_prompt()` to include "Sources for this fact:" line when `FactWithContext.source_defs` is non-empty — semicolon-separated list of source definitions
- Added entity role distinction instruction to prompt preamble: distinguishes SUBJECT entities from SOURCE entities, instructs LLM that a source person becoming inactive does NOT make the fact stale
- Source context appears after related information for each fact, before the blank line separator
- 3 new tests: source context included in prompt, omitted when empty, entity role instruction present
- 1047 lib + 351 bin = 1398 tests passing, zero clippy warnings
- `source_defs` dead_code warning from Task 5.2 now resolved — field is consumed by `build_prompt`
- No difficulties encountered

### Task 5.4 — Unit tests for prompt source context and integration test (commit cae666e)
- Added `test_build_prompt_multiple_source_defs_semicolon_separated`: verifies multiple source definitions are joined with semicolons in prompt output
- Added `test_build_prompt_source_context_after_related_info`: verifies source context appears after related document info (positional ordering assertion)
- Added `test_cross_validate_product_fact_with_source_not_flagged_stale`: integration-style test using MockLlm returning CONSISTENT for a product fact sourced from a person — inserts a person document with embedding into test DB, runs `cross_validate_document` on product content with `[^1]` footnote referencing the person, verifies no stale questions generated
- Added `test_repo_in_db` and `Document` imports to test module for integration test setup
- 3 new tests, 1050 lib + 351 bin = 1401 tests passing, zero clippy warnings
- No difficulties encountered — MockEmbedding returns constant vectors (all 0.1) which produces similarity 1.0 between all documents, ensuring the person document appears as a related result above the 0.3 threshold

### Task 6 — Deferred item surfacing (commits 390f111, 50b2fe3)
- **6.1**: Added `get_deferred_items` MCP tool in `review/queue.rs` — delegates to `get_review_queue` with `status: "deferred"`, reshapes response to `{ deferred_items, total_deferred, summary }`. Summary uses human-friendly phrasing ("N items need human attention"). Registered in tool schema (tool count 19→20), routing, and dispatch consistency test. 2 unit tests with test DB.
- **6.2**: `resolve_step` in `workflow.rs` now accepts `deferred: usize` parameter. `workflow()` calls `count_deferred()` helper before dispatching. Step 1 includes `deferred_count` field and appends instruction note when deferred > 0 directing agent to call `get_deferred_items` first. 2 new tests for deferred/no-deferred cases.
- **6.3**: `lint_repository` MCP response now includes `deferred_count` field. Counts deferred items from the already-loaded docs by filtering `parse_review_queue` results for `!answered && answer.is_some()`.
- **6.4**: CLI `review --status` now shows "Deferred: N" row between Answered and Unanswered. `ReviewStatusJson` gains `deferred` field. Unanswered count adjusted to `total - answered - deferred`. Updated serialization tests.
- **6.5**: All tests covered by 6.1 and 6.2 — `get_deferred_items` returns only deferred (test_get_deferred_items_returns_only_deferred), empty when none (test_get_deferred_items_empty_when_none), workflow includes count (test_resolve_step1_includes_deferred_note), workflow omits when zero (test_resolve_step1_no_deferred_note_when_zero).
- Difficulty: `REVIEW_QUESTION_REGEX` requires backtick-wrapped `@q[type]` markers — initial test content used bare markers, causing `parse_review_queue` to return empty. Fixed by using `` `@q[type]` `` format in test content.
- 1054 lib + 351 bin = 1405 tests passing, zero clippy warnings
