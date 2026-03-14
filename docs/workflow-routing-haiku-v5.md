# Workflow Routing Test Results — Haiku v5

**Date:** 2026-03-12  
**KB:** bible-facts  
**Model:** claude-haiku-4.5  
**Fixes tested:** #516 (call workflow immediately), #517 (factbase-first priority)  
**Purpose:** Routing test post-fixes — compare against v4 Haiku results (7/10)

---

## Results

| # | Prompt | First Call | Correct? | v4 Result | Notes |
|---|--------|-----------|----------|-----------|-------|
| 1 | The letter to the Hebrews was written by Paul | `workflow(correct, step=1)` | ✓ | ✗ | **FIXED** — v4 searched first; now calls workflow immediately |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | `workflow(transition, step=1)` | ✓ | ✓ | No regression |
| 3 | Some facts about Moses seem off, can you look at them | `factbase(list, title_filter="Moses")` | ✓ | ✓ | Lookup-first acceptable for vague prompt |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | `workflow(add, topic="...", step=1)` | ✓ | ✓ | No regression |
| 5 | Make sure the KB is in good shape | `workflow(maintain, step=1)` | ✓ | ✓ | No regression |
| 6 | Scan the repository | `workflow(maintain, step=1)` | ✓ | ✓ | Correctly avoids `factbase(scan)` directly |
| 7 | Check if there have been any recent archaeological discoveries related to Jericho | `workflow(refresh, topic="Jericho...", step=1)` | ✓ | ✓ | No regression |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | `workflow(correct, step=1)` | ✓ | ✗ | **FIXED** — v4 searched first; now calls workflow immediately |
| 9 | Fix any quality issues and answer the review questions | `workflow(maintain, step=1)` | ✓ | ✓ | No regression |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | `workflow(transition, step=1)` | ✓ | ✗ | **FIXED** — v4 searched first; now calls workflow immediately |

---

## Summary

| Model | Run | Correct | Wrong | Score |
|-------|-----|---------|-------|-------|
| **claude-haiku-4.5** | **v5 (this run)** | **10** | **0** | **10/10** |
| claude-haiku-4.5 | v4 | 7 | 3 | 7/10 |
| claude-sonnet-4.6 | v4 | 5 | 5 | 5/10 |
| claude-opus-4.6 | v4 | 5 | 5 | 5/10 |
| claude-sonnet-4.6 | v3 | 9 | 1 | 9/10 (1 ambig) |
| claude-sonnet-4.6 | v2 | 9 | 1 | 9/10 (1 ambig) |
| claude-opus-4.6 | v1 | 6 | 3 | 6/10 (1 ambig) |
| claude-haiku-4.5 | v1 | 6 | 3 | 6/10 (1 ambig) |

---

## Analysis

### Fixes #516 and #517 — Full Impact

All three v4 failures (P1, P8, P10) are resolved in v5. The "verify-before-act" pattern — where Haiku searched the KB first to locate/confirm an entity before calling the correct workflow — is eliminated.

- **P1** (Hebrews/Paul): `workflow(correct)` called immediately ✓ (was: `search` first)
- **P8** (Exodus dates): `workflow(correct)` called immediately ✓ (was: `search` first)
- **P10** (John/Ephesus): `workflow(transition)` called immediately ✓ (was: `search` first)

Fix #516 ("call workflow immediately") directly addresses the root cause: the model now trusts the prompt and delegates entity lookup to the workflow itself rather than pre-verifying.

### No Regressions

All 7 prompts that were correct in v4 remain correct in v5. P3 (vague "seem off") continues to correctly use lookup-first, which is the expected behavior for ambiguous investigation prompts.

### Haiku v5 vs All Prior Runs

Haiku v5 achieves a perfect 10/10 — the first perfect score across all model/version combinations tested. This surpasses Sonnet v3/v2 (9/10) and all v4 runs.

### P10 Routing Choice

P10 was routed to `workflow(transition)` rather than `workflow(correct)`. This is the preferred routing: the prompt describes a scholarly consensus shift (Jerusalem → Ephesus association), which was plausibly true at some point and has now changed — fitting the transition pattern. Both `transition` and `correct` are marked acceptable per the test spec.
