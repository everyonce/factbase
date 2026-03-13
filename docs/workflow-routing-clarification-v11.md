# Workflow Routing Clarification Test — v11

**Date:** 2026-03-13  
**Model:** Sonnet  
**KB:** `/Users/daniel/work/bible-facts`  
**Harness instruction:** "You are operating in NON-INTERACTIVE mode. If you need to ask the user a clarifying question, respond with EXACTLY: 'ASK: <your question>' and nothing else. Otherwise, call the appropriate factbase workflow or tool as your first action."

---

## Results

| # | Prompt | Response | Correct? | Notes |
|---|--------|----------|----------|-------|
| 1 | "The letter to the Hebrews was written by Paul" | `workflow(correct)` called | ✅ CORRECT | Ran correction workflow; KB already correct (Hebrews not attributed to Paul). No false claim found. |
| 2 | "Make sure the KB is in good shape" | ERROR | ⚠️ N/A | Subagent failed with InternalServerError mid-stream. Expected: `workflow(maintain)`. Cannot score. |
| 3 | "Add research about the Dead Sea Scrolls" | `workflow(add)` called | ✅ CORRECT | Researched and expanded Dead Sea Scrolls document; created definitions file; 221 docs indexed. |
| 4 | "The Gospel of Mark has incorrect dates" | `workflow(correct)` step 1 called, then `ASK: What are the correct dates?` | ⚠️ PARTIAL | Tool call was made (correct routing), but stalled at step 1 asking for the correct dates rather than proceeding to investigate. Technically a tool call was the first action; mid-workflow clarification is defensible but suboptimal. |
| 5 | "Scan the repository" | `workflow(maintain)` called | ✅ CORRECT | Full maintain run: 221 docs scanned, 16 updated, 20 new links stored, 87 new questions found. |
| 6 | "Fix John" | `ASK: Which John did you mean? (Apostle, Baptist, John Mark, Gospel author?)` | ✅ CORRECT | Correctly identified multiple John entities and asked for disambiguation before acting. |
| 7 | "Update it" | `ASK: What topic or entity would you like me to update?` | ✅ CORRECT | No referent for "it" — correctly asked for clarification. |
| 8 | "That needs to be corrected" | `ASK: What specific entity or fact needs to be corrected, and what is the correct information?` | ✅ CORRECT | No referent for "that" — correctly asked for both entity and correct value. |
| 9 | "Fix the entry" | `ASK: Which entry needs to be fixed, and what is wrong with it?` | ✅ CORRECT | No referent for "the entry" — correctly asked for entity name and description of issue. |
| 10 | "The dates are wrong" | `ASK: Which entity or document has wrong dates?` | ✅ CORRECT (defensible) | Chose ASK over maintain-to-investigate. Either is defensible per spec. ASK is reasonable since no entity is named. |

---

## Score Summary

| Category | Expected | Result | Score |
|----------|----------|--------|-------|
| Clear prompts (P1–P5) | Tool call | P1 ✅, P2 ⚠️ error, P3 ✅, P4 ⚠️ partial, P5 ✅ | 3/4 scoreable (P2 errored) |
| Ambiguous prompts (P6–P10) | ASK | P6 ✅, P7 ✅, P8 ✅, P9 ✅, P10 ✅ | 5/5 |
| **Overall** | | | **8/9 scoreable** |

---

## Analysis

**Strengths:**
- All 5 ambiguous prompts correctly produced `ASK` responses with relevant, specific questions.
- P6 ("Fix John") showed good KB awareness — the agent identified multiple John entities rather than guessing.
- P1, P3, P5 routed cleanly to the correct workflows without hesitation.

**Issues:**
- **P2 (server error):** Infrastructure failure; not a routing failure. Re-run to score.
- **P4 (partial):** "The Gospel of Mark has incorrect dates" is a clear correction request — the entity and issue are both named. The agent correctly called `workflow(correct)` as its first action, but stalled at step 1 asking for the correct dates instead of proceeding to investigate what the KB currently says. The correction workflow is designed to look up the current state first; the agent should have continued through step 1 rather than asking mid-workflow.

**P10 note:** "The dates are wrong" — agent chose ASK. This is defensible: no entity is named, so there's nothing to investigate. A `workflow(maintain)` to scan for date issues would also be valid but less targeted.

---

## Recommendation

The clarification instruction is working well for ambiguous prompts (5/5). The main gap is P4: agents should be coached that when an entity is clearly named in a correction request, they should proceed through the workflow's investigation step rather than asking for the corrected value upfront — the workflow itself surfaces the current state and prompts for the correction.
