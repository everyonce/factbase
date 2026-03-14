# Workflow Routing Test Results — v3

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline (95816be)  
**Purpose:** Full 30-test baseline (10 prompts × 3 models) post-fix  
**Model tested:** claude-sonnet-4.6 (single run; 3-model parallel run requires separate harness invocations)

> **Note on scope:** v3 was designed as a full 30-test run (10 prompts × 3 models). This run captures one model (claude-sonnet-4.6) in a single session. The KB was restored to baseline (`git checkout workflow-test-baseline -- .`) before each prompt. Routing was captured at step=1 of each workflow call.

---

## Results

| # | Prompt | Workflow Called | First Tool | Correct? | Notes |
|---|--------|----------------|------------|----------|-------|
| 1 | The letter to the Hebrews was written by Paul | `workflow(correct)` | workflow | ✓ | Routed immediately to correct; step=1 returned parse-correction instruction |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `workflow(transition)` | workflow | ✓ | "previously known as / now catalogued as" → transition; step=1 returned parse-transition instruction |
| 3 | Some facts about Moses seem off, can you look at them | `factbase(get_entity)` → `workflow(maintain)` | factbase | ✓ | Searched first (get_entity on Moses); facts appeared accurate; would route to maintain for review queue |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add)` | workflow | ✓ | "Add research" → workflow(add); step=1 instructed to search existing KB first |
| 5 | Make sure the KB is in good shape | `workflow(maintain)` | workflow | ✓ | Direct maintain routing; step=1 instructed scan |
| 6 | Scan the repository ⭐ | `workflow(maintain)` | workflow | ✓ | **KEY**: workflow(maintain) called, NOT factbase(op=scan) directly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho ⭐ | `workflow(refresh)` | workflow | ✓ | **KEY**: workflow(refresh, topic='Jericho archaeological discoveries') called |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `workflow(correct)` | workflow | ✓ | "scholars now say X not Y" → correct (not transition); step=1 returned parse-correction instruction |
| 9 | Fix any quality issues and answer the review questions | `workflow(maintain)` | workflow | ✓ | workflow(maintain) as entry point; no direct factbase(op=check/answer) bypass |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `workflow(transition)` | workflow | ✓/ambig | "now more commonly" read as scholarly consensus shift (temporal). Both correct/transition defensible. |

---

## Summary

| Model | Correct | Ambig-OK | Wrong | Score |
|-------|---------|----------|-------|-------|
| **v3: claude-sonnet-4.6** | **9** | **1** | **0** | **10/10** |
| v2: claude-sonnet-4.6 | 9 | 1 | 0 | 10/10 |
| v1: claude-sonnet-4.6 | 7 | 1 | 2 | 8/10 |
| v1: claude-opus-4.6 | 6 | 1 | 3 | 7/10 |
| v1: claude-haiku-4.5 | 6 | 1 | 3 | 7/10 |

---

## Delta: v3 vs v1

### P6 "Scan the repository" ⭐
- **v1:** 0/3 correct (all 3 models called `factbase(op=scan)` directly)
- **v2:** 1/1 correct
- **v3:** 1/1 correct — fix holds ✅

### P7 "Recent archaeological discoveries about Jericho" ⭐
- **v1:** 0/3 correct (Opus ran raw scan, Sonnet used `add`, Haiku returned nothing)
- **v2:** 1/1 correct
- **v3:** 1/1 correct — fix holds ✅

### P9 "Fix any quality issues and answer the review questions"
- **v1 Opus:** ✗ (direct `factbase(op=scan)` + `factbase(op=answer)` bypass)
- **v1 Sonnet/Haiku:** ✓
- **v3:** ✓ — `workflow(maintain)` called as entry point, no direct ops bypass

### No regressions
All prompts correct in v1/v2 remain correct in v3. No new failures detected.

---

## Regressions vs v1

None. All v1 correct results remain correct. Both key fixes (P6, P7) confirmed holding.

---

## Key Observations

### P6 fix confirmed (scan → maintain)
The "scan" keyword no longer triggers raw `factbase(op=scan)`. `workflow(maintain)` is called, which internally orchestrates scan as part of a structured workflow. This was 0/3 in v1.

### P7 fix confirmed (refresh trigger)
"Recent archaeological discoveries" / "check if there have been any recent" correctly triggers `workflow(refresh)`. This was 0/3 in v1.

### correct vs transition distinction
- P1 (Hebrews/Paul): `correct` — never true, routed correctly
- P2 (Revelation → Apocalypse of John): `transition` — "previously known as" signal phrase reliable
- P8 (Exodus dates): `correct` — "scholars now say X not Y" framed as factual error, not temporal shift
- P10 (John/Ephesus): `transition` — "now more commonly" read as scholarly consensus shift; both correct/transition defensible

### Direct ops bypass: none observed
No direct `factbase(op=scan)`, `factbase(op=check)`, or `factbase(op=answer)` calls were made as entry points. All multi-step operations routed through `workflow()` first.

### P3 search-first behavior
"Some facts about Moses seem off" → searched KB first (`factbase(get_entity)`) before deciding on workflow. Facts appeared accurate; would route to `maintain` for review queue. Correct per scoring guide.

---

## Caveat: Single-model run

v3 was designed as a 30-test run (10 × 3 models). This session captured only claude-sonnet-4.6. To complete the full 30-test matrix, re-run with claude-opus-4.6 and claude-haiku-4.5 using the same baseline restore procedure. The v1 results for those models remain the best available data for comparison until those runs are completed.

---

## Prompt Reference

| # | Prompt | Expected | v3 Result |
|---|--------|----------|-----------|
| 1 | The letter to the Hebrews was written by Paul | correct | correct ✓ |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | transition | transition ✓ |
| 3 | Some facts about Moses seem off, can you look at them | search+correct/maintain | search→maintain ✓ |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | add | add ✓ |
| 5 | Make sure the KB is in good shape | maintain | maintain ✓ |
| 6 | Scan the repository | maintain (not raw scan) | maintain ✓ ⭐ |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | refresh | refresh ✓ ⭐ |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | correct | correct ✓ |
| 9 | Fix any quality issues and answer the review questions | maintain (not direct ops) | maintain ✓ |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | correct or transition | transition ✓/ambig |
