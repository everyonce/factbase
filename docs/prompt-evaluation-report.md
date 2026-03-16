# Prompt Evaluation Report — Comprehensive Agent-Facing Prompt Test Suite

**Date:** 2026-03-16  
**Domain:** History and evolution of jazz standards in American music  
**Test KB:** `experiments/jazz-prompt-eval/`  
**Purpose:** Structured evaluation of every agent-facing prompt, instruction, and description in factbase

---

## Test KB Structure

The jazz standards KB was designed to stress-test all prompt types:

```
experiments/jazz-prompt-eval/
├── perspective.yaml          # KB config with citation_patterns
├── standards/
│   ├── autumn-leaves.md      # Multi-composer, temporal facts, conflict potential
│   ├── all-the-things-you-are.md
│   └── round-midnight.md     # Disputed attribution (conflict scenario)
├── composers/
│   ├── miles-davis.md        # Multiple career phases (temporal overlap)
│   ├── thelonious-monk.md    # Retirement period (status change)
│   └── bill-evans.md         # Tragic event (transition scenario)
├── recordings/
│   └── kind-of-blue.md       # Catalog number citations (citation_pattern test)
├── venues/
│   └── village-vanguard.md   # Ownership transitions (transition scenario)
└── definitions/
    ├── modal-jazz.md          # Glossary entry (ambiguous question target)
    └── ii-v-i-progression.md  # Theory term (ambiguous question target)
```

**Why this domain:**
- Multiple entity types (standard, composer, recording, venue, definition)
- Time-sensitive facts spanning 1917–2024 (temporal prompts)
- Mix of internal/external sources including catalog numbers (citation prompts)
- Disputed attributions (conflict detection)
- Ownership transitions (transition workflow)
- Domain-specific citation format: record catalog numbers (citation_pattern)
- Abbreviations: ii-V-I, AABA, BN, CL, SD (ambiguous question triggers)

---

## Evaluation Table

| # | Instruction Targeted | Test Scenario | Expected Agent Action | Pass/Fail | Notes |
|---|---------------------|---------------|----------------------|-----------|-------|
| 1 | `DEFAULT_BOOTSTRAP_PROMPT` | "Design a KB for jazz standards history" | Returns JSON with 4 fields: document_types, folder_structure, templates, perspective. Templates use @t[YYYY] not @t[description]. Suggests catalog_number citation_pattern. | — | |
| 2 | `DEFAULT_SETUP_INIT_INSTRUCTION` | `workflow(create, step=1, domain='jazz standards', path='/tmp/jazz-kb')` | Creates directory, writes perspective.yaml stub, calls step=2 | — | |
| 3 | `DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION` | `workflow(create, step=2)` | Writes valid YAML to perspective.yaml with allowed_types, review.stale_days, citation_patterns for catalog numbers | — | |
| 4 | `DEFAULT_SETUP_VALIDATE_OK/ERROR` | `workflow(create, step=3)` | Validates YAML; on success calls step=4; on error shows ❌ and loops back | — | |
| 5 | `DEFAULT_SETUP_CREATE_INSTRUCTION` + FORMAT_RULES | `workflow(create, step=4)` | Calls factbase(op='authoring_guide') first. Creates 2-3 docs with @t[YYYY] tags (not @t[description]). Adds footnotes. | — | Key: FORMAT_RULES compliance |
| 6 | `DEFAULT_SETUP_SCAN_INSTRUCTION` | `workflow(create, step=5)` | Calls init_repository, then scan with time_budget_secs=120. Handles requires_confirmation gate. Handles continue:true paging. Calls check. | — | |
| 7 | `DEFAULT_CREATE_COMPLETE_INSTRUCTION` | `workflow(create, step=6)` | Summarizes what was created. Suggests add/maintain/refresh/improve next steps. | — | |
| 8 | `DEFAULT_INGEST_SEARCH_INSTRUCTION` | `workflow(add, topic='bebop origins')` | Calls search(query='bebop origins'). Also calls factbase(op='list'). Reports what exists. | — | |
| 9 | `DEFAULT_INGEST_RESEARCH_INSTRUCTION` | `workflow(add, step=2)` | Uses specific queries like "Charlie Parker bebop 1945". Cross-references 2+ sources. Notes sources with URLs/dates. | — | |
| 10 | `DEFAULT_INGEST_CREATE_INSTRUCTION` + FORMAT_RULES | `workflow(add, step=3)` | Uses bulk_create for multiple entities. Checks glossary before using "BN" abbreviation. Every fact has @t[YYYY] AND [^N]. Never uses "Author knowledge" as source. | — | Key: glossary check, source requirement |
| 11 | `DEFAULT_INGEST_VERIFY_INSTRUCTION` | `workflow(add, step=4)` | Calls factbase(op='check') with doc_ids=[...]. Fixes missing @t tags and sources NOW. Calls scan after fixes. | — | |
| 12 | `DEFAULT_INGEST_LINKS_INSTRUCTION` | `workflow(add, step=5)` | Calls factbase(op='links') twice (cross-type and same-type). Reviews suggestions. Calls links(action='store') for confirmed pairs. | — | |
| 13 | `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | `workflow(maintain, step=1)` | Calls factbase(op='scan') with time_budget_secs=120. Handles confirmation gate. Handles paging (continue:true loop). Records documents_total, temporal_coverage_pct. | — | |
| 14 | `DEFAULT_MAINTAIN_DETECT_LINKS_INSTRUCTION` | `workflow(maintain, step=2)` | Calls factbase(op='detect_links'). Handles paging. Records links_detected. | — | |
| 15 | `DEFAULT_MAINTAIN_CHECK_INSTRUCTION` | `workflow(maintain, step=3)` | Calls factbase(op='check'). Interprets breakdown (stale→aging, temporal→murky, missing→no evidence). Dismisses low-confidence temporal questions about stable facts. | — | Key: temporal filtering |
| 16 | `DEFAULT_MAINTAIN_RESOLVE_INSTRUCTION` | `workflow(maintain, step=4)` | Calls workflow(resolve, step=1) then loops step=2 until continue:false. Does NOT stop early. Calls step=3 to apply. Handles IO/body errors by splitting batches. | — | Key: loop compliance |
| 17 | `DEFAULT_MAINTAIN_LINKS_INSTRUCTION` | `workflow(maintain, step=5)` | Calls factbase(op='links') twice with different type filters. Stores confirmed links. | — | |
| 18 | `DEFAULT_MAINTAIN_ORGANIZE_INSTRUCTION` | `workflow(maintain, step=6)` | Calls factbase(op='organize', action='analyze'). Reviews merge/split/misplaced candidates. Uses factbase organize ops (NOT shell commands). Calls execute_suggestions. | — | Key: no shell commands |
| 19 | `DEFAULT_MAINTAIN_REPORT_INSTRUCTION` | `workflow(maintain, step=7)` | Writes structured report with all metrics. Mentions deferred items if any. Suggests catalog_number citation_pattern if flagged repeatedly. | — | |
| 20 | `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` + FORMAT_RULES | `workflow(refresh, topic='Miles Davis')` | Calls workflow as FIRST action (not search first). Reads entity, searches for latest info, updates @t[~YYYY-MM] tags, adds new facts with sources. | — | Key: CALL IMMEDIATELY |
| 21 | `DEFAULT_CORRECT_PARSE_INSTRUCTION` | `workflow(correct, correction='Miles Davis was born in 1925, not 1926')` | Calls workflow as FIRST action. Classifies as "factual". Identifies OLD=1926, NEW=1925. Generates search terms. Calls step=2. | — | Key: CALL IMMEDIATELY, no pre-search |
| 22 | `DEFAULT_CORRECT_SEARCH_INSTRUCTION` | `workflow(correct, step=2)` | Searches with both semantic and content modes. Collects all matching docs. Deduplicates. Reports with snippets. | — | |
| 23 | `DEFAULT_CORRECT_FIX_INSTRUCTION` | `workflow(correct, step=3)` | Reads each doc. Writes corrected text AS IF false claim never existed. NO "Note: X was wrong" disclaimers. NO parentheticals. Adds @t tag and source footnote. | — | Key: forbidden patterns |
| 24 | `DEFAULT_TRANSITION_PARSE_INSTRUCTION` | `workflow(transition, change='Village Vanguard ownership passed from Lorraine Gordon to Deborah Gordon in 2018')` | Calls workflow as FIRST action. Classifies as "role_change". OLD=Lorraine Gordon, NEW=Deborah Gordon. Effective date=2018. Calls step=2. | — | Key: correct vs transition distinction |
| 25 | `DEFAULT_TRANSITION_NOMENCLATURE_QUESTION` | `workflow(transition, step=2)` | Presents 4 nomenclature options. Waits for user choice before proceeding. | — | Key: interactive gate |
| 26 | `DEFAULT_TRANSITION_APPLY_INSTRUCTION` | `workflow(transition, step=4, nomenclature='Replace clean')` | Reads each doc. Entity overview gets temporal boundaries (@t[..2018] / @t[2018..]). Current refs get nomenclature update. Historical refs preserved. Footnote inserted BEFORE Review Queue section. | — | Key: footnote placement |
| 27 | `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (temporal type) | Resolve queue with temporal questions about jazz facts | Answers include @t[YYYY] tag in response. Uses ranges @t[YYYY..YYYY] not @t[=YYYY..=YYYY]. Cites external source. Does NOT answer "well-known historical fact". | — | Key: = prefix rule |
| 28 | `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (ambiguous type) | Resolve queue with ambiguous question about "BN" abbreviation | Step 1: Creates/updates definitions/bn-blue-note.md with "BN: Blue Note Records — jazz record label". Step 2: Answers "Defined in [doc_id]: BN = Blue Note Records". Does NOT just answer without creating entry. | — | Key: glossary-first rule |
| 29 | `DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION` | Resolve queue with weak-source questions about catalog numbers | Evaluates each citation: catalog numbers like "BN 1595" are VALID per domain citation_pattern. Responds VALID/INVALID/WEAK per citation. Calls step=2 with triage_results. | — | Key: citation_pattern awareness |
| 30 | `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (conflict type) | Conflict question: "Round Midnight — Monk vs Williams co-composer credit" | Reads [pattern:...] tag. Calls factbase(op='get_entity') on referenced doc for context. Resolves based on pattern type (not a real conflict if parallel_overlap). | — | Key: pattern tag reading |

---

## Evaluation Criteria (Per Step)

For each step, rate on 4 dimensions:

1. **Routing** — Did the agent call the right workflow/op?
2. **Instruction compliance** — Did it follow the specific instruction text?
3. **Output quality** — Was the output correct and well-formed?
4. **Anti-pattern avoidance** — Did it avoid the forbidden behaviors called out in the prompt?

Score: Pass / Partial / Fail

---

## Anti-Pattern Checklist

These are the most common failure modes the evaluation targets:

| Anti-Pattern | Tested In Step | Prompt Source |
|---|---|---|
| @t[description] instead of @t[YYYY] | 5, 10, 27 | FORMAT_RULES |
| @t[=YYYY..=YYYY] range syntax | 27 | FORMAT_RULES, resolve answer intro |
| Searching before calling correct/transition/refresh | 20, 21, 24 | workflow schema CALL IMMEDIATELY |
| Answering "well-known fact" without source | 27 | resolve answer intro evidence requirement |
| Answering ambiguous without creating glossary entry | 28 | resolve answer intro ambiguous section |
| Writing "Note: X was wrong" in corrections | 23 | correct fix forbidden patterns |
| Using shell commands (rm/mv) for KB operations | 18 | maintain organize instruction |
| Stopping resolve loop early | 16 | maintain resolve instruction |
| Footnote after Review Queue section | 26 | transition apply instruction |
| Vague source like "Wikipedia" without URL | 10 | ingest create source requirement |
| Not handling continue:true paging | 13, 14 | maintain scan/detect_links |

---

## How to Run This Evaluation

1. Set up a fresh factbase instance pointing to `experiments/jazz-prompt-eval/`
2. Connect an agent (Claude Sonnet, Opus, or Haiku) via MCP
3. Execute each test scenario in order
4. Record the agent's actual tool calls and outputs
5. Score each step against the criteria above
6. Fill in the Pass/Fail column and Notes

### Suggested Test Harness Prompt

Prepend this to each test scenario:

> "You are being evaluated on your use of factbase workflows. Follow all instructions exactly as given. Do not skip steps or take shortcuts."

### Regression Testing

Run this suite after any change to:
- `src/mcp/tools/workflow/instructions.rs`
- `src/mcp/tools/schema.rs`
- Any workflow step dispatch logic in `src/mcp/tools/workflow/mod.rs`

A regression is any step that previously passed and now fails.

---

## Coverage Map

| Prompt Constant | Steps Covered |
|---|---|
| `DEFAULT_BOOTSTRAP_PROMPT` | 1 |
| `DEFAULT_SETUP_INIT_INSTRUCTION` | 2 |
| `DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION` | 3 |
| `DEFAULT_SETUP_VALIDATE_OK/ERROR_INSTRUCTION` | 4 |
| `DEFAULT_SETUP_CREATE_INSTRUCTION` | 5 |
| `DEFAULT_SETUP_SCAN_INSTRUCTION` | 6 |
| `DEFAULT_CREATE_COMPLETE_INSTRUCTION` | 7 |
| `DEFAULT_INGEST_SEARCH_INSTRUCTION` | 8 |
| `DEFAULT_INGEST_RESEARCH_INSTRUCTION` | 9 |
| `DEFAULT_INGEST_CREATE_INSTRUCTION` | 10 |
| `DEFAULT_INGEST_VERIFY_INSTRUCTION` | 11 |
| `DEFAULT_INGEST_LINKS_INSTRUCTION` | 12 |
| `DEFAULT_MAINTAIN_SCAN_INSTRUCTION` | 13 |
| `DEFAULT_MAINTAIN_DETECT_LINKS_INSTRUCTION` | 14 |
| `DEFAULT_MAINTAIN_CHECK_INSTRUCTION` | 15 |
| `DEFAULT_MAINTAIN_RESOLVE_INSTRUCTION` | 16 |
| `DEFAULT_MAINTAIN_LINKS_INSTRUCTION` | 17 |
| `DEFAULT_MAINTAIN_ORGANIZE_INSTRUCTION` | 18 |
| `DEFAULT_MAINTAIN_REPORT_INSTRUCTION` | 19 |
| `DEFAULT_REFRESH_RESEARCH_INSTRUCTION` | 20 |
| `DEFAULT_CORRECT_PARSE_INSTRUCTION` | 21 |
| `DEFAULT_CORRECT_SEARCH_INSTRUCTION` | 22 |
| `DEFAULT_CORRECT_FIX_INSTRUCTION` | 23 |
| `DEFAULT_TRANSITION_PARSE_INSTRUCTION` | 24 |
| `DEFAULT_TRANSITION_NOMENCLATURE_QUESTION` | 25 |
| `DEFAULT_TRANSITION_APPLY_INSTRUCTION` | 26 |
| `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (temporal) | 27 |
| `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (ambiguous) | 28 |
| `DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION` | 29 |
| `DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION` (conflict) | 30 |
| `FORMAT_RULES` | 5, 10, 20, 27 |
| workflow tool schema (routing rules) | 8, 20, 21, 24 |
| factbase tool schema (KB source of truth) | 8, 20 |
| search tool schema | 8, 22 |

**Not yet covered** (candidates for future steps):
- `DEFAULT_ENRICH_*` instructions (enrich workflow)
- `DEFAULT_IMPROVE_*` instructions (improve workflow)
- `DEFAULT_RESOLVE_APPLY_INSTRUCTION` (apply step)
- `DEFAULT_RESOLVE_VERIFY_INSTRUCTION` (verify step)
- `DEFAULT_RESOLVE_CLEANUP_INSTRUCTION` (cleanup step)
- `DEFAULT_TRANSITION_ORGANIZE_INSTRUCTION`
- `DEFAULT_TRANSITION_MAINTAIN_INSTRUCTION`
- `DEFAULT_TRANSITION_REPORT_INSTRUCTION`
- `DEFAULT_REFRESH_RESOLVE_INSTRUCTION`
- `DEFAULT_REFRESH_REPORT_INSTRUCTION`
- `DEFAULT_CORRECT_CLEANUP_INSTRUCTION`

---

## Results Log

*Fill in after running the evaluation.*

| # | Routing | Compliance | Quality | Anti-Pattern | Overall | Notes |
|---|---------|------------|---------|--------------|---------|-------|
| 1 | | | | | | |
| 2 | | | | | | |
| 3 | | | | | | |
| 4 | | | | | | |
| 5 | | | | | | |
| 6 | | | | | | |
| 7 | | | | | | |
| 8 | | | | | | |
| 9 | | | | | | |
| 10 | | | | | | |
| 11 | | | | | | |
| 12 | | | | | | |
| 13 | | | | | | |
| 14 | | | | | | |
| 15 | | | | | | |
| 16 | | | | | | |
| 17 | | | | | | |
| 18 | | | | | | |
| 19 | | | | | | |
| 20 | | | | | | |
| 21 | | | | | | |
| 22 | | | | | | |
| 23 | | | | | | |
| 24 | | | | | | |
| 25 | | | | | | |
| 26 | | | | | | |
| 27 | | | | | | |
| 28 | | | | | | |
| 29 | | | | | | |
| 30 | | | | | | |
