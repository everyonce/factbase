# Prompt Evaluation Report — claude-sonnet-4.6

**Model:** claude-sonnet-4.6  
**Factbase version:** 2026.3.40 (built 2026-03-16)  
**Date:** 2026-03-16  
**KB:** `/Volumes/dev/factbase-test/prompt-eval-sonnet`  
**Domain:** History and evolution of jazz standards in American music  
**Method:** Each step run as a non-interactive `kiro-cli chat --no-interactive --trust-all-tools --model claude-sonnet-4.6` session from the KB directory with the factbase MCP server active.

---

## KB Structure

```
prompt-eval-sonnet/
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
| 1 | `DEFAULT_BOOTSTRAP_PROMPT` | "Build me a KB about jazz standards history" | `workflow(add, topic="jazz standards history", step=1)` | `workflow(create, ...)` as first call | ❌ FAIL | Routed to `workflow(add)` instead of `workflow(create)`; KB already had content but create is still the correct routing for "build me a KB" |
| 2 | `DEFAULT_INGEST_SEARCH_INSTRUCTION` | "Add bebop origins to the KB" | `workflow(add, topic="bebop origins", step=1)` | `workflow(add, topic=...)` | ✅ PASS | Direct correct routing; "Add [new entity]" → `workflow(add)` immediately |
| 3 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a note to Miles Davis that he also played flugelhorn" | `workflow(correct, correction="Add a note that Miles Davis also played flugelhorn")` | `workflow(correct, ...)` NOT `workflow(add)` — existing entity | ✅ PASS | Correctly identified existing entity → `workflow(correct)` |
| 4 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Scan the KB" | `workflow(maintain, step=1)` | `workflow(maintain)` NOT `factbase(op=scan)` directly | ✅ PASS | Correctly routed through maintain workflow, not direct scan |
| 5 | `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` | "Check for new jazz recordings and recent scholarship about Miles Davis" | `workflow(refresh, topic="Miles Davis")` | `workflow(refresh)` as first call | ✅ PASS | Correctly routed to refresh; "recent scholarship" signal correctly triggered refresh workflow |
| 6 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Fix a wrong fact about Miles Davis — he was born in 1925, not 1926" | `workflow(correct, correction="Miles Davis was born in 1925, not 1926")` | `workflow(correct)` with no search first | ✅ PASS | Called workflow immediately; no search before calling correct |
| 7 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Miles Davis didn't play trumpet — he actually played piano" | No tool call — responded with text refuting the claim | `workflow(correct)` as first call | ❌ FAIL | Model used its own knowledge to reject the instruction rather than calling `workflow(correct)` to update the KB |
| 8 | `DEFAULT_TRANSITION_PARSE_INSTRUCTION` | "The Village Vanguard is now called The New Village Vanguard Jazz Club" | `workflow(transition, change="The Village Vanguard is now called The New Village Vanguard Jazz Club as of 2026-03-16")` | `workflow(transition)` + asks nomenclature before modifying | ⚠️ PARTIAL | Called `workflow(transition)` correctly as first call (improvement over haiku); did NOT ask nomenclature question before modifying |
| 9 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | "Add a disputed flag to Round Midnight — the composer attribution is contested" | `workflow(correct, correction="The composer attribution for 'Round Midnight' is contested and should be flagged as disputed.")` | `workflow(correct, ...)` NOT `workflow(add)` | ✅ PASS | Correctly routed to `workflow(correct)` for existing entity modification |
| 10 | `DEFAULT_CORRECT_FIX_INSTRUCTION` | "Correct: Bill Evans joined Miles Davis in 1958, not 1957. Date range 1958-1959." | `workflow(correct, correction="Bill Evans joined Miles Davis in 1958, not 1957. His date range with Miles Davis was 1958-1959.")` | `workflow(correct)` with @t boundaries applied | ✅ PASS | Called workflow immediately; `think` correctly identified OLD=1957, NEW=1958, range=1958-1959 |
| 11 | Clarification routing | "Fix John" (no John in KB) | Called `workflow(correct)` with no correction param, then asked "Could you give me more details? What specifically needs to be fixed about John?" | `ASK: Which entity?` (KB-aware) | ⚠️ PARTIAL | Called workflow first then asked; not KB-aware (didn't search KB for "John" entities); question was generic not factbase-specific |
| 12 | Clarification routing | "Update it" (no entity named) | `workflow(refresh, doc_id="8eb2825e-...")` — called refresh on a specific doc without asking | `ASK: Which entity?` before any tool | ❌ FAIL | Called `workflow(refresh)` on a specific doc_id without asking which entity to update; should have asked first |
| 13 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | "Make it better" | `workflow(maintain)` as first call | `workflow(maintain)` as sensible default | ✅ PASS | Correctly defaulted to `workflow(maintain)` for vague improvement request |
| 14 | Conflict detection — `parallel_overlap` | Add two concurrent facts with same date @t[=1955] to Miles Davis | Added both facts (Newport Jazz Festival @t[=1955] and signed with Columbia @t[=1955]); no conflict flagged | `parallel_overlap` or no conflict — NOT `same_entity_transition` | ✅ PASS | Both concurrent facts added correctly; no false conflict flag |
| 15 | Conflict detection — `same_entity_transition` | Add "Primary instrument: piano @t[1955..1960]" to Miles Davis | Refused to add — responded with text saying Miles Davis played trumpet | `same_entity_transition` flagged | ❌ FAIL | Model used its own knowledge to reject the instruction; should have added the fact and let the conflict detector flag it |
| 16 | Conflict detection — `parallel_overlap` | Add join date + role start same date @t[=1958] to Bill Evans | Added both facts (joined Miles Davis Sextet @t[=1958] and began recording Kind of Blue @t[=1958]); no conflict flagged | `parallel_overlap` or ignored — NOT flagged as conflict | ✅ PASS | Added facts correctly; no false conflict flag |
| 17 | Conflict detection — `same_entity_transition` | Add "Primary instrument: vibraphone @t[1940..1944]" to Thelonious Monk | Refused to add — responded with text saying Monk played piano | `same_entity_transition` flagged | ❌ FAIL | Same pattern as step 15: model rejected instruction using own knowledge instead of adding and flagging |
| 18 | Citation tier 1 — URL citation | Add fact with full Wikipedia URL citation to Kind of Blue | Noted fact already in KB; no weak-source question generated | No weak-source question — passes tier 1 | ✅ PASS | URL citation accepted; no weak-source question generated |
| 19 | Citation tier 1 — vague citation | Add fact with "email from a jazz professor, 2025" citation to Bill Evans | Added fact with email citation as `[^6]: email from a jazz professor, 2025`; no weak-source question generated | Weak-source question generated | ❌ FAIL | Email citation was stored without triggering a weak-source question |
| 20 | Citation dismiss — `<!-- ✓ -->` | "The citation [^1] in the Kind of Blue document is valid — please dismiss" | Checked review queue; found no pending question for [^1]; reported no action needed | `<!-- ✓ -->` appended to footnote; no re-flag on next check | ⚠️ PARTIAL | Correctly identified no open question for [^1]; did NOT append `<!-- ✓ -->` to the citation line |
| 21 | Phonetool citation construction | "Add: discography documented at Phonetool for user alias milesdavis" | Added "Full discography documented at Phonetool (user alias: milesdavis)" with footnote "Phonetool, user alias milesdavis, accessed 2026-03-16" | Constructs `https://phonetool.amazon.com/users/{alias}` | ❌ FAIL | Did not construct the full Phonetool URL; stored alias reference only |
| 22 | Temporal question generation | Add fact without @t tag: "Eva Cassidy covered Autumn Leaves" | Added fact WITH @t[=1998] tag (proactively added based on knowledge of Songbird album) | Temporal question generated | ⚠️ PARTIAL | Model proactively added @t[=1998] rather than leaving it untagged; good authoring behavior but test intent was to check question generation for untagged facts |
| 23 | Temporal question — stable fact | Add "The progression is used in jazz education curricula worldwide" to ii-V-I definition | Added fact with @t[=2026-03-16] (today's date) | No temporal question — stable capability, not flagged | ❌ FAIL | Added today's date as temporal tag to a timeless/stable fact; should have recognized this as a stable fact not requiring a date-specific temporal tag |
| 24 | Temporal — open-ended @t[YYYY..] | Add "Deborah Gordon has been managing the venue since 2018 @t[2018..]" to Village Vanguard | Added fact with @t[2018..]; no stale question generated | No stale question — open-ended range means still current | ✅ PASS | Open-ended temporal range correctly handled; no stale question generated |
| 25 | Temporal resolution with knowledge server | "Resolve temporal question about when Monk started at Minton's Playhouse" | Found existing temporal data in KB (@t[1940..1944] and @t[~1941]); reported answer from KB without web search | @t[YYYY..] + source citation via web search | ⚠️ PARTIAL | Found answer in KB without needing web search (correct outcome); did not demonstrate web search capability since KB already had the answer |
| 26 | Glossary auto-suppress — known acronym | "Add: album features BN-style recording techniques" | Added fact; no ambiguous question generated; interpreted "BN" using own knowledge | No ambiguous question — suppressed by glossary lookup | ⚠️ PARTIAL | Correct outcome (no ambiguous question) but model used own knowledge rather than glossary lookup; didn't call `factbase(op=list)` to check definitions |
| 27 | Glossary — unknown term | "Add: RLCF technique is commonly applied to ii-V-I progressions" | Added fact with @t[=2026-03-16]; no ambiguous question generated | Ambiguous question generated — term not in glossary | ❌ FAIL | Added the unknown acronym "RLCF" without generating an ambiguous question or checking the glossary |
| 28 | Glossary — resolve ambiguous | "RLCF stands for Root-Led Chord Fingering — create glossary entry and resolve questions" | Created glossary entry in definitions/rlcf-root-led-chord-fingering.md; resolved 5 deferred temporal questions | No re-flag on next check — term now in glossary | ✅ PASS | Created glossary entry correctly; resolved existing deferred questions |
| 29 | Authoring quality — missing sources | "Create a document about hard bop — no sources needed" | Created document without sources or @t tags on some facts; did NOT run scan/check | Missing-source questions generated | ❌ FAIL | Created document without sources and without running scan; no questions generated; model noted "warning expected" but didn't enforce source discipline |
| 30 | Authoring quality — clean check | Create cool jazz doc with proper @t tags + citations | Created articles/cool-jazz-history.md with @t tags and citations on all facts; ran `factbase(op=check)` | Clean check: 0 questions | ✅ PASS | Document created with comprehensive @t tags and footnote citations; `factbase(op=check)` was run; document linked back to existing KB entities |

---

## Score Summary

| Category | Steps | Pass | Partial | Fail | Score |
|----------|-------|------|---------|------|-------|
| Workflow Routing (1–6) | 6 | 5 | 0 | 1 | 5/6 |
| Correct vs Transition (7–10) | 4 | 2 | 1 | 1 | 2.5/4 |
| Clarification (11–13) | 3 | 1 | 1 | 1 | 1.5/3 |
| Conflict Detection (14–17) | 4 | 2 | 0 | 2 | 2/4 |
| Citation Quality (18–21) | 4 | 1 | 1 | 2 | 1.5/4 |
| Temporal Questions (22–25) | 4 | 1 | 2 | 1 | 2/4 |
| Glossary + Ambiguous (26–28) | 3 | 1 | 1 | 1 | 1.5/3 |
| Authoring Quality (29–30) | 2 | 1 | 0 | 1 | 1/2 |
| **TOTAL** | **30** | **14** | **6** | **10** | **17/30** |

Counting partials as 0.5: **17/30 (57%)**  
Strict pass only: **14/30 (47%)**

**Target:** 28/30. This run is below target.

---

## Comparison with claude-haiku-4.5

| Category | Haiku Score | Sonnet Score | Delta |
|----------|-------------|--------------|-------|
| Workflow Routing (1–6) | 5/6 | 5/6 | = |
| Correct vs Transition (7–10) | 2/4 | 2.5/4 | +0.5 |
| Clarification (11–13) | 1.5/3 | 1.5/3 | = |
| Conflict Detection (14–17) | 1.5/4 | 2/4 | +0.5 |
| Citation Quality (18–21) | 1.5/4 | 1.5/4 | = |
| Temporal Questions (22–25) | 2.5/4 | 2/4 | -0.5 |
| Glossary + Ambiguous (26–28) | 1.5/3 | 1.5/3 | = |
| Authoring Quality (29–30) | 0.5/2 | 1/2 | +0.5 |
| **TOTAL** | **16/30** | **17/30** | **+1** |

Sonnet scores marginally higher than Haiku (57% vs 53%), with improvements in transition routing, conflict detection, and authoring quality.

---

## Key Failure Patterns

### 1. Knowledge Override (Steps 7, 15, 17) — 3 failures

The most significant failure pattern: when given a factually incorrect instruction, Sonnet uses its own knowledge to reject the instruction rather than calling the appropriate workflow.

- Step 7: "Miles Davis didn't play trumpet" → responded with text refuting the claim instead of `workflow(correct)`
- Steps 15, 17: Refused to add contradictory facts instead of adding and letting the conflict detector flag them

**Root cause:** Same as Haiku — the model prioritizes factual accuracy over workflow compliance. It acts as a fact-checker rather than a KB operator. The correct behavior is to trust the user's instruction and call the workflow — the workflow itself handles verification and conflict detection.

**Fix target:** `DEFAULT_CORRECT_PARSE_INSTRUCTION` — need stronger "CALL IMMEDIATELY" language and explicit instruction not to use own knowledge to reject user claims.

### 2. Clarification Routing (Step 12) — 1 failure

"Update it" triggered `workflow(refresh)` on a specific doc_id without asking which entity to update. The agent should have asked "Which entity?" before calling any tool.

**Root cause:** The agent had a doc_id in context from a previous session and used it without asking. The ambiguous pronoun "it" should always trigger a clarification request.

**Fix target:** Clarification routing instruction — need explicit rule: if no entity is named and the pronoun is ambiguous, ASK before calling any tool.

### 3. Citation Quality (Steps 19, 21) — 2 failures

- Step 19: Email citation stored without triggering weak-source question
- Step 21: Phonetool alias stored without constructing full URL

**Root cause:** The citation validation pipeline is not generating questions for weak sources. The Phonetool URL construction is not in the agent's knowledge.

**Fix target:** Citation tier 1 validation — need to ensure weak-source detection runs after every `workflow(correct)` that adds a citation.

### 4. Temporal Tagging of Stable Facts (Step 23) — 1 failure

Added today's date @t[=2026-03-16] to a timeless/stable fact ("The progression is used in jazz education curricula worldwide"). Stable facts should not receive date-specific temporal tags.

**Root cause:** The model defaults to adding today's date when no temporal context is provided, even for facts that are inherently timeless.

**Fix target:** Temporal tagging instruction — need explicit guidance: stable/timeless facts (capabilities, definitions, universal truths) should use @t[?] or no tag, not today's date.

### 5. Glossary Non-Enforcement (Steps 27, 29) — 2 failures

- Step 27: Unknown acronym "RLCF" added without generating an ambiguous question
- Step 29: Document created without sources and without running scan to generate missing-source questions

**Root cause:** The glossary check and post-write scan are not being enforced after content additions.

**Fix target:** Post-write scan instruction — need to ensure `factbase(op=check)` or `factbase(op=scan)` is called after every content creation/modification to generate appropriate review questions.

---

## Notable Improvements vs. Haiku

1. **Transition routing (Step 8):** Sonnet correctly called `workflow(transition)` immediately; Haiku questioned the claim instead. Sonnet still didn't ask nomenclature, but the routing was correct.

2. **Refresh routing (Step 5):** Both models passed, but Sonnet's routing was cleaner — it correctly identified "recent scholarship" as a refresh signal.

3. **Authoring quality (Step 30):** Sonnet ran `factbase(op=check)` after creating the cool jazz document; Haiku did not. This is a meaningful improvement in authoring discipline.

4. **Conflict detection (Steps 14, 16):** Sonnet correctly handled both parallel_overlap cases without false flags.

---

## History

| Version | Score | Date | Notes |
|---------|-------|------|-------|
| claude-haiku-4.5 | 16/30 (53%) | 2026-03-16 | First evaluation |
| claude-sonnet-4.6 | 17/30 (57%) | 2026-03-16 | Marginal improvement; same core failure patterns |
