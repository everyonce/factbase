# Resolve Prompt Experiment: Type-Specific Evidence vs Research-Then-Batch

## Goal

Test 2 resolve instruction variants against baseline. Measure which approach gets the highest verified resolution rate.

## Variants

### Baseline (`variant` omitted or `variant=baseline`)
Current default resolve prompts. Generic evidence requirement for all types.

### Variant A — Type-specific evidence standards (`variant=type_evidence`)
Different evidence bars per question type:
- **stale**: Search for claim + current year. Wikipedia acceptable for established facts.
- **temporal**: Search for specific event date. Cite URL with date.
- **ambiguous**: Check KB first (get_entity). Only search externally if KB has no answer.
- **conflict**: Read BOTH referenced documents. Compare by recency and authority.
- **precision**: Search for quantitative replacement. Defer if no specific number exists.

Each question in the batch includes an `evidence_guidance` field with type-specific instructions.

### Variant B — Research-then-batch (`variant=research_batch`)
Restructured workflow: research first, answer second.
- Phase 1: Read full document via get_entity, do one comprehensive search covering all questions
- Phase 2: Answer all questions for that document in one batch
- Questions are grouped by `document_groups` instead of flat `questions` array

## How to Run

Use the `variant` parameter when calling the resolve workflow:

```
# Baseline (default)
workflow(workflow='resolve', step=1)

# Variant A
workflow(workflow='resolve', step=1, variant='type_evidence')

# Variant B
workflow(workflow='resolve', step=1, variant='research_batch')
```

The variant parameter is passed through to step 2 automatically. The agent must include `variant` in every step 2 call.

## Scoring Metrics

| Metric | Description | Target |
|--------|-------------|--------|
| % verified | Applied answers with sources | Higher is better |
| % believed | Parked without external source | Lower is better |
| % deferred | Explicit "I don't know" | Moderate is GOOD |
| Web searches / resolved | Efficiency metric | Lower is better |
| Total time | Practical constraint | Lower is better |

## Running the Experiment

```bash
# 1. Back up the database
cp ~/.local/share/factbase/factbase.db ~/.local/share/factbase/factbase.db.backup

# 2. Run baseline (3 times)
# Start fresh Kiro session, tell agent:
#   "Run workflow resolve on the WWII repo"
# Record metrics after each run, restore DB backup between runs

# 3. Run Variant A (3 times)
# Start fresh Kiro session, tell agent:
#   "Run workflow resolve with variant=type_evidence on the WWII repo"
# Record metrics, restore DB between runs

# 4. Run Variant B (3 times)
# Start fresh Kiro session, tell agent:
#   "Run workflow resolve with variant=research_batch on the WWII repo"
# Record metrics, restore DB between runs
```

## Results Template

| Run | Variant | Verified % | Believed % | Deferred % | Searches | Time |
|-----|---------|-----------|-----------|-----------|----------|------|
| 1   | baseline | | | | | |
| 2   | baseline | | | | | |
| 3   | baseline | | | | | |
| 1   | type_evidence | | | | | |
| 2   | type_evidence | | | | | |
| 3   | type_evidence | | | | | |
| 1   | research_batch | | | | | |
| 2   | research_batch | | | | | |
| 3   | research_batch | | | | | |
