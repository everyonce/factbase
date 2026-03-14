# Workflow Routing Adversarial Test v8

**Date:** 2026-03-13  
**Model:** claude-sonnet (Sonnet)  
**Purpose:** Re-test v7 adversarial prompts after fix #523, plus new edge cases  
**KB:** bible-facts (220 documents)

## Results

| # | Prompt | First Call | Correct? | Notes |
|---|--------|-----------|----------|-------|
| 1 | "Add a note that the Epistle of James authorship is disputed" | `workflow(correct)` | ✅ | KEY FIX TEST PASSED. "Add a note to existing entity" correctly routed to correct, not add. |
| 2 | "Add some information about how the Gutenberg Bible influenced biblical scholarship" | `workflow(add)` | ✅ | New entity not in KB → add. Workflow(add) step 1 instructed to search first to confirm entity doesn't exist. |
| 3 | "Add a disputed flag to all entries where Pauline authorship is questioned" | `workflow(correct)` | ✅ | Bulk modification of existing entities → correct. "Add" keyword did not mislead. |
| 4 | "Can you add the word 'allegedly' before all claims about miracles?" | `workflow(correct)` | ✅ | Bulk edit to existing content → correct. Systemic scope. |
| 5 | "I want to add a section on archaeological evidence to the existing Jericho entry" | `workflow(correct)` | ✅ | "existing … entry" signal correctly triggered correct, not add. |
| 6 | "Add the dates of composition for each Gospel" | `workflow(correct)` | ✅ | Gospels are existing entities; adding missing facts → correct. |
| 7 | "Let's add a new category of entities: apocryphal books" | `factbase(perspective)` | ✅ | KB structure/type change → perspective op (expected: workflow(create) or factbase(perspective)). |
| 8 | "What does the KB say about the Dead Sea Scrolls?" | `factbase(get_entity)` | ✅ | Pure read-only. No workflow invoked. Entity not found in KB (404), but routing was correct — no workflow triggered. |
| 9 | "Our entry on the Book of Daniel has some incorrect dates — please scan and update" | `workflow(correct)` | ✅ | "scan and update" + specific entity with incorrect data → correct. Not routed to workflow(maintain). |
| 10 | "Add new details about Paul's missionary journeys from recent scholarship" | `workflow(refresh)` | ✅ | "recent scholarship" + updating existing entity → refresh. "Add" keyword did not mislead. |

## Summary

| Metric | Value |
|--------|-------|
| Total prompts | 10 |
| Correct | 10 |
| Wrong | 0 |
| Score | **10/10 (100%)** |

## P1 Critical Cases (add vs correct)

All 6 P1 cases (prompts 1, 3, 4, 5, 6, 9) where the correct answer was `workflow(correct)` despite "add" language in the prompt were routed correctly. Fix #523 appears effective.

## Key Signals That Drove Correct Routing

- **"add a note to X"** → existing entity annotation → `correct`
- **"existing … entry"** → explicit existing-entity signal → `correct`
- **"all entries where…"** → bulk modification of existing content → `correct`
- **"recent scholarship"** → external source update → `refresh`
- **"new category"** → structural/type change → `perspective`/`create`
- **"what does the KB say"** → read-only query → no workflow
- **Entity not in KB** (Gutenberg Bible) → `add`
