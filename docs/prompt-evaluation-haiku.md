# Prompt Evaluation Report — claude-haiku-4.5

**Model:** claude-haiku-4.5  
**Factbase version:** 2026.3.40 (built 2026-03-16)  
**Date:** 2026-03-16  
**KB:** `/Volumes/dev/factbase-test/prompt-eval-haiku`  
**Domain:** History and evolution of jazz standards in American music  
**Method:** Each step run as a non-interactive `kiro-cli chat --no-interactive --trust-all-tools` session from the KB directory with the factbase MCP server active.

---

## KB Structure

```
prompt-eval-haiku/
├── perspective.yaml
├── standards/          (3 docs: all-the-things-you-are, autumn-leaves, round-midnight)
├── composers/          (3 docs: bill-evans, miles-davis, thelonious-monk)
├── recordings/         (1 doc: kind-of-blue)
├── venues/             (1 doc: village-vanguard)
└── definitions/        (2 docs: ii-v-i-progression, modal-jazz)
```

---

## Evaluation Table

| # | Instruction Targeted | Test Scenario | Agent Action | Expected | Pass/Fail | Notes |
|---|---------------------|---------------|-------------|----------|-----------|-------|
| 1 | `DEFAULT_BOOTSTRAP_PROMPT` | "Build me a KB about jazz standards history" | `workflow(create, domain="jazz standards history...", entity_types="...", path="...")` | `workflow(create, ...)` as first call | ✅ PASS | Correct immediate routing; no other tool called first |
| 2 | `DEFAULT_INGEST_SEARCH_INSTRUCTION` | "Add bebop origins to the KB" | `workflow(add, topic="bebop origins", step=1)` | `workflow(add, topic=...)` | ✅ PASS | Direct correct routing; "Add [new entity]" → `workflow(add)` immediately |
| 3 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a note to Miles Davis that he also played flugelhorn" | `workflow(correct, correction="...")` | `workflow(correct, ...)` NOT `workflow(add)` | ✅ PASS | Correctly identified existing entity → `workflow(correct)` |
| 4 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Scan the KB" | `workflow(maintain, step=1)` | `workflow(maintain)` NOT `factbase(op=scan)` | ✅ PASS | Correctly routed through maintain workflow, not direct scan |
| 5 | `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` | "Check for new jazz recordings and recent scholarship about Miles Davis" | `web_search("Miles Davis new jazz recordings 2025 2026")` | `workflow(refresh)` as first call | ❌ FAIL | Called `web_search` directly instead of `workflow(refresh)`; "recent scholarship" should trigger refresh workflow |
| 6 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Fix a wrong fact about Miles Davis — he was born in 1925, not 1926" | `workflow(correct, correction="...")` | `workflow(correct)` with no search first | ✅ PASS | Called workflow immediately; used `think` to verify the claim internally |
| 7 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Miles Davis didn't play trumpet — he actually played piano" | No tool call — responded with text refuting the claim | `workflow(correct)` as first call | ❌ FAIL | Model used its own knowledge to reject the instruction rather than calling `workflow(correct)` to update the KB |
| 8 | `DEFAULT_TRANSITION_PARSE_INSTRUCTION` | "The Village Vanguard is now called The New Village Vanguard Jazz Club" | No tool call — responded with text questioning the claim | `workflow(transition)` + asks nomenclature | ❌ FAIL | Model used its own knowledge to question the claim instead of calling `workflow(transition)` immediately |
| 9 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a disputed flag to Round Midnight — the composer attribution is contested" | `workflow(correct, correction="...")` | `workflow(correct, ...)` NOT `workflow(add)` | ✅ PASS | Correctly routed to `workflow(correct)` for existing entity modification |
| 10 | `DEFAULT_CORRECT_FIX_INSTRUCTION` | "Correct: Bill Evans joined Miles Davis in 1958, not 1957. Date range 1958-1959." | `workflow(correct, correction="...")` with temporal context preserved | `workflow(correct)` with @t boundaries applied | ✅ PASS | Called workflow immediately; `think` correctly identified OLD=1957, NEW=1958, range=1958-1959 |
| 11 | Clarification routing | "Fix John" (no John in KB) | Asked "Is 'John' a file, variable, function?" — no KB tool call | `ASK: Which entity?` (KB-aware) | ⚠️ PARTIAL | Asked for clarification but not KB-aware; didn't search KB for "John" entities; generic question not factbase-specific |
| 12 | Clarification routing | "Update it" (no entity named) | Asked "Could you clarify what you'd like me to update?" — no tool call | `ASK: Which entity?` before any tool | ✅ PASS | Correctly asked before calling any tool |
| 13 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Make it better" | Asked for clarification — no tool call | `workflow(maintain)` as sensible default | ❌ FAIL | Should have defaulted to `workflow(maintain)` for vague improvement request; instead asked for clarification |
| 14 | Conflict detection — `parallel_overlap` | Add two concurrent facts with same date @t[=1955] | Added both facts; no conflict flagged | `parallel_overlap` or no conflict — NOT `same_entity_transition` | ✅ PASS | Both concurrent facts added correctly; no false conflict flag |
| 15 | Conflict detection — `same_entity_transition` | Add "Primary instrument: piano @t[1955..1960]" to Miles Davis | Refused to add — responded with text saying Miles Davis played trumpet | `same_entity_transition` flagged | ❌ FAIL | Model used its own knowledge to reject the instruction; should have added the fact and let the conflict detector flag it |
| 16 | Conflict detection — `parallel_overlap` | Add join date + role start same date @t[=1958] to Bill Evans | Added both facts; noted date discrepancy with existing @t[1959..1961] | `parallel_overlap` or ignored — NOT flagged as conflict | ⚠️ PARTIAL | Added facts correctly; noted discrepancy but didn't explicitly classify as `parallel_overlap` |
| 17 | Conflict detection — `same_entity_transition` | Add "Primary instrument: vibraphone @t[1940..1944]" to Thelonious Monk | Refused to add — responded with text saying Monk played piano | `same_entity_transition` flagged | ❌ FAIL | Same pattern as step 15: model rejected instruction using own knowledge instead of adding and flagging |
| 18 | Citation tier 1 — URL citation | Add fact with full Wikipedia URL citation | Added fact; no weak-source question generated | No weak-source question — passes tier 1 | ✅ PASS | URL citation accepted; scan generated no question for this citation |
| 19 | Citation tier 1 — vague citation | Add fact with "email from a jazz professor, 2025" citation | Added fact with email citation; no weak-source question for email | Weak-source question generated | ❌ FAIL | Email citation was stored as `[^6]: Email from a jazz professor, 2025` without triggering a weak-source question |
| 20 | Citation dismiss — `<!-- ✓ -->` | "The citation [^3] in Kind of Blue is valid — please dismiss" | Checked review queue; found no pending question for [^3] | `<!-- ✓ -->` appended; no re-flag on next check | ⚠️ PARTIAL | Citation was already marked `<!-- ✓ -->` from a prior step; model correctly reported no action needed |
| 21 | Phonetool citation construction | "Add: discography documented at Phonetool for user alias milesdavis" | Added "Phonetool (user alias: milesdavis)" without constructing full URL | Constructs `https://phonetool.amazon.com/users/{alias}` | ❌ FAIL | Did not construct the full Phonetool URL; stored alias reference only |
| 22 | Temporal question generation | Add fact without @t tag: "Eva Cassidy covered Autumn Leaves" | Added fact WITH @t[=1996] tag (proactively added) | Temporal question generated | ⚠️ PARTIAL | Model proactively added @t[=1996] rather than leaving it untagged; good authoring behavior but test intent was to check question generation for untagged facts |
| 23 | Temporal question — stable fact | Add "The progression is used in jazz education curricula worldwide" | Added fact with @t[=2026-03-16] (today's date) | No temporal question — stable capability | ❌ FAIL | Added today's date as temporal tag to a timeless/stable fact; should have recognized this as a stable fact not requiring a temporal tag |
| 24 | Temporal — open-ended @t[YYYY..] | Add "Deborah Gordon managing since 2018 @t[2018..]" | Added fact with @t[2018..]; no stale question generated | No stale question — open-ended range means still current | ✅ PASS | Open-ended temporal range correctly handled; no stale question generated |
| 25 | Temporal resolution with knowledge server | "Resolve temporal question about when Monk started at Minton's" | Used `web_search` to find 1941; updated @t[1940..1944] → @t[1941..1944]; added source citation | @t[YYYY..] + source citation | ✅ PASS | Correctly used web search to find date; updated both Monk doc and Bebop doc; added citation |
| 26 | Glossary auto-suppress — known acronym | "Add: album features BN-style recording techniques" | Added fact; interpreted "BN" as Blue Note using own knowledge; no ambiguous question | No ambiguous question — suppressed by glossary lookup | ⚠️ PARTIAL | No ambiguous question generated (correct outcome) but model used own knowledge rather than glossary lookup; didn't call `factbase(op=list)` to check definitions |
| 27 | Glossary — unknown term | "Add: RLCF technique is commonly applied to ii-V-I progressions" | Added fact with @t[=2026-03-16]; no ambiguous question generated | Ambiguous question generated — term not in glossary | ❌ FAIL | Added the unknown acronym "RLCF" without generating an ambiguous question or checking the glossary |
| 28 | Glossary — resolve ambiguous | "RLCF stands for Root-Led Chord Fingering — create glossary entry and resolve questions" | Created glossary entry in Music Terms Glossary; checked review queue for ambiguous questions | No re-flag on next check — term now in glossary | ✅ PASS | Created glossary entry correctly; checked for existing ambiguous questions about RLCF |
| 29 | Authoring quality — missing sources | "Create a document about hard bop — no sources needed" | Created document without sources or @t tags; did NOT run scan/check | Missing-source questions generated | ❌ FAIL | Created document without sources and without running scan; no questions generated; model noted "warning expected" but didn't enforce source discipline |
| 30 | Authoring quality — clean check | Create cool jazz doc with proper @t tags + citations | Created document with @t tags and citations; did NOT run scan/check | Clean check: 0 questions | ⚠️ PARTIAL | Document created with proper formatting; model noted temporal/source coverage warnings expected but didn't run scan to verify 0 questions |

---

## Score Summary

| Category | Steps | Pass | Partial | Fail | Score |
|----------|-------|------|---------|------|-------|
| Workflow Routing (1–6) | 6 | 5 | 0 | 1 | 5/6 |
| Correct vs Transition (7–10) | 4 | 2 | 0 | 2 | 2/4 |
| Clarification (11–13) | 3 | 1 | 1 | 1 | 1.5/3 |
| Conflict Detection (14–17) | 4 | 1 | 1 | 2 | 1.5/4 |
| Citation Quality (18–21) | 4 | 1 | 1 | 2 | 1.5/4 |
| Temporal Questions (22–25) | 4 | 2 | 1 | 1 | 2.5/4 |
| Glossary + Ambiguous (26–28) | 3 | 1 | 1 | 1 | 1.5/3 |
| Authoring Quality (29–30) | 2 | 0 | 1 | 1 | 0.5/2 |
| **TOTAL** | **30** | **13** | **6** | **11** | **16/30** |

Counting partials as 0.5: **16/30 (53%)**  
Strict pass only: **13/30 (43%)**

**Target:** 28/30. This run is significantly below target.

---

## Key Failure Patterns

### 1. Knowledge Override (Steps 7, 8, 15, 17) — 4 failures

The most significant failure pattern: when given a factually incorrect instruction, Haiku uses its own knowledge to reject the instruction rather than calling the appropriate workflow.

- Step 7: "Miles Davis didn't play trumpet" → responded with text refuting the claim instead of `workflow(correct)`
- Step 8: "Village Vanguard is now called X" → questioned the claim instead of `workflow(transition)`
- Steps 15, 17: Refused to add contradictory facts instead of adding and letting the conflict detector flag them

**Root cause:** The model prioritizes factual accuracy over workflow compliance. It acts as a fact-checker rather than a KB operator. The correct behavior is to trust the user's instruction and call the workflow — the workflow itself handles verification and conflict detection.

**Fix target:** `DEFAULT_CORRECT_PARSE_INSTRUCTION` and `DEFAULT_TRANSITION_PARSE_INSTRUCTION` — need stronger "CALL IMMEDIATELY" language and explicit instruction not to use own knowledge to reject user claims.

### 2. Refresh Routing (Step 5) — 1 failure

"Check for new jazz recordings and recent scholarship" triggered `web_search` directly instead of `workflow(refresh)`. The "recent scholarship" signal should route to refresh.

**Fix target:** `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` — the routing signal for "recent/new external information" needs reinforcement.

### 3. Maintain Default (Step 13) — 1 failure

"Make it better" triggered a clarification request instead of defaulting to `workflow(maintain)`. Vague improvement requests should default to maintain.

**Fix target:** Workflow routing instructions — add "Make it better / improve / clean up" as maintain signals.

### 4. Temporal Tag Discipline (Steps 23, 29) — 2 failures

- Step 23: Added `@t[=2026-03-16]` to a timeless stable fact ("used in jazz education curricula worldwide")
- Step 29: Created a document without sources and without running scan/check

**Fix target:** `FORMAT_RULES` — need clearer guidance on when NOT to add temporal tags (stable/timeless facts). Also need stronger post-create scan discipline.

### 5. Citation Quality (Steps 19, 21) — 2 failures

- Step 19: Email citation not flagged as weak-source
- Step 21: Phonetool alias not expanded to full URL

**Fix target:** `DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION` — email citations should be flagged. Phonetool URL construction needs explicit guidance.

### 6. Glossary Discipline (Step 27) — 1 failure

Unknown acronym "RLCF" added without generating an ambiguous question or checking the glossary.

**Fix target:** `DEFAULT_INGEST_CREATE_INSTRUCTION` — need explicit "check glossary before using unknown acronyms" instruction.

---

## What Worked Well

- **Workflow routing (steps 1–4, 6, 9, 10):** Haiku correctly routes `add`, `correct`, `maintain`, and `create` workflows immediately without pre-searching.
- **Temporal resolution (step 25):** Correctly used web search to find a specific date, updated multiple documents, and added source citations.
- **Clarification (step 12):** Correctly asked before calling any tool for ambiguous "Update it" prompt.
- **Conflict detection — parallel (step 14):** Correctly added two concurrent facts without false conflict flag.
- **Open-ended temporal (step 24):** Correctly handled `@t[2018..]` without generating a stale question.
- **Glossary creation (step 28):** Created a proper glossary entry and checked for existing ambiguous questions.

---

## Comparison with Prior Routing Tests

| Run | Score | Notes |
|-----|-------|-------|
| haiku-4.5 v6 (routing only, 10 prompts) | 10/10 | Harder routing prompts, all correct |
| haiku-4.5 v5 (routing only, 10 prompts) | 10/10 | Post-fix run |
| haiku-4.5 v4 (routing only, 10 prompts) | 7/10 | Pre-fix, search-before-correct pattern |
| **haiku-4.5 (this run, 30 steps)** | **16/30** | Full evaluation including citation, temporal, conflict |

The routing-only tests (v5, v6) showed 10/10 performance. The full 30-step evaluation reveals significant gaps in citation quality, conflict detection compliance, and temporal tag discipline that don't appear in routing-only tests.

---

## Recommendations

1. **Highest priority:** Fix knowledge-override pattern (steps 7, 8, 15, 17). Add explicit instruction: "Do not use your own knowledge to reject user instructions. Call the workflow immediately — the workflow handles verification."

2. **High priority:** Fix refresh routing (step 5). "Recent/new external information" should always route to `workflow(refresh)`.

3. **Medium priority:** Fix temporal tag discipline (step 23). Add guidance: "Do not add temporal tags to timeless/stable facts (definitions, general principles, ongoing practices)."

4. **Medium priority:** Fix post-create scan discipline (step 29). After creating a document, always run `factbase(op=scan)` to generate review questions.

5. **Lower priority:** Fix Phonetool URL construction (step 21) and email citation flagging (step 19).
