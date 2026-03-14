# Citation System Test: Jazz KB
**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-jazz` (Jazz musicians and albums of the 1950s–60s)  
**Test:** 3-tier citation validator behavior on music-domain citations

---

## KB Overview

- 9 documents total (musicians, albums, labels, styles)
- 32 cross-document links detected
- 60 review questions in queue (25 temporal, 17 stale, 3 precision, 3 ambiguous, 2 conflict)
- 6 citations flagged by the validator (out of 36 total: 30 specific, 6 vague)

---

## Citation Validator Results

### Citations Flagged (6 total)

| # | Document | Citation | Failure Reason | Assessment |
|---|----------|----------|----------------|------------|
| 1 | A Love Supreme | `Original liner notes, A Love Supreme, Impulse! Records, A-77, 1965` | "source type unrecognized" | **FALSE POSITIVE** |
| 2 | Modal Jazz | `Ira Gitler, liner notes, My Favorite Things, Atlantic Records, SD 1361, 1961` | "source type unrecognized" | **FALSE POSITIVE** |
| 3 | Miles Davis | `Nat Hentoff, liner notes, Sketches of Spain, Columbia Records, CL 1480 / CS 8271, 1960` | "source type unrecognized" | **FALSE POSITIVE** |
| 4 | Kind of Blue | `Bill Evans, liner notes, Kind of Blue, Columbia Records, CL 1355 / CS 8163, 1959` | "source type unrecognized" | **FALSE POSITIVE** |
| 5 | Blue Note Records | `Michael Cuscuna and Michel Ruppli, The Blue Note Label: A Discography, Greenwood Press, 2001` | "book/publication present but no page/chapter/section reference" | **WEAK** (legitimate) |
| 6 | Blue Note Records | `Richard Cook, Blue Note Records: The Biography, Justin, Charles & Co., 2003` | "source type unrecognized" | **FALSE POSITIVE** (it's a book) |

---

## Key Finding: Liner Notes Are a False Positive Category

**4 of 6 flagged citations are liner notes with catalog numbers.** These are the standard primary source format for jazz scholarship:

```
Bill Evans, liner notes, Kind of Blue, Columbia Records, CL 1355 / CS 8163, 1959
```

This citation format is:
- **Author** (liner notes writer)
- **Source type** ("liner notes")
- **Album title**
- **Label**
- **Catalog number** (the record ID — equivalent to a URL or ISBN)
- **Year**

The catalog number (`CL 1355 / CS 8163`) is a navigable record identifier — it uniquely identifies the physical artifact. The validator's "source type unrecognized" failure is incorrect: liner notes with catalog numbers are **more specific** than many web URLs (which can go dead) and are the gold standard for jazz citation.

**Recommendation:** The validator should recognize `liner notes` as a valid source type when accompanied by a catalog number. Pattern: `liner notes, <album>, <label>, <catalog_number>, <year>`.

---

## Book Citation Behavior

Citation #5 (Cuscuna/Ruppli discography) was correctly flagged as WEAK — it's a book with no page reference. This is appropriate behavior.

Citation #6 (Richard Cook biography) was flagged as "source type unrecognized" — but it's clearly a book (author, title, publisher, year). The validator should recognize `Author, Title, Publisher, Year` as a book citation pattern even without a page number, and flag it as WEAK rather than "unrecognized."

---

## Comparison with Volcanoes KB

| Aspect | Volcanoes KB | Jazz KB |
|--------|-------------|---------|
| Primary citation types | URLs, academic papers, USGS reports | Liner notes, books, URLs |
| False positive rate | Low (URLs recognized) | High (liner notes not recognized) |
| Domain-specific pattern | Volcano observatory URLs | Liner notes + catalog numbers |
| Book citations | Rare | Common |
| "Source type unrecognized" | Rare | 4/6 flagged citations |

The Jazz KB exposes a clear gap: the validator handles web-native sources well but doesn't recognize physical media citation formats (liner notes, catalog numbers).

---

## Stale Question Behavior on Historical KBs

The Jazz KB is a historical domain (1940s–1960s). The validator generated 17 stale questions and 25 temporal questions — many of which were inappropriate for a historical KB:

**Examples of over-eager stale/temporal questions:**
- "Artist: John Coltrane — when was this true?" (on A Love Supreme album doc)
- "Year: 1964 — when was this true?" (on A Love Supreme)
- "McCoy Tyner — piano — when was this true?"
- "Founders: Alfred Lion and Max Margulis — may be outdated, is this still accurate?"

These are **fixed historical facts** about a 1964 recording. The liner notes from 1965 are the authoritative primary source — flagging them as potentially stale because the source is from 1965 is a category error.

**Recommendation:** The validator should detect when a document is about a historical artifact (album, historical event) and suppress stale/temporal questions about immutable facts (personnel, year, structure). A heuristic: if the document's subject has a fixed end date (e.g., an album released in 1964), facts sourced from contemporaneous primary sources should not be flagged as stale.

---

## Resolve Summary

- 60 questions total in queue
- 44 resolved (verified: 29, believed: 15)
- 16 deferred (need human review — mostly conflict/believed answers)
- 0 unanswered remaining

All stale and temporal questions were confirmed as still accurate (historical facts don't change). Ambiguous questions (NJ, EMI, RIAA) were answered with definitions.

---

## Recommendations for Validator Improvement

1. **Add `liner notes` as a recognized source type** — valid when accompanied by catalog number (e.g., `CL 1355`, `SD 1361`, `A-77`). Catalog numbers are record IDs.

2. **Recognize `Author, Title, Publisher, Year` as a book citation** — flag as WEAK (missing page) rather than "source type unrecognized."

3. **Suppress stale/temporal questions on immutable historical facts** — album personnel, recording year, track structure, and similar facts sourced from contemporaneous primary sources should not be flagged as potentially stale.

4. **Historical KB detection** — when a KB's focus is a historical period (e.g., "1950s and 1960s jazz"), apply a more lenient staleness policy for facts about that period.
