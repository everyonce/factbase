# Workflow Routing Test Results — Opus v4

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline (95816be)  
**Model:** claude-opus-4.6  
**Agent:** factbase-only (tools restricted to `@factbase` MCP only)  
**Purpose:** Routing test — does Opus call the correct workflow/op first?

> Each test: `git checkout workflow-test-baseline -- .` → run prompt → capture first tool call.

---

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | The letter to the Hebrews was written by Paul | `search("Hebrews authorship")` | ✗ | Should be `workflow(correct)`; Opus searched KB first, then explained discrepancy without routing to correct workflow |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `search("Revelation book Bible")` | ✗ | Should be `workflow(transition)`; Opus searched first, found doc already named "Apocalypse of John" (stale index from prior test run), declared "already done" |
| 3 | Some facts about Moses seem off, can you look at them | `search("Moses")` | ✓ | Lookup-first acceptable for vague "seem off" — consistent with v3 Sonnet behavior (get_entity first); proceeded to check + review_queue |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add, topic="Council of Nicaea...", step=1)` | ✓ | Direct correct routing; "Add research" → workflow(add) immediately |
| 5 | Make sure the KB is in good shape | `workflow(maintain, step=1)` | ✓ | Direct correct routing; proceeded through full maintain sequence |
| 6 | Scan the repository ⭐ | `workflow(maintain, step=1)` | ✓ | **KEY**: Did NOT call `factbase(op=scan)` directly; routed through maintain workflow correctly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | `search("Jericho archaeological discoveries")` | ✗ | Should be `workflow(refresh)`; Opus searched first, then called `workflow(refresh)` as second call |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `search("Exodus date")` | ✗ | Should be `workflow(correct)`; Opus searched first to verify current content before routing |
| 9 | Fix any quality issues and answer the review questions | `workflow(maintain, step=1)` | ✓ | Correct entry point; called workflow(maintain) and factbase(review_queue) in parallel |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `search("John apostle")` | ✗ | Should be `workflow(transition)`; Opus searched first, read the doc, then correctly identified and called `workflow(transition)` — right destination, wrong entry point |

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-opus-4.6** | **v4 (this run)** | **5** | **5** | **5/10** |
| claude-sonnet-4.6 | v3 | 9 | 1 | 9/10 (1 ambig) |
| claude-sonnet-4.6 | v2 | 9 | 1 | 9/10 (1 ambig) |
| claude-opus-4.6 | v1 | 6 | 3 | 6/10 (1 ambig) |
| claude-haiku-4.5 | v1 | 6 | 3 | 6/10 (1 ambig) |

---

## Analysis

### Opus Pattern: "Lookup Before Routing"

Opus consistently searched or fetched the entity before deciding on a workflow for prompts 1, 2, 7, 8, and 10. This is a **search-first** pattern:

- P1 (correct): searched "Hebrews authorship" → explained discrepancy → never called `workflow(correct)`
- P2 (transition): searched "Revelation" → found stale index showing already-renamed doc → declared done
- P7 (refresh): searched "Jericho archaeological discoveries" → then called `workflow(refresh)` (second call)
- P8 (correct): searched "Exodus date" → fetched entity → searched "1200 BC" → never called `workflow(correct)`
- P10 (transition): searched "John apostle" → fetched entity → then correctly called `workflow(transition)`

For P7 and P10, Opus eventually reached the right workflow — just not as the first call. For P1 and P8, it never called a correction workflow at all.

### Regression vs v1 Opus

v4 scores 5/10 vs v1 Opus 6/10. The regression is primarily on P1 (Hebrews/correct) and P8 (Exodus/correct) — both factual-correction prompts where Opus searched first and then answered from its own knowledge rather than routing to `workflow(correct)`.

### P6 "Scan the repository" ✓ (fix holds)

Opus correctly routed to `workflow(maintain)` rather than calling `factbase(op=scan)` directly. The routing fix from v2 holds for Opus.

### Stale Index Note (P2)

The factbase MCP server index was not fully reset between test runs despite `git checkout workflow-test-baseline -- .`. For P2, the server returned the post-transition state (Apocalypse of John) even after the git restore. This affected the model's response but not the routing classification — the first call was still `search` (wrong), not `workflow(transition)` (correct).

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
