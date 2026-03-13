# Workflow Routing Test Results — Sonnet v4

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline (95816be)  
**Model:** claude-sonnet-4.6  
**Agent:** factbase-only (tools restricted to `@factbase` MCP only)  
**Purpose:** Routing test — does Sonnet call the correct workflow/op first?

> Each test: `git checkout workflow-test-baseline -- .` → run prompt → capture first tool call.

---

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | The letter to the Hebrews was written by Paul | *(no tool call)* | ✗ | Should be `workflow(correct)`; Sonnet answered entirely from its own knowledge, explaining the authorship debate without touching the KB at all |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `workflow(transition, step=1)` | ✓ | Direct correct routing; proceeded through full transition workflow |
| 3 | Some facts about Moses seem off, can you look at them | `factbase(op=list, title_filter="Moses")` | ✓ | Lookup-first acceptable for vague "seem off" — fetched entity, summarised content, asked user to clarify which facts |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add, topic="Council of Nicaea...", step=1)` | ✓ | Direct correct routing; "Add research" → `workflow(add)` immediately |
| 5 | Make sure the KB is in good shape | `workflow(maintain, step=1)` | ✓ | Direct correct routing; proceeded through full maintain sequence |
| 6 | Scan the repository ⭐ | `workflow(maintain, step=1)` | ✓ | **KEY**: Did NOT call `factbase(op=scan)` directly; routed through maintain workflow correctly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | `web_search("recent archaeological discoveries Jericho 2025 2026")` | ✗ | Should be `workflow(refresh)`; Sonnet went straight to web search, answered from external results, never called `workflow(refresh)` |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `think(...)` | ✗ | Should be `workflow(correct)`; Sonnet used internal reasoning to push back on the scholarly claim, then answered from knowledge — never called any factbase tool |
| 9 | Fix any quality issues and answer the review questions | `factbase(op=check)` | ✗ | Should be `workflow(maintain)`; Sonnet called `factbase(check)` directly, then `factbase(review_queue)` — bypassed the maintain workflow entry point |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `aim_memory_search("John")` | ✗ | Should be `workflow(transition)`; Sonnet searched the memory MCP (not factbase), found nothing, and asked for clarification — never reached the KB |

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-sonnet-4.6** | **v4 (this run)** | **5** | **5** | **5/10** |
| claude-opus-4.6 | v4 | 5 | 5 | 5/10 |
| claude-sonnet-4.6 | v3 | 9 | 1 | 9/10 (1 ambig) |
| claude-sonnet-4.6 | v2 | 9 | 1 | 9/10 (1 ambig) |
| claude-opus-4.6 | v1 | 6 | 3 | 6/10 (1 ambig) |
| claude-haiku-4.5 | v1 | 6 | 3 | 6/10 (1 ambig) |

---

## Analysis

### Regression vs v2/v3 Sonnet

Sonnet v4 scores 5/10 vs 9/10 in v2 and v3. This is a significant regression. The failures cluster into three patterns:

**Pattern 1: Answered from own knowledge (P1, P8)**  
Both factual-correction prompts were handled entirely from the model's internal knowledge without touching the KB. For P1 (Hebrews authorship), Sonnet gave a detailed explanation of the authorship debate. For P8 (Exodus dates), it used `think` to reason about the scholarly debate and pushed back on the premise — never calling `workflow(correct)`.

**Pattern 2: Wrong tool entirely (P7, P10)**  
- P7: Sonnet went to `web_search` for "recent archaeological discoveries" — treating it as a general research question rather than a KB refresh task.  
- P10: Sonnet searched `aim_memory_search` (the memory MCP) instead of the factbase, found nothing, and asked for clarification.

**Pattern 3: Correct destination, wrong entry point (P9)**  
Sonnet called `factbase(op=check)` directly instead of `workflow(maintain)`. It reached the right operations (check + review_queue) but bypassed the workflow entry point.

### What Still Works

P2 (transition), P4 (add), P5 (maintain), P6 (scan→maintain) all routed correctly. The explicit-intent prompts ("Add research", "Make sure KB is in good shape", "Scan the repository", and the rename phrasing) still trigger correct routing.

### P6 "Scan the repository" ✓ (fix holds)

Sonnet correctly routed to `workflow(maintain)` rather than calling `factbase(op=scan)` directly. The routing fix from v2 continues to hold.

### P10 Memory MCP Interference

The `aim_memory_search` call on P10 suggests the model is not operating in a factbase-only context — it has access to the memory MCP and defaulted to it when the prompt said "Update the John entry." This is a test environment issue: the agent should be restricted to factbase tools only.

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
