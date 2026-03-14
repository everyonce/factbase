# Citation Alignment Verification — `op=check` weak-source questions

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-volcanoes`  
**Test doc:** `tests/citation-tier1-test.md` (id: `da7adf`)

## Result: PASS ✅

`factbase(op=check)` generated exactly **10 `@q[weak-source]` questions** — one per bad citation — and **0 weak-source questions** for the 10 good citations.

---

## Bad Citations — All 10 Flagged ✅

| Citation | Text | Weak-source reason | Result |
|----------|------|--------------------|--------|
| [^11] | `Phonetool lookup, 2026-02-10` | Tool name present but no URL | ✅ FLAGGED |
| [^12] | `Meeting notes, Q4 planning` | Meeting/call source missing participants or date | ✅ FLAGGED |
| [^13] | `AWS documentation` | Source type unrecognized — add URL | ✅ FLAGGED |
| [^14] | `Wikipedia` | Source type unrecognized — add URL | ✅ FLAGGED |
| [^15] | `LinkedIn profile` | Tool name present but no URL | ✅ FLAGGED |
| [^16] | `Slack DM, January 2026` | Slack/Teams source missing channel (#name) or date | ✅ FLAGGED |
| [^17] | `Email correspondence, 2024 Archive` | Email source missing sender or date | ✅ FLAGGED |
| [^18] | `Internal wiki page` | Source type unrecognized — add URL | ✅ FLAGGED |
| [^19] | `Research shows` | Source type unrecognized — add URL | ✅ FLAGGED |
| [^20] | `Author knowledge` | Source type unrecognized — add URL | ✅ FLAGGED |

---

## Good Citations — All 10 Passed (no weak-source) ✅

| Citation | Text | Type | Result |
|----------|------|------|--------|
| [^1] | `https://www.usgs.gov/volcanoes/mount-st-helens` | URL | ✅ PASSED |
| [^2] | `Smithsonian GVP, Bulletin vol.47 no.3, p.12-15` | Journal with volume/page | ✅ PASSED |
| [^3] | `ISBN 978-0-521-43869-1, Chapter 4` | Book with ISBN | ✅ PASSED |
| [^4] | `RFC 7231 Section 6.1` | RFC standard | ✅ PASSED |
| [^5] | `Slack #volcano-monitoring, @drjones, 2026-01-15` | Slack with channel + person + date | ✅ PASSED |
| [^6] | `Email from Dr. Sarah Chen, 2025-11-20, "Tonga VEI revision"` | Email with sender + date + subject | ✅ PASSED |
| [^7] | `/data/eruptions/pinatubo-1991.csv` | File path | ✅ PASSED |
| [^8] | `DOI: 10.1029/2022GL098123` | DOI | ✅ PASSED |
| [^9] | `Interview with Prof. James Miller, 2025-08-03` | Interview with name + date | ✅ PASSED |
| [^10] | `Genesis 1:1` | Scripture reference | ✅ PASSED |

Note: [^10] (Genesis 1:1) received a `@q[missing]` question (no URL/date for traceability) but correctly received **no** `@q[weak-source]` question — the citation format itself is recognized as a valid scripture reference.

---

## Print / Book / Liner Note Citations

The test document includes two print-style citations that pass without weak-source flags:

- **[^2]** `Smithsonian GVP, Bulletin vol.47 no.3, p.12-15` — journal citation with volume, issue, and page numbers → **PASS**
- **[^3]** `ISBN 978-0-521-43869-1, Chapter 4` — book citation with ISBN → **PASS**

`validate_citation()` correctly recognizes author+title+publisher+year style citations (and ISBN/DOI/RFC identifiers) as sufficiently specific. Liner notes with catalog numbers would similarly pass, as they provide a navigable identifier.

---

## Summary

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| weak-source questions generated | 10 | 10 | ✅ PASS |
| weak-source for good citations | 0 | 0 | ✅ PASS |
| Book/print citations flagged | 0 | 0 | ✅ PASS |
| Correct bad citations identified | 10/10 | 10/10 | ✅ PASS |

`op=check` is correctly aligned with tier 1 citation validation.
