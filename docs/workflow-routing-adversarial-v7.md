# Workflow Routing Test Results — Adversarial v7 (Sonnet)

**Date:** 2026-03-12  
**KB:** bible-facts  
**Model:** claude-sonnet-4-6  
**Purpose:** Adversarial prompts designed to cause model confusion via keyword mismatch, compound requests, and vague framing  
**Baseline:** v6 = 10/10

---

## Design Principles (v7 — adversarial)

- Keyword mismatch: 'correct' meaning fix/update (P2), 'add' meaning correct (P3), 'refresh' meaning correct (P5)
- Compound two-step requests (P8)
- Extremely vague requests (P7, P10)
- Structural/config changes vs content changes (P6, P9)
- Ambiguous correct vs transition (P4)

---

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | "Can you scan the repository for quality issues?" | `workflow(maintain, step=1)` | ✓ | 'scan' → maintain rule fires; 'quality issues' reinforces it |
| 2 | "Correct the index — some entries are out of date" | `workflow(maintain, step=1)` | ✓ | 'correct' as verb ≠ workflow(correct); "out of date" = stale, not always-wrong → maintain |
| 3 | "Add a note that the Epistle of James authorship is disputed" | `workflow(add, step=1, topic='Epistle of James authorship')` | ✗ | **MISROUTED** — 'add a note' triggers workflow(add); should be workflow(correct) since KB has definitive (wrong) attribution |
| 4 | "The Council of Jerusalem took place in 49 AD — but our KB says 50 AD. Please fix." | `workflow(correct, step=1, correction='Council of Jerusalem date is 49 AD, not 50 AD as recorded')` | ✓ | Clear factual error — date was never 50 AD; correct immediately per routing rules |
| 5 | "Refresh our records on the Psalms — some of the authorship attributions are outdated" | `workflow(refresh, step=1, topic='Psalms authorship attributions')` | ✓ | 'refresh' keyword + "outdated" → refresh; either refresh or correct defensible per spec |
| 6 | "I'd like to start tracking a new entity type: church councils. Can you set that up?" | `workflow(create, step=1)` | ✓ | New entity type = KB domain expansion → workflow(create) |
| 7 | "Everything about the Apostle Peter seems wrong" | `workflow(maintain, step=1)` | ✓ | Extremely vague — "seems wrong" doesn't justify correct/transition; investigate first |
| 8 | "Scan for and then fix all the places where we say Paul wrote Hebrews" | `workflow(correct, step=1, correction='Paul did not write Hebrews; authorship is disputed/pseudonymous')` | ✓ | Compound request collapsed to single workflow(correct); correction is fully specified in prompt |
| 9 | "I need to update the KB — we've merged our notes on Acts with Luke into a single document" | `factbase(op=organize, action=merge)` | ✓ | Merge already happened — recording structural change; organize(merge) or workflow(transition) both valid |
| 10 | "Fix the review queue" | `workflow(maintain, step=1)` | ✓ | Vague maintenance request; review queue = resolve list → maintain as catch-all |

---

## Score

**9/10** — 1 misrouting

---

## Failure Analysis

### P3 — 'add' keyword misleads into workflow(add) ✗

**Prompt:** "Add a note that the Epistle of James authorship is disputed"  
**Called:** `workflow(add, step=1, topic='Epistle of James authorship')`  
**Should have called:** `workflow(correct, step=1)`

**Why it failed:** "Add a note" is a natural-language phrase for "record this fact", but the routing system interprets 'add' as workflow(add) = research and CREATE new documents. The actual intent is to correct existing content that makes a definitive (wrong) authorship claim. The 'add' keyword overrode the semantic content.

**Fix signal:** If the prompt had said "our KB incorrectly states James wrote it" or "fix the authorship entry", workflow(correct) would have fired. The phrase "add a note" obscures that this is a correction.

---

## Key Findings

| Signal | Behavior | Verdict |
|--------|----------|---------|
| P2: 'correct' as verb (not workflow name) | Correctly routed to maintain | ✓ Robust |
| P3: 'add' keyword masking a correction | Misrouted to workflow(add) | ✗ Failure |
| P5: 'refresh' keyword with correction semantics | Routed to refresh (defensible) | ✓ Acceptable |
| P8: compound scan+fix request | Collapsed to single workflow(correct) | ✓ Robust |
| P9: 'update'+'merged' structural change | Routed to organize(merge) | ✓ Robust |
| P10: vague "fix the review queue" | Routed to maintain | ✓ Robust |

---

## Cumulative Sonnet History

| Run | Correct | Wrong | Score |
|-----|---------|-------|-------|
| v7 adversarial (this run) | 9 | 1 | 9/10 |
| v6 | 10 | 0 | 10/10 |
| v5 | 10 | 0 | 10/10 |
| v4 | 5 | 5 | 5/10 |
| v3 | 9 | 1 | 9/10 |
| v2 | 9 | 1 | 9/10 |

---

## Recommendation

The 'add' keyword is a reliable misrouting trigger when the semantic intent is a correction. Consider adding a routing rule: if the prompt contains 'add' but the content describes an existing KB entry being wrong/disputed/incorrect, prefer workflow(correct) over workflow(add).
