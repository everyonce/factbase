# Workflow Routing Adversarial Test v9

**KB:** bible-facts  
**Date:** 2026-03-13  
**Model:** Sonnet  
**Purpose:** Multi-step requests, contradictory signals, cross-workflow chains.

---

## Results

### P1 — Chained refresh + correct
> "First refresh our Jericho content with recent discoveries, then correct any facts that are wrong"

**Routed to:** `workflow(refresh, topic='Jericho')`  
**Expected:** `workflow(refresh)` first  
**Pass:** ✅  
**Reasoning:** Refresh subsumes correction — it pulls in new external information and updates existing facts in one pass. No need to chain a separate `correct` call; refresh handles both discovery and fact reconciliation internally.

---

### P2 — Chained maintain + add
> "Scan for quality issues, then add missing entities that you find gaps for"

**Routed to:** `workflow(maintain)` first, then `workflow(add)` based on gap findings  
**Expected:** `workflow(maintain)` first  
**Pass:** ✅  
**Reasoning:** Maintain is the correct entry point — it scans, identifies gaps, and surfaces what's missing. Only after reviewing maintain output should `workflow(add)` be invoked for specific gaps identified.

---

### P3 — Nuanced factual correction
> "The KB says Solomon built the Temple — but scholars now say it was built by workers he conscripted, not him personally. Update it."

**Routed to:** `workflow(correct)`  
**Expected:** `workflow(correct)`  
**Pass:** ✅  
**Reasoning:** The original claim was always imprecise/wrong — Solomon didn't personally build it. This is a factual precision correction, not a transition (the entity didn't change, the attribution was always inaccurate). `correct` is right.

---

### P4 — Explicit correction with both dates
> "The Epistle of James was written in 48 AD — this is a correction from our previous incorrect date of 55 AD"

**Routed to:** `workflow(correct)`  
**Expected:** `workflow(correct)`  
**Pass:** ✅  
**Reasoning:** User explicitly states the prior date was incorrect — it was never true. The key test: "was the old value ever actually true?" → No. Use `correct`, not `transition`.

---

### P5 — Bulk source citation change
> "I want to transition the KB to use the New Revised Standard Version instead of Berean Standard Bible"

**Routed to:** `workflow(transition)`  
**Expected:** `workflow(transition)`  
**Pass:** ✅  
**Reasoning:** "Transition" keyword is present and semantically correct — the BSB citations were valid at the time, now the KB is moving to NRSV going forward. This is a systemic source change affecting many documents, fitting the transition pattern (old value was true, now superseded by a deliberate choice).

---

### P6 — Combined maintain + add (Dead Sea Scrolls)
> "Can you both check quality AND add new scholarship on the Dead Sea Scrolls?"

**Routed to:** `workflow(maintain)` first, then `workflow(refresh, topic='Dead Sea Scrolls')` for new scholarship  
**Expected:** `workflow(maintain)` first, not `workflow(add)` first  
**Pass:** ✅  
**Reasoning:** Quality check must come first. "New scholarship" on an existing topic is a refresh (updating existing docs), not an add (creating new entities). After maintain, `refresh` is more appropriate than `add` here since Dead Sea Scrolls content likely already exists in the KB.

---

### P7 — Vague "everything about Paul"
> "Everything in the KB about Paul needs to be reviewed and updated with modern scholarship"

**Routed to:** `workflow(refresh, topic='Paul')`  
**Expected:** `workflow(refresh, topic='Paul')`  
**Pass:** ✅  
**Reasoning:** "Updated with modern scholarship" is the key signal — this means pulling in new external information to update existing content. Refresh with a topic scope handles this precisely. Not `maintain` (no quality scan needed), not `add` (not creating new entities).

---

### P8 — Structural merge
> "We've decided to merge the Luke and Acts entries into a single Gospel+Acts entry"

**Routed to:** `factbase(op='organize', action='merge', source_id=<luke_id>, target_id=<acts_id>)`  
**Expected:** `factbase(organize, action=merge)` or `workflow(transition)`  
**Pass:** ✅  
**Reasoning:** This is a structural document operation — merging two KB entries into one. `factbase(op='organize', action='merge')` is the most direct and precise tool. `workflow(transition)` could work but is less precise for a document-level structural change.

---

### P9 — Adding a missing entity
> "Add Paul's letter to the Laodiceans — it's mentioned in Colossians 4:16 but we don't have it"

**Routed to:** `workflow(add, topic="Epistle to the Laodiceans")`  
**Expected:** `workflow(add)`  
**Pass:** ✅  
**Reasoning:** Truly new entity not yet in the KB. Even though it's referenced in Colossians, the KB doesn't have a document for it. `workflow(add)` researches and creates the new entry. Not `correct` (nothing to correct), not `refresh` (nothing to update).

---

### P10 — Name precision update
> "The Gospel of Mark is attributed to 'John Mark' in Acts — correct our entries to reflect this full name"

**Routed to:** `workflow(correct)`  
**Expected:** `workflow(correct)`  
**Pass:** ✅  
**Reasoning:** The author was always "John Mark" — the KB was using an imprecise/shortened form. The old value was never fully accurate. Key test: "was 'Mark' ever the correct full name?" → No, the full name was always John Mark. Use `correct`, not `transition`.

---

## Summary

| # | Prompt Signal | Routed To | Expected | Pass |
|---|--------------|-----------|----------|------|
| 1 | Chained refresh+correct | `workflow(refresh)` | `workflow(refresh)` | ✅ |
| 2 | Chained maintain+add | `workflow(maintain)` first | `workflow(maintain)` first | ✅ |
| 3 | Nuanced correction | `workflow(correct)` | `workflow(correct)` | ✅ |
| 4 | Explicit correction | `workflow(correct)` | `workflow(correct)` | ✅ |
| 5 | "transition" keyword | `workflow(transition)` | `workflow(transition)` | ✅ |
| 6 | Compound maintain+add | `workflow(maintain)` → `workflow(refresh)` | `workflow(maintain)` first | ✅ |
| 7 | Vague "everything" + scholarship | `workflow(refresh, topic='Paul')` | `workflow(refresh, topic='Paul')` | ✅ |
| 8 | Structural merge | `factbase(organize, merge)` | `factbase(organize)` or `workflow(transition)` | ✅ |
| 9 | Missing entity | `workflow(add)` | `workflow(add)` | ✅ |
| 10 | Name precision | `workflow(correct)` | `workflow(correct)` | ✅ |

**Score: 10/10**

---

## Key Routing Principles Validated

- **refresh subsumes correct** — never chain both; refresh handles fact reconciliation internally
- **maintain before add** — always scan quality first before creating new entities
- **correct vs transition** — ask "was the old value ever true?": No → correct; Yes, until a date → transition
- **refresh vs add** — updating existing content with new info → refresh; creating net-new entities → add
- **organize for structural ops** — document merges/splits go to `factbase(op='organize')`, not workflow
- **topic-scoped refresh** — vague "update everything about X" maps cleanly to `workflow(refresh, topic='X')`
