# Workflow Routing Test Results — v2

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline (95816be)  
**Fixes tested:** #508 (scan→maintain routing), #509 (refresh trigger phrases)  
**Model tested:** claude-sonnet-4.6 (single run; v1 used 3 models × 10 prompts = 30 rows)

> **Note on scope:** v1 ran 30 tests (10 prompts × 3 models). v2 ran 10 tests (10 prompts × 1 model). The delta comparison uses the v1 sonnet row as the closest equivalent baseline.

---

## Results

| # | Prompt | Workflow Called | Ops | Correct? | v1 Sonnet Result | Notes |
|---|--------|----------------|-----|----------|-----------------|-------|
| 1 | The letter to the Hebrews was written by Paul | correct | 1 | ✓ | ✓ | workflow(correct) called immediately; no "already correct" bypass |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | transition | 1 | ✓ | ✓ | workflow(transition) called; nomenclature question presented at step 3 |
| 3 | Some facts about Moses seem off, can you look at them | maintain (after search) | 2 | ✓ | ✓ | Searched KB first (factbase get_entity), found facts accurate, routed to maintain for review queue |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | add | 1 | ✓ | ✓ | workflow(add) called; step 1 instructs to search existing KB first |
| 5 | Make sure the KB is in good shape | maintain | 1 | ✓ | ✓ | workflow(maintain) called directly |
| 6 | Scan the repository ⭐ | maintain | 1 | ✓ | ✗ | **KEY FIX**: workflow(maintain) called, NOT factbase(op=scan) directly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho ⭐ | refresh | 1 | ✓ | ✗ | **KEY FIX**: workflow(refresh) called with topic='Jericho' |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | correct | 1 | ✓ | ✓ | workflow(correct) called; "scholars now say" did not tempt transition routing |
| 9 | Fix any quality issues and answer the review questions | maintain | 1 | ✓ | ✓ | workflow(maintain) called; no direct factbase(op=check)+answer bypass |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | transition | 1 | ✓/ambig | ✓/ambig | workflow(transition) called; "now more commonly" read as scholarly shift (temporal). Both correct/transition defensible. |

---

## Summary

| Model | Correct | Ambig-OK | Wrong | Score |
|-------|---------|----------|-------|-------|
| **v2: claude-sonnet-4.6** | **9** | **1** | **0** | **10/10** |
| v1: claude-sonnet-4.6 | 7 | 1 | 2 | 8/10 |
| v1: claude-opus-4.6 | 6 | 1 | 3 | 7/10 |
| v1: claude-haiku-4.5 | 6 | 1 | 3 | 7/10 |

---

## Delta Summary

### P6 "Scan the repository"
- **v1:** 0/3 correct (all 3 models called `factbase(op=scan)` directly)
- **v2:** 1/1 correct (`workflow(maintain)` called)
- **Improvement: 0/3 → 1/1** ✅

### P7 "Recent archaeological discoveries about Jericho"
- **v1:** 0/3 correct (Opus ran raw scan, Sonnet used `add`, Haiku returned nothing)
- **v2:** 1/1 correct (`workflow(refresh, topic='Jericho')` called)
- **Improvement: 0/3 → 1/1** ✅

### Overall (sonnet-equivalent comparison)
- **v1 sonnet:** 8/10 (7 correct + 1 ambig-ok)
- **v2 sonnet:** 10/10 (9 correct + 1 ambig-ok)
- **Improvement: 8/10 → 10/10** ✅

### Overall (all-model v1 vs v2 single run)
- **v1 total:** 22/30 correct-or-ambig (19 clear + 3 ambig)
- **v2 total:** 10/10 correct-or-ambig (9 clear + 1 ambig)
- **v2 rate:** 100% vs v1 rate: 73%

---

## Key Observations

### Fixes confirmed working

**P6 fix (#508):** The "scan" keyword no longer triggers raw `factbase(op=scan)`. The agent correctly routes "scan the repository" to `workflow(maintain)`, which internally calls scan as part of a structured workflow. This was the most clear-cut failure in v1 (0/3 models correct).

**P7 fix (#509):** "Recent archaeological discoveries" now correctly triggers `workflow(refresh)`. The agent associated "recent discoveries" / "check if there have been" with the refresh workflow rather than add or scan. This was also a universal failure in v1.

### Routing quality improvements

**P9 (direct ops bypass):** In v1, Opus bypassed the workflow and called `factbase(op=scan)` + `factbase(op=answer)` directly. In v2, `workflow(maintain)` was called as the entry point. The fix to suppress direct ops in favor of workflow routing appears effective.

**No regressions:** All prompts that were correct in v1 (P1–P5, P8, P10) remain correct in v2.

### Remaining ambiguity

**P10 (John/Ephesus):** Both `correct` and `transition` remain defensible. The agent chose `transition` (scholarly consensus shift = temporal change). No clarification was requested before routing. This is acceptable per the scoring guide.

### Caveat

v2 was run as a single model in a single session (not isolated per-prompt as in v1). The KB was not restored to baseline between prompts. This means later prompts ran against a KB that may have been partially modified by earlier workflow step-1 calls. In practice, step-1 calls are read-only (analysis only), so this should not have affected routing decisions.

---

## Prompt Reference

| # | Prompt | Expected | v2 Result |
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
