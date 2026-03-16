# Prompt Evaluation Report — claude-opus-4.6

**Model:** claude-opus-4.6  
**Factbase version:** 2026.3.40 (built 2026-03-16)  
**Date:** 2026-03-16  
**KB:** `/Volumes/dev/factbase-test/prompt-eval-opus`  
**Domain:** History and evolution of jazz standards in American music  
**Method:** Each step run as a non-interactive `kiro-cli chat --no-interactive --trust-all-tools --model claude-opus-4.6` session from the KB directory with the factbase MCP server active.

---

## KB Structure

```
prompt-eval-opus/
├── perspective.yaml
├── standards/          (3 docs: all-the-things-you-are, autumn-leaves, round-midnight)
├── composers/          (3 docs: bill-evans, miles-davis, thelonious-monk)
├── recordings/         (1 doc: kind-of-blue)
├── venues/             (1 doc: village-vanguard)
└── definitions/        (2 docs: ii-v-i-progression, modal-jazz)
```

---

## Setup Note

The factbase MCP server failed to initialize on first run due to a missing embedding model cache (`.fastembed_cache` directory was present but incomplete — only a lock file, no model files). The model cache was copied from `/Users/daniel/work/factbase/.fastembed_cache` and `factbase scan` was run to initialize the database before evaluation began. All 30 steps were then run with the MCP server active.

---

## Evaluation Table

| # | Instruction Targeted | Test Scenario | Agent Action | Expected | Pass/Fail | Notes |
|---|---------------------|---------------|-------------|----------|-----------|-------|
| 1 | `DEFAULT_BOOTSTRAP_PROMPT` | "Build me a KB about jazz standards history" | `workflow(create, path=...)` as first factbase call | `workflow(create, ...)` as first call | ✅ PASS | Correctly routed to `workflow(create)` immediately; KB already had content but create is still the correct routing for "build me a KB" |
| 2 | `DEFAULT_INGEST_SEARCH_INSTRUCTION` | "Add bebop origins to the KB" | `workflow(add, topic="bebop origins", ...)` | `workflow(add, topic=...)` | ✅ PASS | Direct correct routing; "Add [new entity]" → `workflow(add)` immediately; created 6 new documents (charlie-parker, dizzy-gillespie, kenny-clarke, bud-powell, bebop definition, mintons-playhouse) |
| 3 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a note to Miles Davis that he also played flugelhorn" | `workflow(correct, correction="Add a note that Miles Davis also played flugelhorn")` | `workflow(correct, ...)` NOT `workflow(add)` — existing entity | ✅ PASS | Correctly identified existing entity → `workflow(correct)`; first factbase workflow call was correct |
| 4 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Scan the KB" | `workflow(maintain, step=1)` | `workflow(maintain)` NOT `factbase(op=scan)` directly | ✅ PASS | Correctly routed through maintain workflow, not direct scan |
| 5 | `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` | "Check for new jazz recordings and recent scholarship about Miles Davis" | `workflow(refresh, topic="Miles Davis")` | `workflow(refresh)` as first call | ✅ PASS | Correctly routed to refresh; "recent scholarship" signal correctly triggered refresh workflow |
| 6 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Fix a wrong fact about Miles Davis — he was born in 1925, not 1926" | `workflow(correct, correction="Miles Davis was born in 1925, not 1926")` | `workflow(correct)` with no search first | ✅ PASS | Called workflow immediately; no search before calling correct |
| 7 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Miles Davis didn't play trumpet — he actually played piano" | No tool call — responded with text refuting the claim | `workflow(correct)` as first call | ❌ FAIL | Model used its own knowledge to reject the instruction rather than calling `workflow(correct)` to update the KB |
| 8 | `DEFAULT_TRANSITION_PARSE_INSTRUCTION` | "The Village Vanguard is now called The New Village Vanguard Jazz Club" | No tool call — responded with text saying it couldn't verify the claim | `workflow(transition)` + asks nomenclature before modifying | ❌ FAIL | Model used its own knowledge to reject the instruction; did not call `workflow(transition)` at all |
| 9 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a disputed flag to Round Midnight — the composer attribution is contested" | `workflow(correct, correction="The composer attribution for 'Round Midnight' is contested and should be flagged as disputed.")` | `workflow(correct, ...)` NOT `workflow(add)` | ✅ PASS | Correctly routed to `workflow(correct)` for existing entity modification |
| 10 | `DEFAULT_CORRECT_FIX_INSTRUCTION` | "Correct: Bill Evans joined Miles Davis in 1958, not 1957. Date range 1958-1959." | `workflow(correct, correction="Bill Evans joined Miles Davis in 1958, not 1957. Date range 1958-1959.")` | `workflow(correct)` with @t boundaries applied | ✅ PASS | Called workflow immediately; correctly identified OLD=1957, NEW=1958, range=1958-1959 |
| 11 | Clarification routing | "Fix John" (no John in KB) | No tool call — asked "Your message seems incomplete — 'Fix John' doesn't give me enough to work with. Could you clarify?" | `ASK: Which entity?` (KB-aware) | ⚠️ PARTIAL | Asked for clarification without calling any tool (correct); not KB-aware (didn't search KB for "John" entities); question was generic not factbase-specific |
| 12 | Clarification routing | "Update it" (no entity named) | `think` first, then asked "Could you clarify what you'd like to update?" | `ASK: Which entity?` before any tool | ✅ PASS | Used `think` to reason about ambiguity, then asked before calling any workflow tool |
| 13 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Make it better" | `workflow(maintain)` as first call | `workflow(maintain)` as sensible default | ✅ PASS | Correctly defaulted to `workflow(maintain)` for vague improvement request |
| 14 | Conflict detection — `parallel_overlap` | Add two concurrent facts with same date @t[=1955] to Miles Davis | Added both facts (Newport Jazz Festival @t[=1955] and signed with Columbia @t[=1955]); no conflict flagged | `parallel_overlap` or no conflict — NOT `same_entity_transition` | ✅ PASS | Both concurrent facts added correctly; no false conflict flag |
| 15 | Conflict detection — `same_entity_transition` | Add "Primary instrument: piano @t[1955..1960]" to Miles Davis | Refused to add — responded with text saying Miles Davis played trumpet | `same_entity_transition` flagged | ❌ FAIL | Model used its own knowledge to reject the instruction; should have added the fact and let the conflict detector flag it |
| 16 | Conflict detection — `parallel_overlap` | Add join date + role start same date @t[=1958] to Bill Evans | Reported facts already in KB; no conflict flagged | `parallel_overlap` or ignored — NOT flagged as conflict | ✅ PASS | Correctly identified facts already present; no false conflict flag |
| 17 | Conflict detection — `same_entity_transition` | Add "Primary instrument: vibraphone @t[1940..1944]" to Thelonious Monk | Refused to add — responded with text saying Monk played piano | `same_entity_transition` flagged | ❌ FAIL | Same pattern as step 15: model rejected instruction using own knowledge instead of adding and flagging |
| 18 | Citation tier 1 — URL citation | Add fact with full Wikipedia URL citation to Kind of Blue | Added fact with Wikipedia URL citation; no weak-source question generated | No weak-source question — passes tier 1 | ✅ PASS | URL citation accepted; no weak-source question generated |
| 19 | Citation tier 1 — vague citation | Add fact with "email from a jazz professor, 2025" citation to Bill Evans | Added fact with email citation as `[^6]: Email from a jazz professor, 2025`; no weak-source question generated | Weak-source question generated | ❌ FAIL | Email citation was stored without triggering a weak-source question |
| 20 | Citation dismiss — `<!-- ✓ -->` | "The citation [^1] in the Kind of Blue document is valid — please dismiss" | Checked review queue; found no pending question for [^1]; reported no action needed | `<!-- ✓ -->` appended to footnote; no re-flag on next check | ⚠️ PARTIAL | Correctly identified no open question for [^1]; did NOT append `<!-- ✓ -->` to the citation line |
| 21 | Phonetool citation construction | "Add: Miles Davis discography documented at Phonetool for user alias milesdavis" | Asked clarifying questions about what Phonetool is; did not construct URL | Constructs `https://phonetool.amazon.com/users/{alias}` | ❌ FAIL | Did not recognize Phonetool as an internal Amazon tool; asked for clarification instead of constructing the URL |
| 22 | Temporal question generation | Add fact without @t tag: "Eva Cassidy covered Autumn Leaves" | Added fact with `@t[?]` tag; no temporal question generated after scan | Temporal question generated | ❌ FAIL | Added with `@t[?]` (correct behavior for unknown date) but no temporal question appeared in review queue after scan |
| 23 | Temporal question — stable fact | Add "The progression is used in jazz education curricula worldwide" to ii-V-I definition | Added fact with `@t[?]` tag; no today's date added | No temporal question — stable capability, not flagged | ✅ PASS | Correctly used `@t[?]` for a timeless/stable fact rather than adding today's date |
| 24 | Temporal — open-ended @t[YYYY..] | Add "Deborah Gordon has been managing the venue since 2018 @t[2018..]" to Village Vanguard | Added fact with `@t[2018..]`; no stale question generated | No stale question — open-ended range means still current | ✅ PASS | Open-ended temporal range correctly handled; no stale question generated |
| 25 | Temporal resolution with knowledge server | "Resolve temporal question about when Monk started at Minton's Playhouse" | Found existing temporal data in KB (@t[1940..1944]); performed web search to verify; reported answer with source | @t[YYYY..] + source citation via web search | ✅ PASS | Found answer in KB and used web search to verify; demonstrated web search capability |
| 26 | Glossary auto-suppress — known acronym | "Add: album features BN-style recording techniques" | Questioned the claim using own knowledge (BN = Blue Note, but Kind of Blue is Columbia); no ambiguous question generated | No ambiguous question — suppressed by glossary lookup | ⚠️ PARTIAL | Correct outcome (no ambiguous question) but model used own knowledge rather than glossary lookup; didn't call `factbase(op=list)` to check definitions |
| 27 | Glossary — unknown term | "Add: RLCF technique is commonly applied to ii-V-I progressions" | Added fact with `@t[?]`; no ambiguous question generated | Ambiguous question generated — term not in glossary | ❌ FAIL | Added the unknown acronym "RLCF" without generating an ambiguous question or checking the glossary |
| 28 | Glossary — resolve ambiguous | "RLCF stands for Root-Led Chord Fingering — create glossary entry and resolve questions" | Created `definitions/jazz-abbreviations.md` with RLCF entry and other abbreviations | No re-flag on next check — term now in glossary | ✅ PASS | Created glossary entry correctly; organized into a jazz-abbreviations definitions document |
| 29 | Authoring quality — missing sources | "Create a document about hard bop — no sources needed" | Created `definitions/hard-bop.md` with @t tags but no footnote citations; did NOT run scan/check | Missing-source questions generated | ❌ FAIL | Created document without sources and without running scan; no questions generated; model respected "no sources needed" instruction but didn't enforce source discipline |
| 30 | Authoring quality — clean check | Create cool jazz doc with proper @t tags + citations | Created `definitions/cool-jazz.md` with @t tags and footnote citations on all facts; ran `factbase(op=check)` and `factbase(op=scan)` | Clean check: 0 questions | ⚠️ PARTIAL | Document created with comprehensive @t tags and footnote citations; `factbase(op=check)` was run; however temporal/stale questions were generated (not a clean 0 questions) — no missing-source questions |

---

## Score Summary

| Category | Steps | Pass | Partial | Fail | Score |
|----------|-------|------|---------|------|-------|
| Workflow Routing (1–6) | 6 | 6 | 0 | 0 | 6/6 |
| Correct vs Transition (7–10) | 4 | 2 | 0 | 2 | 2/4 |
| Clarification (11–13) | 3 | 2 | 1 | 0 | 2.5/3 |
| Conflict Detection (14–17) | 4 | 2 | 0 | 2 | 2/4 |
| Citation Quality (18–21) | 4 | 1 | 1 | 2 | 1.5/4 |
| Temporal Questions (22–25) | 4 | 2 | 0 | 2 | 2/4 |
| Glossary + Ambiguous (26–28) | 3 | 1 | 1 | 1 | 1.5/3 |
| Authoring Quality (29–30) | 2 | 0 | 1 | 1 | 0.5/2 |
| **TOTAL** | **30** | **16** | **4** | **10** | **18/30** |

Counting partials as 0.5: **18/30 (60%)**  
Strict pass only: **16/30 (53%)**

**Target:** 28/30. This run is below target.

---

## Comparison with claude-haiku-4.5 and claude-sonnet-4.6

| Category | Haiku Score | Sonnet Score | Opus Score | Opus vs Sonnet |
|----------|-------------|--------------|------------|----------------|
| Workflow Routing (1–6) | 5/6 | 5/6 | 6/6 | +1 |
| Correct vs Transition (7–10) | 2/4 | 2.5/4 | 2/4 | -0.5 |
| Clarification (11–13) | 1.5/3 | 1.5/3 | 2.5/3 | +1 |
| Conflict Detection (14–17) | 1.5/4 | 2/4 | 2/4 | = |
| Citation Quality (18–21) | 1.5/4 | 1.5/4 | 1.5/4 | = |
| Temporal Questions (22–25) | 2.5/4 | 2/4 | 2/4 | = |
| Glossary + Ambiguous (26–28) | 1.5/3 | 1.5/3 | 1.5/3 | = |
| Authoring Quality (29–30) | 0.5/2 | 1/2 | 0.5/2 | -0.5 |
| **TOTAL** | **16/30** | **17/30** | **18/30** | **+1** |

Opus scores marginally higher than Sonnet (60% vs 57%), with improvements in workflow routing (perfect 6/6) and clarification handling. Regressions in correct-vs-transition and authoring quality.

---

## Key Findings

### Strengths

1. **Perfect workflow routing (6/6):** Opus correctly routed all 6 workflow routing tests — the only model to achieve a perfect score in this category. Notably, step 1 ("Build me a KB") was correctly routed to `workflow(create)` without hesitation.

2. **Clarification handling (2.5/3):** Opus showed the best clarification behavior of all three models. Step 12 ("Update it") was handled correctly — `think` was used to reason about ambiguity before asking, rather than blindly calling a workflow.

3. **Temporal resolution (step 25):** Opus proactively used web search to verify the Minton's Playhouse date even when the KB already had an answer, demonstrating good research behavior.

### Weaknesses

1. **Knowledge override pattern (steps 7, 8, 15, 17):** Opus consistently refused to add factually incorrect information to the KB, using its own training knowledge to reject instructions. This is the most significant failure pattern — the model should call `workflow(correct)` or `workflow(transition)` and let the KB's conflict detection handle it, rather than acting as a fact-checker.

2. **Citation quality (steps 19, 21):** Email citations were stored without triggering weak-source questions. Phonetool URL construction was not attempted — the model didn't recognize it as an internal Amazon tool.

3. **Temporal question generation (step 22):** Facts added with `@t[?]` did not generate temporal questions in the review queue after scanning. This may be a factbase behavior issue rather than a model issue.

4. **Authoring quality (step 29):** When told "no sources needed," the model complied without enforcing source discipline or running a post-creation scan.

### Recurring Pattern: Knowledge Override

Steps 7, 8, 15, and 17 all share the same failure mode: the model uses its own factual knowledge to reject user instructions rather than routing to the appropriate workflow. This is a fundamental tension between the model's training (to be accurate) and the factbase operator role (to record what the user says and let the system flag conflicts). The sonnet model showed the same pattern. This likely requires a stronger prompt instruction emphasizing that the agent's role is to record, not to validate.

---
