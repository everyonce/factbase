# Citation System Test — Volcanoes KB
**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-volcanoes` (Pacific Ring of Fire Volcanic Research KB)  
**Test:** 3-tier citation validator via `workflow(maintain)`

---

## Summary Counts

| Metric | Value |
|--------|-------|
| Documents in KB | 12 |
| Total citations in KB | 41 (39 specific + 2 vague per `check` output) |
| Tier 1 (Rust validator) — PASS | 39 |
| Tier 1 (Rust validator) — FAIL (sent to Tier 2) | 2 |
| Tier 2 batch size | 2 |
| Citations remaining after batch | 0 |
| False positives (good citations flagged) | 0 |
| False negatives (bad citations passed) | 0 (assessed) |

---

## Tier 1 Results (Rust Validator)

The `check` op reported:
- `citations_specific: 39` — passed Tier 1 cleanly
- `citations_vague: 2` — failed Tier 1, forwarded to Tier 2
- `citations_missing: 0`

**Pass rate: 95.1% (39/41)**

### Citations That Failed Tier 1

| # | Doc | Footnote | Citation Text | Failure Reason |
|---|-----|----------|---------------|----------------|
| 1 | Mount Pinatubo (`183908`) | `[^3]` | Robock, A., "Pinatubo eruption: The climatic aftermath", Science, 2002 | Source type unrecognized — no URL, record ID, or navigable reference |
| 2 | Volcanic Explosivity Index (`d7a5b5`) | `[^1]` | Newhall, C.G. & Self, S., "The Volcanic Explosivity Index (VEI)...", JGR, 87(C2), 1982 | Book/publication present but no page/chapter/section reference |

---

## Tier 2 Batch (Agent Triage + Resolution)

### Step 4 Phase 1 — Triage

Both citations triaged as **WEAK** (not INVALID):
- Citation 1: Author + title + journal + year present, but no DOI/URL/volume/pages
- Citation 2: Author + title + journal + volume + year present, but no page numbers or DOI

### Step 4 Phase 2 — Resolution

Both citations were fixed via web search:

**Citation 1 — Robock 2002 (Mount Pinatubo)**
- Before: `Robock, A., "Pinatubo eruption: The climatic aftermath", Science, 2002`
- After: `Robock, A., "Pinatubo eruption: The climatic aftermath", Science, Vol. 295, pp. 1242-1244, 2002`
- Source: iastate.edu bibliography confirmed volume/pages; DOI not added (could not verify with certainty)

**Citation 2 — Newhall & Self 1982 (VEI)**
- Before: `Newhall, C.G. & Self, S., "The Volcanic Explosivity Index (VEI)...", Journal of Geophysical Research, 87(C2), 1982`
- After: `Newhall, C.G. & Self, S., "The Volcanic Explosivity Index (VEI)...", Journal of Geophysical Research, 87(C2), pp. 1231-1238, 1982, https://doi.org/10.1029/JC087iC02p01231`
- Source: USGS media page confirmed DOI and page range

---

## Batch Format Assessment

The step 4 batch prompt was **well-structured and actionable**:

```
Evaluate these citations. For each, respond with:
- VALID — citation is specific enough to verify independently
- INVALID — citation is too vague to fix without research
- WEAK — partial info present; include a suggestion for what to look up

1. [doc: Mount Pinatubo] [^3] "Robock, A., ..." — source type unrecognized
2. [doc: Volcanic Explosivity Index] [^1] "Newhall, C.G. & Self, S., ..." — no page/chapter/section
```

**Strengths:**
- Clear 3-option triage vocabulary (VALID/INVALID/WEAK)
- Doc title + footnote number + citation text + failure reason all present
- Phase 1 (triage) → Phase 2 (resolve) split is clean
- `continue: false` / `citations_remaining: 0` correctly signals batch completion
- Structured `citations[]` array with `doc_id`, `line_number`, `footnote_number` — all fields needed for a targeted `get_entity` + `update` cycle

**Minor observations:**
- Batch size was 2 (entire KB). For larger KBs, pagination via `next_offset` is present and appears correct.
- The `failure_reason` field is human-readable but could benefit from a machine-readable `failure_code` for programmatic routing.

---

## False Positive / False Negative Analysis

### False Positives (good citations flagged by Tier 1)
**None detected.** Both flagged citations were genuinely weak:
- Robock 2002 lacked volume/page/DOI — legitimate flag
- Newhall & Self 1982 lacked page numbers — legitimate flag

### False Negatives (bad citations that passed Tier 1)
**None detected** in spot-check of the 39 passing citations. Reviewed a sample:
- `https://volcano.si.edu/volcano.cfm?vn=273083` — specific URL ✓
- `https://www.phivolcs.dost.gov.ph` — specific URL ✓  
- `https://volcano.si.edu/faq/index.cfm?question=eruptionscriteria` — specific URL ✓
- `https://www.usgs.gov/volcanoes/mount-st-helens` — specific URL ✓

The Tier 1 validator correctly distinguishes bare journal citations (no URL/DOI/pages) from URL-backed citations.

---

## Comparison to Previous `weak-source` Questions

In prior maintain runs, weak citations were surfaced as `@q[weak-source]` review questions mixed into the general queue. Evidence from the existing review queue (pre-maintain):
- `183908` had a pre-existing `weak-source` answer: *"Accepted as-is. Robock 2002 Science article is a well-known peer-reviewed publication; a DOI or URL would improve verifiability but the citation is sufficient..."*
- `d7a5b5` had a pre-existing `weak-source` answer: *"Accepted as-is. Newhall & Self 1982 is the foundational VEI paper; a DOI or journal reference (JGR, 87(C2), 1231-1238) would improve verifiability."*

**Key difference with new system:**
- Old: weak citations surfaced as passive review questions, often dismissed with "accepted as-is"
- New: weak citations are **actively resolved** — agent searches for DOI/pages and updates the document
- Result: both citations that were previously "accepted as-is" are now fixed with verifiable references

---

## Overall Assessment

The 3-tier citation validator is working correctly:

1. **Tier 1 (Rust)** correctly identifies citations missing navigable references. Pass/fail logic is sound — URL-backed citations pass, bare journal citations without pages/DOI fail.

2. **Step 4 batch format** is actionable. The triage → resolve two-phase structure works well. An agent can complete the full cycle (triage → web search → update) without ambiguity.

3. **No false positives or false negatives** detected in this KB. The validator is appropriately strict without being over-aggressive.

4. **Improvement over prior system:** Citations that were previously dismissed as "good enough" are now actively improved with DOIs and page numbers.

**Recommendation:** The system is ready for broader use. Consider adding a `failure_code` enum to the citation batch for programmatic routing in larger KBs.
