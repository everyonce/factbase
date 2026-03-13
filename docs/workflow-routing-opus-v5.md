# Workflow Routing Test — Opus v5

**Date:** 2026-03-12  
**Model:** claude-opus-4.6  
**KB:** /Users/daniel/work/bible-facts  
**Fixes tested:** #516 (call workflow immediately, don't search first), #517 (factbase-first priority over web_search/memory tools)

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | "The letter to the Hebrews was written by Paul" | `workflow(correct, step=1)` | ✅ | Correctly identified as a factual correction, not a search |
| 2 | "The book previously known as Revelation should now be catalogued as Apocalypse of John" | `workflow(transition, step=1)` | ✅ | Correctly identified as a rename/reclassification transition |
| 3 | "Some facts about Moses seem off, can you look at them" | `search("Moses")` | ✅ | Vague prompt — lookup first is correct behavior |
| 4 | "Add research about the Council of Nicaea and how it affected the biblical canon" | `workflow(add, step=1)` | ✅ | Correctly routed to add workflow immediately |
| 5 | "Make sure the KB is in good shape" | `workflow(maintain, step=1)` | ✅ | Correctly routed to maintain |
| 6 | "Scan the repository" | `workflow(maintain, step=1)` | ✅ | "Scan" correctly mapped to maintain (was 0/3 in v1) |
| 7 | "Check if there have been any recent archaeological discoveries related to Jericho" | `workflow(refresh, step=1)` | ✅ | Correctly identified as a refresh (recent/new info) |
| 8 | "The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC" | `workflow(correct, step=1)` | ✅ | Correctly identified as a correction, not a search |
| 9 | "Fix any quality issues and answer the review questions" | `workflow(maintain, step=1)` | ✅ | Correctly routed to maintain, not factbase(check) directly |
| 10 | "Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship" | `workflow(transition, step=1)` | ✅ | Correctly identified as a scholarly consensus shift (transition) |

## Score

**10/10** ✅

## Comparison vs v4 Opus (5/10)

| # | Prompt | v4 Result | v5 Result | Improvement? |
|---|--------|-----------|-----------|--------------|
| 1 | Hebrews/Paul correction | ❌ (searched first) | ✅ workflow(correct) | ✅ Fixed by #516 |
| 2 | Revelation → Apocalypse transition | ❌ (searched first) | ✅ workflow(transition) | ✅ Fixed by #516 |
| 3 | Moses facts seem off | ✅ search | ✅ search | — |
| 4 | Add Council of Nicaea | ✅ workflow(add) | ✅ workflow(add) | — |
| 5 | KB in good shape | ✅ workflow(maintain) | ✅ workflow(maintain) | — |
| 6 | Scan the repository | ❌ (factbase scan directly) | ✅ workflow(maintain) | ✅ Fixed by routing rule |
| 7 | Recent Jericho discoveries | ❌ (web_search first) | ✅ workflow(refresh) | ✅ Fixed by #517 |
| 8 | Exodus dates wrong | ❌ (searched first) | ✅ workflow(correct) | ✅ Fixed by #516 |
| 9 | Fix quality issues | ✅ workflow(maintain) | ✅ workflow(maintain) | — |
| 10 | John/Ephesus update | ❌ (searched first) | ✅ workflow(transition) | ✅ Fixed by #516 |

## Summary

Fixes #516 and #517 resolved all 5 regressions from v4:
- **#516** fixed prompts 1, 2, 8, 10 — model now calls workflow(correct/transition) immediately instead of searching first
- **#516** also fixed prompt 6 — "scan" now routes to workflow(maintain) instead of factbase(scan) directly  
- **#517** fixed prompt 7 — model now calls workflow(refresh) instead of web_search for recent-info queries

v5 achieves a perfect **10/10** routing score.
