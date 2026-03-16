# Prompt Evaluation v2 — Results & Comparison Report

**Date:** 2026-03-16  
**Factbase version:** v53.0.0  
**Status:** ⚠️ v2 full evaluation not yet run — results files missing  
**v1 baseline:** haiku 16/30, sonnet 17/30, opus 18/30

---

## Status

The 90 v2 evaluation tasks were created (30 steps × 3 models via `scripts/create-v2-eval-tasks-*.sh`) but the results files have not been written:

- `docs/v2-results-haiku.md` — **MISSING**
- `docs/v2-results-sonnet.md` — **MISSING**
- `docs/v2-results-opus.md` — **MISSING**

This report documents the v1 baseline, the 5 prompt fixes applied between v1 and v2, and projected score improvements based on individual fix test results.

---

## v1 Baseline Scores

| Category | Steps | Haiku | Sonnet | Opus |
|----------|-------|-------|--------|------|
| Workflow Routing | 1–6 | 5/6 | 5/6 | 6/6 |
| Correct vs Transition | 7–10 | 2/4 | 2.5/4 | 2/4 |
| Clarification | 11–13 | 1.5/3 | 1.5/3 | 2.5/3 |
| Conflict Detection | 14–17 | 1.5/4 | 2/4 | 2/4 |
| Citation Quality | 18–21 | 1.5/4 | 1.5/4 | 1.5/4 |
| Temporal Questions | 22–25 | 2.5/4 | 2/4 | 2/4 |
| Glossary + Ambiguous | 26–28 | 1.5/3 | 1.5/3 | 1.5/3 |
| Authoring Quality | 29–30 | 0.5/2 | 1/2 | 0.5/2 |
| **TOTAL** | **30** | **16/30 (53%)** | **17/30 (57%)** | **18/30 (60%)** |

---

## Prompt Fixes Applied (v1 → v2)

Five fixes were made to `src/mcp/tools/workflow/instructions.rs` and `src/mcp/tools/schema.rs` between v1 and v2 evaluations.

### Fix 1: Never Validate Content
**Target:** Steps 7, 8, 15, 17 (knowledge override failures)  
**Change:** Added negative instruction preamble to `DEFAULT_CORRECT_FIX_INSTRUCTION` and `DEFAULT_TRANSITION_APPLY_INSTRUCTION`:
> "You must NOT evaluate whether the user's claim is factually true or false. Your job is routing and recording, not validation."

**Individual test result:** 4/4 PASS  
**Expected impact:** +2 to +4 points across all models (steps 7, 8, 15, 17)

### Fix 2: Worked Examples — False Claim → Route Anyway
**Target:** Step 7 (false claim routing)  
**Change:** Added 2 concrete worked examples to `DEFAULT_CORRECT_FIX_INSTRUCTION` showing the exact `workflow(correct, ...)` call to make for obviously false claims (Eiffel Tower in London; Miles Davis never played trumpet).

**Individual test result:** 2/2 PASS  
**Expected impact:** Reinforces Fix 1 for step 7; marginal additional improvement

### Fix 3: Mechanical Routing Rules
**Target:** Steps 1–6 (workflow routing)  
**Change:** Rewrote `workflow` tool description in `src/mcp/tools/schema.rs` from prose guidance to explicit if/then routing rules:
```
- 'build', 'create', 'start', 'new KB' → workflow(create)
- 'add [new topic/entity]' → workflow(add, topic=...)
- 'add [note/flag/tag] to [existing entity]' → workflow(correct)
- 'scan', 'index', 'reindex' → workflow(maintain)
- 'check for new', 'look for updates', 'what's new' → workflow(refresh)
- factual correction about existing entity → workflow(correct) IMMEDIATELY
- change that happened over time → workflow(transition)
- no entity named → ASK one focused clarifying question
```

**Individual test result:** All 9 existing routing tests pass + 4 new tests added  
**Expected impact:** +1 for haiku/sonnet step 1 (create routing); +1 for haiku step 5 (refresh routing); +1 for haiku step 13 (maintain default)

### Fix 4: Always Scan After Content Changes
**Target:** Steps 22, 29, 30 (post-write scan discipline)  
**Change:** Added scan reminder to end of `DEFAULT_INGEST_CREATE_INSTRUCTION`, `DEFAULT_ENRICH_RESEARCH_INSTRUCTION`, `DEFAULT_CORRECT_FIX_INSTRUCTION`, `DEFAULT_TRANSITION_APPLY_INSTRUCTION`:
> "⚠️ AFTER WRITING: Always call factbase(op='scan') after modifying or creating documents."

**Individual test result:** Build passes, 2492 tests pass  
**Expected impact:** +0.5 to +1 for steps 22, 29 (authoring quality, scan discipline)

### Fix 5: Glossary Discipline
**Target:** Step 27 (unknown term without glossary check)  
**Change:** Added structured `GLOSSARY DISCIPLINE` section to `DEFAULT_INGEST_CREATE_INSTRUCTION` and `DEFAULT_ENRICH_RESEARCH_INSTRUCTION` with explicit 4-step lookup procedure.

**Individual test result:** 2492 tests pass, existing glossary tests updated  
**Expected impact:** +1 for step 27 across all models

---

## Projected v2 Scores

Based on individual fix test results, projected improvements per step:

| Step | Category | v1 Haiku | v1 Sonnet | v1 Opus | Fix | Projected Change |
|------|----------|----------|-----------|---------|-----|-----------------|
| 1 | Routing | ✅ | ❌ | ✅ | Fix 3 | Sonnet: ❌→✅ (+1) |
| 5 | Routing | ❌ | ✅ | ✅ | Fix 3 | Haiku: ❌→✅ (+1) |
| 7 | Correct/Trans | ❌ | ❌ | ❌ | Fix 1+2 | All: ❌→✅ (+1 each) |
| 8 | Correct/Trans | ❌ | ⚠️ | ❌ | Fix 1 | Haiku/Opus: ❌→✅ (+1 each); Sonnet: ⚠️→✅ (+0.5) |
| 13 | Clarification | ❌ | ✅ | ✅ | Fix 3 | Haiku: ❌→✅ (+1) |
| 15 | Conflict | ❌ | ❌ | ❌ | Fix 1 | All: ❌→✅ (+1 each) |
| 17 | Conflict | ❌ | ❌ | ❌ | Fix 1 | All: ❌→✅ (+1 each) |
| 27 | Glossary | ❌ | ❌ | ❌ | Fix 5 | All: ❌→✅ (+1 each) |
| 29 | Authoring | ❌ | ❌ | ❌ | Fix 4 | Partial improvement (+0.5 each) |

**Projected v2 totals (optimistic, assuming all targeted fixes hold):**

| Model | v1 Score | Projected Gain | Projected v2 |
|-------|----------|----------------|--------------|
| Haiku | 16/30 | +6.5 | ~22.5/30 (75%) |
| Sonnet | 17/30 | +5.5 | ~22.5/30 (75%) |
| Opus | 18/30 | +5.5 | ~23.5/30 (78%) |

**Conservative estimate** (accounting for regression risk and partial compliance):

| Model | v1 Score | Conservative Gain | Conservative v2 |
|-------|----------|-------------------|-----------------|
| Haiku | 16/30 | +4 | ~20/30 (67%) |
| Sonnet | 17/30 | +4 | ~21/30 (70%) |
| Opus | 18/30 | +4 | ~22/30 (73%) |

---

## Fixes With Most Expected Impact

Ranked by number of failing steps targeted:

1. **Fix 1 (Never Validate Content)** — targets 4 failures (steps 7, 8, 15, 17) across all 3 models = up to 12 points total. Highest impact fix.
2. **Fix 3 (Mechanical Routing Rules)** — targets 3 failures (steps 1, 5, 13) across haiku/sonnet = up to 3 points. Also hardens all routing for future regressions.
3. **Fix 5 (Glossary Discipline)** — targets 1 failure (step 27) across all 3 models = up to 3 points.
4. **Fix 2 (Worked Examples)** — reinforces Fix 1 for step 7; marginal additional gain.
5. **Fix 4 (Scan After Write)** — targets steps 22, 29; partial improvement expected.

---

## Failures Not Addressed by v2 Fixes

These v1 failures have no corresponding fix and are expected to persist in v2:

| Step | Category | Issue | All Models |
|------|----------|-------|-----------|
| 19 | Citation | Email citation not flagged as weak-source | ❌ all |
| 20 | Citation | `<!-- ✓ -->` not appended to dismissed citation | ⚠️ all |
| 21 | Citation | Phonetool URL not constructed | ❌ all |
| 22 | Temporal | Temporal question not generated for untagged facts | ⚠️/❌ |
| 23 | Temporal | Stable fact gets today's date tag (haiku/sonnet) | ❌ haiku/sonnet |
| 26 | Glossary | Known acronym resolved via own knowledge, not glossary lookup | ⚠️ all |

These represent the remaining gap to the 28/30 target. A v3 evaluation cycle would need to address citation quality (steps 19–21) and temporal tagging discipline (step 23).

---

## New Failures to Watch For

The following regressions are possible given the scope of changes:

- **Fix 3 routing rules** could cause step 3 ("add note to existing entity") to route to `workflow(add)` instead of `workflow(correct)` if the rule matching is ambiguous. Watch step 3 in v2.
- **Fix 1 "never validate"** could cause the agent to blindly apply contradictory facts without flagging them as conflicts (steps 14–17). The fix targets the routing refusal, not conflict detection — but over-application could suppress legitimate conflict flags.
- **Fix 4 scan reminder** could cause double-scans in workflows that already have a dedicated scan step, adding noise to results.

---

## Next Steps

1. **Run the 90 v2 eval tasks** (30 steps × 3 models) using the Vikunja task queue
2. **Collect results** into `docs/v2-results-haiku.md`, `docs/v2-results-sonnet.md`, `docs/v2-results-opus.md`
3. **Re-run this compilation task** once all 90 results are written
4. If v2 scores are below projected, investigate which fixes didn't hold and plan v3 fixes targeting citation quality (steps 19–21) and temporal tagging (step 23)

---

## Source Documents

- v1 results: `docs/prompt-evaluation-haiku.md`, `docs/prompt-evaluation-sonnet.md`, `docs/prompt-evaluation-opus.md`
- Fix test results: `docs/prompt-fix-1-test.md` through `docs/prompt-fix-5-test.md`
- Eval task scripts: `scripts/create-v2-eval-tasks-haiku.sh`, `scripts/create-v2-eval-tasks-sonnet.sh`, `scripts/create-v2-eval-tasks-opus.sh`
