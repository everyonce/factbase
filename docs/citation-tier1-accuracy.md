# Citation Tier 1 Accuracy Test Results

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-volcanoes`  
**Test doc:** `tests/citation-tier1-test.md` (id: `da7adf`)  
**Tool:** `factbase(op=check)`

---

## Key Finding: No Tier 1 Citation Validator Exists

`factbase(op=check)` does **not** implement a citation traceability/tier validator. It generates three question types — `temporal`, `ambiguous`, and `stale` — none of which evaluate whether a source is traceable or sufficient. The expected 10-pass / 10-fail split based on citation quality did not occur.

---

## Raw Check Output Summary

- **Total questions generated:** 20
- **temporal:** 15
- **ambiguous:** 3 (GVP, VEI, DM acronyms)
- **stale:** 2

---

## Per-Citation Results

### Good Citations (Should Pass — No Citation Flag Expected)

| # | Citation | Questions Generated | Verdict |
|---|----------|---------------------|---------|
| 1 | `https://www.usgs.gov/volcanoes/mount-st-helens` | temporal (missing @t[]) | ❌ False positive |
| 2 | `Smithsonian GVP, Bulletin vol.47 no.3, p.12-15` | temporal + ambiguous (GVP) | ❌ False positive |
| 3 | `ISBN 978-0-521-43869-1, Chapter 4` | temporal (missing @t[]) | ❌ False positive |
| 4 | `RFC 7231 Section 6.1` | temporal (missing @t[]) | ❌ False positive |
| 5 | `Slack #volcano-monitoring, @drjones, 2026-01-15` | none | ✅ Pass |
| 6 | `Email from Dr. Sarah Chen, 2025-11-20, "Tonga VEI revision"` | none | ✅ Pass |
| 7 | `/data/eruptions/pinatubo-1991.csv` | temporal (missing @t[]) | ❌ False positive |
| 8 | `DOI: 10.1029/2022GL098123` | temporal + stale¹ | ❌ False positive |
| 9 | `Interview with Prof. James Miller, 2025-08-03` | none | ✅ Pass |
| 10 | `Genesis 1:1` | temporal (missing @t[]) | ❌ False positive |

**Good citations: 3 passed, 7 flagged (all for temporal/ambiguous, not citation quality)**

¹ The stale question misread the DOI prefix `10.1029` as the year `1029`, triggering a spurious "may be outdated" warning.

### Bad Citations (Should Fail — Citation Flag Expected)

| # | Citation | Questions Generated | Verdict |
|---|----------|---------------------|---------|
| 11 | `Phonetool lookup, 2026-02-10` | none | ❌ False negative |
| 12 | `Meeting notes, Q4 planning` | temporal | ⚠️ Wrong reason |
| 13 | `AWS documentation` | temporal | ⚠️ Wrong reason |
| 14 | `Wikipedia` | temporal | ⚠️ Wrong reason |
| 15 | `LinkedIn profile` | temporal | ⚠️ Wrong reason |
| 16 | `Slack DM, January 2026` | ambiguous (DM) | ⚠️ Wrong reason |
| 17 | `Email correspondence, 2024 Archive` | temporal + stale | ⚠️ Wrong reason |
| 18 | `Internal wiki page` | temporal | ⚠️ Wrong reason |
| 19 | `Research shows` | temporal | ⚠️ Wrong reason |
| 20 | `Author knowledge` | temporal | ⚠️ Wrong reason |

**Bad citations: 0 correctly flagged for citation quality, 1 completely missed, 9 flagged only for temporal coverage**

---

## Accuracy Summary

| Metric | Expected | Actual |
|--------|----------|--------|
| Good citations with no flags | 10 | 3 |
| Bad citations flagged for citation quality | 10 | 0 |
| False positives (good citations flagged) | 0 | 7 |
| False negatives (bad citations not flagged) | 0 | 10 |
| Citation-type questions generated | 10 | 0 |

**Result: 0/20 correct by the test's criteria. The validator being tested does not exist.**

---

## Why Citations 5, 6, 9 "Passed"

These three good citations happened to be attached to facts that already had `@t[...]` temporal tags in the document body, so the temporal checker had no complaint. Their pass is incidental — the check did not evaluate citation quality.

## Why Citation 11 (Phonetool) Was Completely Missed

`Phonetool lookup, 2026-02-10` contains a date (`2026-02-10`), which appears to satisfy the temporal checker when it scans the fact text. The fact line also had `@t[=2026-02-10]` in the document, so no temporal question was raised. The citation's lack of a URL or identifier went undetected.

---

## What `factbase(op=check)` Actually Checks

1. **temporal** — facts missing `@t[...]` tags (dynamic facts without date context)
2. **ambiguous** — acronyms or jargon needing definition
3. **stale** — citations with old dates that may need refreshing

It does **not** check:
- Whether a source name alone is sufficient (e.g., "Wikipedia", "AWS documentation")
- Whether a citation is traceable (URL, DOI, ISBN, page number present)
- Whether a source type is banned (e.g., "Author knowledge" used by an agent)
- Whether a tool reference has a URL (e.g., "Phonetool lookup")

---

## Recommendations

1. **Add a `citation` question type** to `op=check` that flags sources failing traceability rules:
   - Source name only, no URL/DOI/ISBN/page → flag
   - Banned patterns (`Author knowledge`, `Research shows`) → flag
   - Tool references without URL → flag
   - Slack DM without channel+author → flag
   - Meeting notes without participants+date → flag

2. **Fix the DOI stale false positive** — `10.1029` is a DOI registrant prefix, not a year. The stale checker should not parse DOI strings as dates.

3. **The authoring guide already documents the rules** (traceability requirements, banned sources) — they just aren't enforced by `op=check`.
