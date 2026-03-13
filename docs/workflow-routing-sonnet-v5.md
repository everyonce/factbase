# Workflow Routing Test Results — Sonnet v5

**Date:** 2026-03-12  
**KB:** bible-facts  
**Model:** claude-sonnet-4.6  
**Fixes tested:** #516 (call workflow immediately) + #517 (factbase-first priority)  
**Purpose:** Verify routing improvements post-#516/#517 vs v4 baseline (5/10)

---

## Results

| # | Prompt | First Call | Correct? | v4 Result | Notes |
|---|--------|-----------|----------|-----------|-------|
| 1 | The letter to the Hebrews was written by Paul | `workflow(correct, step=1)` | ✓ | ✗ (no tool call) | Fix #516: called immediately instead of answering from training data |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `workflow(transition, step=1)` | ✓ | ✓ | Already correct in v4; still correct |
| 3 | Some facts about Moses seem off, can you look at them | `factbase(op=list, title_filter="Moses")` | ✓ | ✓ | Vague prompt — lookup-first acceptable; still correct |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add, topic="Council of Nicaea...", step=1)` | ✓ | ✓ | Already correct in v4; still correct |
| 5 | Make sure the KB is in good shape | `workflow(maintain, step=1)` | ✓ | ✓ | Already correct in v4; still correct |
| 6 | Scan the repository | `workflow(maintain, step=1)` | ✓ | ✓ | Already correct in v4; scan→maintain routing holds |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | `workflow(refresh, topic="Jericho", step=1)` | ✓ | ✗ (web_search) | Fix #517: factbase-first; fix #516: workflow immediately instead of web_search |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `workflow(correct, step=1)` | ✓ | ✗ (think → answered from knowledge) | Fix #516: called immediately; no internal reasoning detour |
| 9 | Fix any quality issues and answer the review questions | `workflow(maintain, step=1)` | ✓ | ✗ (factbase(op=check) directly) | Correct entry point now; no bypassing maintain workflow |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `workflow(transition, step=1)` | ✓ | ✗ (aim_memory_search) | Fix #517: factbase-first; no memory MCP detour |

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-sonnet-4.6** | **v5 (this run)** | **10** | **0** | **10/10** |
| claude-sonnet-4.6 | v4 | 5 | 5 | 5/10 |
| claude-opus-4.6 | v4 | 5 | 5 | 5/10 |
| claude-sonnet-4.6 | v3 | 9 | 1 | 9/10 (1 ambig) |
| claude-sonnet-4.6 | v2 | 9 | 1 | 9/10 (1 ambig) |
| claude-opus-4.6 | v1 | 6 | 3 | 6/10 (1 ambig) |
| claude-haiku-4.5 | v1 | 6 | 3 | 6/10 (1 ambig) |

---

## Analysis

### Improvement vs v4: +5 (5/10 → 10/10)

All five v4 failures are resolved. The fixes map cleanly to the failure patterns:

**Fix #516 resolved P1, P8 (answered from own knowledge)**  
The "call workflow immediately" rule prevents the model from reasoning internally about factual claims and answering without touching the KB. Both correction prompts now route to `workflow(correct)` as the first action.

**Fix #516 resolved P7 (web_search instead of workflow)**  
"Recent discoveries" phrasing previously triggered a web search. The rule that refresh prompts must call `workflow(refresh)` immediately — before any external search — closes this gap.

**Fix #517 resolved P7, P10 (wrong tool entirely)**  
The factbase-first priority rule prevents routing to `web_search` (P7) or `aim_memory_search` (P10). When a factbase is configured, all lookups go through factbase tools first.

**Routing rule resolved P9 (correct destination, wrong entry point)**  
`workflow(maintain)` is now the required entry point for quality/review tasks, not `factbase(op=check)` directly.

### What Was Already Working (v4 → v5 unchanged)

P2 (transition), P3 (vague lookup), P4 (add), P5 (maintain), P6 (scan→maintain) all routed correctly in both v4 and v5.

### Exceeds v2/v3 Baseline

v5 scores 10/10 vs 9/10 in v2/v3. The one ambiguous case from v2/v3 (likely P9 or P10) is now unambiguously correct.
