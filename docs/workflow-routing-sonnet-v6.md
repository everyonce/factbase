# Workflow Routing Test Results — Sonnet v6

**Date:** 2026-03-12  
**KB:** bible-facts  
**Model:** claude-sonnet-4-6  
**Purpose:** Harder routing prompts — v6 design (more ambiguous framing, mixed signals, edge cases)  
**Baseline:** v5 = 10/10

---

## Prompt Design (v6 — harder than v5)

Design principles applied:
- More ambiguous framing (P1, P9: vague "mistakes" / "accurate and complete")
- Could be misread as wrong workflow (P7: "updating" sounds like add, is refresh; P6: reorganize)
- Test read-only vs write routing (P5, P10: user asks what KB says — no changes)
- Test correct vs maintain when user is uncertain (P1, P9)
- Test organize as a distinct routing target (P6)

---

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | "I think there are some mistakes in how we've recorded the early church" | `workflow(maintain, step=1)` | ✓ | Vague — investigate first, don't assume correction |
| 2 | "The apostle Paul didn't write Ephesians — modern scholars attribute it to a student of Paul" | `workflow(correct, step=1)` | ✓ | Clear factual correction — authorship was always wrong |
| 3 | "Refresh the KB with the latest Dead Sea Scrolls scholarship" | `workflow(refresh, step=1, topic='Dead Sea Scrolls scholarship')` | ✓ | Explicit refresh keyword |
| 4 | "We need to update our records — the Gospel of Mark was actually written AFTER Luke, not before" | `workflow(correct, step=1)` | ✓ | Factual correction — ordering was always wrong |
| 5 | "Can you help me understand what the KB says about baptism?" | `search(query='baptism')` | ✓ | Read-only — user wants to understand, not change |
| 6 | "I want to reorganize the KB so that all epistles are grouped together" | `factbase(op=organize, action=analyze, focus=structure)` | ✓ | Restructure request → organize |
| 7 | "The KB needs updating — there's been a lot of new work on the historical Paul recently" | `workflow(refresh, step=1, topic='historical Paul')` | ✓ | "New work recently" = external scholarship → refresh (not add) |
| 8 | "I think we should correct the record on the Synoptic Problem — our KB has it wrong" | `workflow(correct, step=1)` | ✓ | Explicit "has it wrong" → correct immediately |
| 9 | "Can you check whether our Dead Sea Scrolls content is accurate and complete?" | `workflow(maintain, step=1)` | ✓ | Quality/completeness audit → maintain |
| 10 | "The Gospel of John was written by John the Apostle — but I've seen this disputed. What does our KB say?" | `search(query='Gospel of John authorship')` | ✓ | User asking what KB says — read-only, no change requested |

---

## Score

**10/10** ✅

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-sonnet-4-6** | **v6 (this run)** | **10** | **0** | **10/10** |
| claude-sonnet-4-6 | v5 | 10 | 0 | 10/10 |
| claude-sonnet-4-6 | v4 | 5 | 5 | 5/10 |
| claude-sonnet-4-6 | v3 | 9 | 1 | 9/10 |
| claude-sonnet-4-6 | v2 | 9 | 1 | 9/10 |

---

## Analysis

### All 10 harder prompts routed correctly

**Ambiguous "mistakes" (P1):** Correctly routed to `maintain` rather than `correct`. The vague framing ("I think there are some mistakes") doesn't justify jumping to a correction workflow — investigation first is the right call.

**Read-only routing (P5, P10):** Both prompts asking "what does the KB say" correctly routed to `search`. P10 is particularly tricky — it states a fact but then asks what the KB says, making it a read-only query.

**Refresh vs Add (P7):** "New work on the historical Paul recently" correctly identified as `refresh` (update existing content with new external scholarship) rather than `add`. The "recently" signal is key.

**Organize (P6):** Correctly identified as `factbase(op=organize)` rather than `workflow(maintain)`.

**Maintain vs Correct (P9):** "Accurate and complete?" is a quality audit → `maintain`.

### No regressions from v5

All routing patterns established in v5 hold under harder prompts. The fixes from #516 and #517 remain stable.
