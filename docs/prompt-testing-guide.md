# Factbase Prompt Evaluation Guide

This guide describes how to run a comprehensive evaluation of all agent-facing prompts in factbase. Run this after any significant prompt change to catch regressions.

---

## When to Run

- After changing any workflow instruction constant (in `src/mcp/tools/workflow/instructions.rs`)
- After changing any MCP tool description or op description
- After changing conflict pattern descriptions
- Before releasing a new version (regression check)
- When debugging unexpected agent behavior

---

## Setup

### 1. Create a fresh test KB

```bash
mkdir -p /Volumes/dev/factbase-test/prompt-eval
factbase init /Volumes/dev/factbase-test/prompt-eval
```

Or use the KB task:
```
[kb:/Volumes/dev/factbase-test/prompt-eval] Prompt evaluation — 30 steps
```

### 2. Choose a domain

The KB needs a domain with:
- Multiple entity types (person, work, organization, event)
- Time-sensitive facts (for temporal prompts)
- Internal + external sources (for citation prompts)
- Potential for conflicts (for conflict detection)

**Recommended:** "Jazz standards history" or "Classical composers and their works"

### 3. Use the `.factbase/instructions/` override for prompt testing

**Before filing a code task for a prompt change**, test it with an override file:

```toml
# /Volumes/dev/factbase-test/prompt-eval/.factbase/instructions/resolve.toml
[resolve]
answer_conflict_guidance = """
For overlapping facts, ask: 'Could both be true simultaneously?'
...
"""
```

Run a maintain/resolve and observe behavior. Iterate on the text freely — no recompile needed. Only file a `[factbase]` code task once the text is validated.

---

## The 30 Evaluation Steps

Run each step as a KB task with the specified user prompt. Evaluate the agent's **first tool call** and overall behavior.

### Workflow Routing (Steps 1–6)

These test that the agent routes to the right workflow from a natural language prompt.

| # | User prompt | Expected first call | Pass criterion |
|---|---|---|---|
| 1 | "Build me a KB about [domain]" | `workflow(create, ...)` | No other tool called first |
| 2 | "Add [new entity name]" | `workflow(add, topic=...)` | Not `workflow(correct)` |
| 3 | "Add a note to [existing entity]" | `workflow(correct, ...)` | NOT `workflow(add)` — existing entity |
| 4 | "Scan the KB" | `workflow(maintain)` | NOT `factbase(op=scan)` directly |
| 5 | "Check for new [domain-relevant news]" | `workflow(refresh)` | Not maintain or add |
| 6 | "Fix a wrong fact about [entity]" | `workflow(correct, ...)` | No search before calling correct |

### Correct vs. Transition (Steps 7–10)

These test the critical distinction between a false claim (correct) and a temporal change (transition).

| # | User prompt | Expected | Pass criterion |
|---|---|---|---|
| 7 | State a false claim (e.g., "Miles Davis didn't play trumpet") | `workflow(correct)` as first call | No search first |
| 8 | State a name/role change (e.g., "XSOLIS is now called PRIMA-X") | `workflow(transition)` + asks nomenclature | Asks before modifying |
| 9 | "Add a disputed flag to [entity]" | `workflow(correct, ...)` | NOT `workflow(add)` |
| 10 | Correction with explicit old/new dates | `workflow(correct)` with @t boundaries applied | Temporal context preserved |

### Clarification (Steps 11–13)

These test when the agent asks for clarification vs. acts with a default.

| # | User prompt | Expected | Pass criterion |
|---|---|---|---|
| 11 | "Fix John" (multiple Johns in KB) | `ASK: Which entity?` | One focused question, no action |
| 12 | "Update it" (no entity named) | `ASK: Which entity?` | Asks before calling any tool |
| 13 | "Make it better" | `workflow(maintain)` | Sensible default, no asking |

### Conflict Detection (Steps 14–17)

These test the conflict detector's ability to distinguish concurrent facts from contradictions.

| # | Setup | Expected | Pass criterion |
|---|---|---|---|
| 14 | Add two concurrent facts with same date | `parallel_overlap` or no conflict | NOT flagged as `same_entity_transition` |
| 15 | Add same role with overlapping date ranges | `same_entity_transition` flagged | IS flagged as conflict |
| 16 | Join date + role start same date | `parallel_overlap` or ignored | NOT flagged as conflict |
| 17 | Genuinely contradictory facts (different role titles, same period) | `same_entity_transition` flagged | IS flagged, correct pattern |

### Citation Quality (Steps 18–21)

These test the citation validation pipeline.

| # | Action | Expected | Pass criterion |
|---|---|---|---|
| 18 | Add fact with full URL citation | No weak-source question | Passes tier 1 |
| 19 | Add fact with vague citation ("email, 2025") | Weak-source question generated | Tier 1 fails |
| 20 | Dismiss a valid internal citation | `<!-- ✓ -->` appended to footnote | No re-flag on next check |
| 21 | Agent given vague Phonetool citation | Constructs `https://phonetool.amazon.com/users/{alias}` | Full URL in answer |

### Temporal Questions (Steps 22–25)

These test temporal question generation and resolution.

| # | Action | Expected | Pass criterion |
|---|---|---|---|
| 22 | Add fact without @t tag | Temporal question generated | Question appears in review queue |
| 23 | Add AWS service feature bullet | No temporal question | Stable capability, not flagged |
| 24 | Add role with open-ended @t[YYYY..] | No stale question | Open-ended range means still current |
| 25 | Resolve temporal with knowledge server | @t[YYYY..] + source citation | Uses tools to find actual date |

### Glossary + Ambiguous Questions (Steps 26–28)

These test the glossary auto-suppress mechanism.

| # | Action | Expected | Pass criterion |
|---|---|---|---|
| 26 | Use acronym already in glossary | No ambiguous question | Suppressed by glossary lookup |
| 27 | Use unknown term | Ambiguous question generated | Term not in glossary → question |
| 28 | Resolve ambiguous by creating glossary entry | No re-flag on next check | Term now in glossary → suppressed |

### Authoring Quality (Steps 29–30)

These test source and temporal discipline during content creation.

| # | Action | Expected | Pass criterion |
|---|---|---|---|
| 29 | Create doc with no sources | Missing-source questions generated | Checker correctly flags |
| 30 | Create doc with proper @t tags + citations | Clean check: 0 questions | No false positives |

---

## Scoring

After all 30 steps, compile a score:

- **PASS**: Agent behavior matched expected
- **PARTIAL**: Close but not exact (note what differed)
- **FAIL**: Agent did something different or wrong

**Target**: 28/30 PASS or better. Below 25/30 means a prompt needs investigation.

---

## Reporting

Write results to `docs/prompt-evaluation-report.md`:

```markdown
| # | Step | Instruction targeted | Agent action | Expected | Pass/Fail | Notes |
|---|------|---------------------|-------------|----------|-----------|-------|
| 1 | "Build me a KB" | workflow tool routing | workflow(create) | workflow(create) | ✅ | - |
...
```

Include the factbase version and date at the top.

---

## Iteration Cycle

When a step fails:
1. Check which instruction constant is responsible
2. Draft a fix in `.factbase/instructions/<workflow>.toml` (no recompile)
3. Re-run just that step to verify
4. Once validated, file a `[factbase]` code task to bake it in
5. Re-run the full 30 steps to confirm no regressions

---

## History

| Version | Score | Date | Notes |
|---|---|---|---|
| v2026.3.39 | — | 2026-03-16 | First evaluation planned |

---

## Methodology Notes

### v1 (Single-session evaluation)
Run all 30 steps in one kiro session. The agent accumulates context across steps — by step 20, it has seen what the KB looks like and what worked before. This introduces context bias: higher-capability models may score higher partly because they benefit more from accumulated context.

**Use for:** Quick baseline, understanding model differences, debugging obvious failures.

### v2 (Isolated-session evaluation, recommended)
Each step runs as a separate kiro session with:
1. Fresh context (no memory of prior steps)
2. KB reset to baseline: `git checkout baseline-tag -- .` before each step
3. Only the user prompt passed to the agent

This requires 30 tasks × N models = 30N tasks in the queue, but produces clean, reproducible results.

**Use for:** Regression testing, comparing prompt changes, official scores.

### Comparing results across methodology versions
Results from v1 and v2 are not directly comparable. When comparing runs, always note which methodology was used.
