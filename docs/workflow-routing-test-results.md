# Workflow Routing Test Results

**Date:** 2026-03-12  
**KB:** bible-facts  
**Baseline tag:** workflow-test-baseline  
**Models:** claude-opus-4.6, claude-sonnet-4.6, claude-haiku-4.5

## Results

| # | Prompt | Model | Workflow | Ops | Correct? | Notes |
|---|--------|-------|----------|-----|----------|-------|
| 1 | The letter to the Hebrews was written by Paul... | claude-opus-4.6 | correct(text)+add(text) | 4 | ✓ | 46s: > I'm not going to apply this change. The factbase alre |
| 2 | The book previously known as Revelation shoul... | claude-opus-4.6 | transition | 20 | ✓ | 307s: [TIMEOUT after 300s] |
| 3 | Some facts about Moses seem off, can you look... | claude-opus-4.6 | correct(text)+add(text)+maintain(text) [direct:check] | 4 | ✓ | 107s: Want me to run the correction workflow on any of these, |
| 4 | Add research about the Council of Nicaea and ... | claude-opus-4.6 | add [direct:scan,check,answer] | 20 | ✓ | 307s: [TIMEOUT after 300s] |
| 5 | Make sure the KB is in good shape... | claude-opus-4.6 | maintain [direct:scan,check,answer] | 74 | ✓ | 307s: [TIMEOUT after 300s] |
| 6 | Scan the repository... | claude-opus-4.6 | add(text) [direct:scan] | 1 | ✗ | 30s: The main areas for improvement are source citations and |
| 7 | Check if there is been any recent archaeologi... | claude-opus-4.6 | unknown [direct:scan] | 5 | ✗ | 133s:  - Completed in 10.173s |
| 8 | The dates for the Exodus are wrong — schola... | claude-opus-4.6 | correct [direct:scan,answer] | 35 | ✓ | 307s: [TIMEOUT after 300s] |
| 9 | Fix any quality issues and answer the review ... | claude-opus-4.6 | add(text) [direct:scan,answer] | 80 | ✗ | 308s: [TIMEOUT after 300s] |
| 10 | Update the John entry — he is now more comm... | claude-opus-4.6 | transition(text)+add(text) [direct:scan] | 7 | ✓/ambig | 128s:  - Completed in 9.690s |
| 1 | The letter to the Hebrews was written by Paul... | claude-sonnet-4.6 | correct(text)+transition(text)+add(text) | 1 | ✓ | 26s: 3. Store it as-is (not recommended — it would be a fa |
| 2 | The book previously known as Revelation shoul... | claude-sonnet-4.6 | transition [direct:scan] | 17 | ✓ | 77s:  - Completed in 0.0s |
| 3 | Some facts about Moses seem off, can you look... | claude-sonnet-4.6 | correct(text)+add(text) [direct:check] | 6 | ✓ | 61s: Content itself: All the stated facts are biblically acc |
| 4 | Add research about the Council of Nicaea and ... | claude-sonnet-4.6 | add [direct:scan,check] | 24 | ✓ | 153s:  - Completed in 0.0s |
| 5 | Make sure the KB is in good shape... | claude-sonnet-4.6 | maintain+resolve [direct:scan,check,answer] | 42 | ✓ | 307s: [TIMEOUT after 300s] |
| 6 | Scan the repository... | claude-sonnet-4.6 | maintain(text) [direct:scan] | 1 | ✗ | 21s: The low source coverage (40%) and zero temporal coverag |
| 7 | Check if there is been any recent archaeologi... | claude-sonnet-4.6 | add | 6 | ✗ | 169s:  - Completed in 0.37s |
| 8 | The dates for the Exodus are wrong — schola... | claude-sonnet-4.6 | correct [direct:scan] | 19 | ✓ | 185s:  - Completed in 0.19s |
| 9 | Fix any quality issues and answer the review ... | claude-sonnet-4.6 | maintain+resolve [direct:scan,check,answer] | 42 | ✓ | 307s: [TIMEOUT after 300s] |
| 10 | Update the John entry — he is now more comm... | claude-sonnet-4.6 | transition [direct:scan,check,answer] | 14 | ✓/ambig | 92s:  - Completed in 0.0s |
| 1 | The letter to the Hebrews was written by Paul... | claude-haiku-4.5 | correct [direct:scan] | 9 | ✓ | 46s: Finding: The Hebrews document (ID: 22861d) already corr |
| 2 | The book previously known as Revelation shoul... | claude-haiku-4.5 | transition | 2 | ✓ | 16s:  - Completed in 0.0s |
| 3 | Some facts about Moses seem off, can you look... | claude-haiku-4.5 | resolve [direct:scan] | 8 | ✗ | 46s:  - Completed in 10.18s |
| 4 | Add research about the Council of Nicaea and ... | claude-haiku-4.5 | add [direct:scan,check] | 15 | ✓ | 82s:  - Completed in 0.6s |
| 5 | Make sure the KB is in good shape... | claude-haiku-4.5 | maintain+resolve [direct:scan,check,answer] | 38 | ✓ | 220s: > ## Maintenance Complete |
| 6 | Scan the repository... | claude-haiku-4.5 | unknown [direct:scan] | 1 | ✗ | 21s: > Scan complete. Here's the status: |
| 7 | Check if there is been any recent archaeologi... | claude-haiku-4.5 | unknown | 1 | ✗ | 10s:  - Completed in 0.18s |
| 8 | The dates for the Exodus are wrong — schola... | claude-haiku-4.5 | correct [direct:scan,check] | 10 | ✓ | 46s: > Perfect. The correction is complete. Here's the summa |
| 9 | Fix any quality issues and answer the review ... | claude-haiku-4.5 | maintain+resolve [direct:scan,check,answer] | 39 | ✓ | 307s: [TIMEOUT after 300s] |
| 10 | Update the John entry — he is now more comm... | claude-haiku-4.5 | transition [direct:scan,check] | 19 | ✓/ambig | 77s:  - Completed in 0.0s |

## Summary by Model

- **claude-opus-4.6**: 6 correct, 1 ambiguous-ok, 3 wrong (out of 10)
- **claude-sonnet-4.6**: 7 correct, 1 ambiguous-ok, 2 wrong (out of 10)
- **claude-haiku-4.5**: 6 correct, 1 ambiguous-ok, 3 wrong (out of 10)

## Prompt Reference

| # | Prompt | Expected |
|---|--------|----------|
| 1 | The letter to the Hebrews was written by Paul | correct |
| 2 | The book previously known as Revelation should now be catalogued as Apocalypse of John | transition |
| 3 | Some facts about Moses seem off, can you look at them | search+correct/maintain |
| 4 | Add research about the Council of Nicaea and how it affected the biblical canon | add |
| 5 | Make sure the KB is in good shape | maintain |
| 6 | Scan the repository | maintain (not raw scan) |
| 7 | Check if there is been any recent archaeological discoveries related to Jericho | refresh |
| 8 | The dates for the Exodus are wrong — scholars now say 1446 BC not 1200 BC | correct |
| 9 | Fix any quality issues and answer the review questions | maintain (not direct ops) |
| 10 | Update the John entry — he is now more commonly associated with Ephesus than Jerusalem in modern scholarship | correct or transition (ambiguous) |

## Key Observations

### Universal failures (all 3 models)
- **P6 "Scan the repository"** — Every model ran `factbase(op=scan)` directly instead of routing to `maintain`. The word "scan" is too literal and overrides workflow-first behavior. This is the clearest routing failure.
- **P7 "Check if there is been any recent archaeological discoveries related to Jericho"** — No model used `refresh`. Opus ran a raw scan, Sonnet used `add`, Haiku returned nothing useful. The `refresh` workflow is the least-discovered path; models don't associate "recent discoveries" with it.

### Correct/transition distinction
- **P1 "Hebrews written by Paul"** — All models correctly identified this as a `correct` case (never true). Opus and Sonnet described the workflow in text but didn't call the tool; Haiku actually called `workflow(workflow='correct')`. The distinction was understood but execution varied.
- **P2 "Revelation → Apocalypse of John"** — All models correctly used `transition`. Strong signal words ("previously known as", "now catalogued as") reliably trigger transition routing.
- **P8 "Exodus dates wrong"** — All models used `correct`. The framing "scholars now say X not Y" was correctly interpreted as a factual correction rather than a transition.
- **P10 "John/Ephesus"** — All models chose `transition` (defensible: modern scholarship shift = temporal change). No model asked for clarification. Sonnet and Haiku called the workflow tool; Opus described it in text.

### Maintain workflow
- **P5 "Make sure the KB is in good shape"** — All models routed to `maintain`. ✓
- **P9 "Fix any quality issues and answer the review questions"** — Sonnet and Haiku used `maintain`; Opus used direct `factbase(op=scan)` + `factbase(op=answer)` without going through the workflow. All models also called direct ops alongside maintain, which is a secondary concern.

### Direct ops bypass (all models)
All three models frequently called `factbase(op=scan)`, `factbase(op=check)`, and `factbase(op=answer)` directly alongside or instead of workflow calls. This is the most pervasive issue — models treat factbase as a direct API rather than routing through workflows.

### Timeout behavior
Several tests hit the 300s timeout (P2 Opus, P4 Opus, P5 Opus, P8 Opus, P9 all models). Longer workflows (transition, add, maintain) regularly exceed 5 minutes. The timeout did not prevent workflow detection since the workflow call happens early.

### Model comparison
| Model | Correct | Ambig-OK | Wrong | Notes |
|-------|---------|----------|-------|-------|
| claude-sonnet-4.6 | 7 | 1 | 2 | Best overall; P6 and P7 failures only |
| claude-opus-4.6 | 6 | 1 | 3 | P6, P7, P9 failures; P9 used direct ops |
| claude-haiku-4.5 | 6 | 1 | 3 | P3, P6, P7 failures; P3 used resolve instead of correct/maintain |

### Recommendations
1. **Strengthen "scan" → maintain routing** in agent instructions. The word "scan" should not trigger raw `factbase(op=scan)`.
2. **Add refresh workflow examples** to agent instructions. Models don't know when to use it.
3. **Suppress direct factbase ops** in the agent system prompt — require workflow() as the entry point for all multi-step operations.
4. **Sonnet is the most reliable router** for this task. Haiku is faster but misroutes vague prompts (P3, P7).

## Log Files
Raw logs: `/tmp/workflow-test-logs/test-{model}-p{n}.log`
