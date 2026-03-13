# Workflow Routing Test Results — Haiku v4

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline (95816be)  
**Model:** claude-haiku-4.5  
**Agent:** factbase-only (tools restricted to `@factbase` MCP only)  
**Purpose:** Routing test — does Haiku call the correct workflow/op first?

> Each test: `git checkout workflow-test-baseline -- .` → run prompt → capture first tool call.

---

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | The letter to the Hebrews was written by Paul | `search(query="Hebrews authorship Paul")` | ✗ | Should be `workflow(correct)`; Haiku searched first to find the document, then called `workflow(correct)` — correct destination, wrong entry point |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `workflow(transition, step=1)` | ✓ | Direct correct routing; proceeded through full transition workflow |
| 3 | Some facts about Moses seem off, can you look at them | `search(query="Moses", limit=20)` | ✓ | Lookup-first acceptable for vague "seem off" — fetched entity, summarised review queue, asked user to clarify which facts |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add, topic="Council of Nicaea...", step=1)` | ✓ | Direct correct routing; "Add research" → `workflow(add)` immediately |
| 5 | Make sure the KB is in good shape | `workflow(maintain, step=1)` | ✓ | Direct correct routing; proceeded through full maintain sequence |
| 6 | Scan the repository ⭐ | `workflow(maintain, step=1)` | ✓ | **KEY**: Did NOT call `factbase(op=scan)` directly; routed through maintain workflow correctly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | `workflow(refresh, topic="Jericho archaeological discoveries", step=1)` | ✓ | Direct correct routing; "recent discoveries" → `workflow(refresh)` immediately |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `search(query="Exodus date 1200 BC")` | ✗ | Should be `workflow(correct)`; Haiku searched first to verify the claim, then called `workflow(correct)` — correct destination, wrong entry point |
| 9 | Fix any quality issues and answer the review questions | `workflow(maintain, step=1)` | ✓ | Direct correct routing; proceeded through full maintain sequence including check and review_queue |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `search(query="John", limit=5)` | ✗ | Should be `workflow(transition)`; Haiku searched first to find the document, then called `workflow(transition)` — correct destination, wrong entry point |

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-haiku-4.5** | **v4 (this run)** | **7** | **3** | **7/10** |
| claude-sonnet-4.6 | v4 | 5 | 5 | 5/10 |
| claude-opus-4.6 | v4 | 5 | 5 | 5/10 |
| claude-sonnet-4.6 | v3 | 9 | 1 | 9/10 (1 ambig) |
| claude-sonnet-4.6 | v2 | 9 | 1 | 9/10 (1 ambig) |
| claude-opus-4.6 | v1 | 6 | 3 | 6/10 (1 ambig) |
| claude-haiku-4.5 | v1 | 6 | 3 | 6/10 (1 ambig) |

---

## Analysis

### Haiku v4 vs Sonnet/Opus v4

Haiku scores 7/10 vs 5/10 for both Sonnet and Opus in v4. This is a notable improvement — Haiku outperforms the larger models in this run.

### Failure Pattern: Search-Before-Correct (P1, P8, P10)

All three failures share the same pattern: Haiku searched the KB first to locate/verify the relevant document, then correctly called the appropriate workflow. The routing destination was always right; only the entry point was wrong.

- **P1** (Hebrews/Paul): `search("Hebrews authorship Paul")` → `workflow(correct)` — should have gone directly to `workflow(correct)`
- **P8** (Exodus dates): `search("Exodus date 1200 BC")` → `workflow(correct)` — should have gone directly to `workflow(correct)`
- **P10** (John/Ephesus): `search("John")` → `workflow(transition)` — should have gone directly to `workflow(transition)`

This is a "verify-before-act" pattern: Haiku wants to confirm the entity exists before committing to a workflow. The correct behavior is to trust the prompt and call the workflow immediately (the workflow itself handles entity lookup).

### What Works Well

- **P2** (transition), **P4** (add), **P5** (maintain), **P6** (scan→maintain), **P7** (refresh), **P9** (maintain) all routed correctly and immediately.
- **P7** is a notable improvement over Sonnet v4, which went to `web_search` instead of `workflow(refresh)`. Haiku correctly identified "recent discoveries" as a refresh task.
- **P6** "Scan the repository" continues to route correctly to `workflow(maintain)` rather than `factbase(op=scan)` directly.
- **P10** is a significant improvement over Sonnet v4, which called `aim_memory_search` (wrong tool entirely). Haiku stayed within factbase tools and reached the correct workflow — just not as the first call.

### P3 "Some facts about Moses seem off" ✓

Haiku's search-first approach is acceptable here because the prompt is vague. The model fetched the Moses document, reviewed the review queue, and asked the user to clarify which facts seem off. This is the correct behavior for an ambiguous investigation prompt.

---

## Correct Routing Reference

| # | Prompt type | Expected first call |
|---|-------------|---------------------|
| 1 | Factual error assertion | `workflow(correct)` |
| 2 | Entity rename/recataloguing | `workflow(transition)` |
| 3 | Vague "seem off" investigation | `search` or `factbase(get_entity)` ✓ |
| 4 | Explicit "add research" | `workflow(add)` |
| 5 | General health check | `workflow(maintain)` |
| 6 | "Scan" command | `workflow(maintain)` (not `factbase(scan)` directly) |
| 7 | "Recent discoveries" / external update | `workflow(refresh)` |
| 8 | Date correction with scholarly citation | `workflow(correct)` |
| 9 | Fix quality + answer review questions | `workflow(maintain)` |
| 10 | Scholarly consensus shift | `workflow(transition)` |
