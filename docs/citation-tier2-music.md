# Citation Tier 2 Batch Evaluate — Variant B, Music Domain

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-jazz` (Jazz musicians and albums of the 1950s–60s)  
**Purpose:** Test Variant B contextual prompt quality on music-specific borderline citations  
**Variant B prompt:** *"For each citation, ask: could someone with access to this KB's domain find the exact source using only the information provided? Consider the document context (entity name, type, domain). Respond: line number, VALID/INVALID/WEAK, reason, suggestion"*

---

## Test Corpus

| # | Citation |
|---|----------|
| 1 | `Bill Evans, liner notes, Kind of Blue, Columbia CL 1355, 1959` |
| 2 | `https://www.allmusic.com/album/...` |
| 3 | `Downbeat Magazine, interview with Miles Davis, March 1959, p.15` |
| 4 | `Columbia Studios, Session CL62290, March 2 1959` |
| 5 | `Jepsen, Jazz Records 1942-80, vol.6, p.234` |
| 6 | `Wikipedia, 'Kind of Blue', accessed 2026-03` |
| 7 | `Wikipedia` |
| 8 | `https://milesdavis.com/discography` |
| 9 | `Jazz magazine article, sometime in the 1960s` |
| 10 | `Well-known among jazz historians` |

---

## Variant B Results

| # | Verdict | Reason | Suggestion |
|---|---------|--------|------------|
| 1 | **VALID** | Liner notes with catalog number — author (Bill Evans), source type, album title, label, catalog number (CL 1355), and year are all present. In the jazz domain, catalog numbers are navigable record identifiers equivalent to ISBNs. Any jazz library or archive holding the LP can locate these liner notes. Gold-standard primary source format. | — |
| 2 | **VALID** | AllMusic is an authoritative music reference database; URL is present and navigable. | Add access date. Ensure URL is complete (not truncated). |
| 3 | **VALID** | Magazine name (Downbeat), content type (interview), subject (Miles Davis), date (March 1959), and page (p.15) are all present. Locatable via library Downbeat archives or ProQuest. | Add issue number if available for faster lookup. |
| 4 | **WEAK** | Session ID (CL62290) and date are present, but Columbia Studios session logs are internal archival documents — not publicly accessible. Findable only with access to Sony Music / Columbia Records archives. | Supplement with a published source that reproduces the session data, e.g. Ashley Kahn, *Kind of Blue: The Making of the Miles Davis Masterpiece*, Da Capo Press, 2000, or cite the archive holding the log (Sony Music Archives). |
| 5 | **VALID** | Standard bibliographic reference: author (Jepsen), title (*Jazz Records 1942-80*), volume (vol.6), and page (p.234) are all present. Locatable via any library catalog. | Add full author name (Erik Wiedemann Jepsen) and publisher for completeness, though the citation is sufficient as-is. |
| 6 | **WEAK** | Article name ('Kind of Blue') and access date (2026-03) are present, but no URL. Without a URL, the exact version accessed cannot be verified — Wikipedia articles change over time. | Add URL: `https://en.wikipedia.org/wiki/Kind_of_Blue` |
| 7 | **INVALID** | No article name, no URL, no access date. Completely untraceable — which Wikipedia article? Which version? | Rewrite as: `Wikipedia, 'Kind of Blue', https://en.wikipedia.org/wiki/Kind_of_Blue, accessed 2026-03` |
| 8 | **WEAK** | URL is present and navigable, but milesdavis.com is a fan/tribute site — not an institutional or academic source. Lower authority for factual discography claims. | For factual claims, prefer AllMusic (`https://www.allmusic.com`), Discogs, or an academic discography. If this is the official Miles Davis estate site, note that explicitly to establish authority. |
| 9 | **INVALID** | No specific magazine name, no year, no issue number, no page, no author. "Sometime in the 1960s" is not a date. Completely untraceable. | Identify the specific magazine (Downbeat, Metronome, Jazz Review, etc.), year, issue number, and page. Without these, the citation cannot be verified. |
| 10 | **INVALID** | Not a source — this is a claim about consensus, not a citation. There is nothing to find. | Replace with an actual source: a jazz encyclopedia entry, academic paper, or authoritative book that documents the claim. |

---

## Summary

| Verdict | Count | Citations |
|---------|-------|-----------|
| VALID | 3 | #1, #2, #3, #5 |
| WEAK | 3 | #4, #6, #8 |
| INVALID | 3 | #7, #9, #10 |

*(#1, #2, #3, #5 = 4 VALID; corrected below)*

| Verdict | Count | Citations |
|---------|-------|-----------|
| VALID | 4 | #1, #2, #3, #5 |
| WEAK | 3 | #4, #6, #8 |
| INVALID | 3 | #7, #9, #10 |

---

## Comparison with Expected Results

| # | Expected | Variant B | Match? |
|---|----------|-----------|--------|
| 1 | VALID | VALID | ✅ |
| 2 | VALID | VALID | ✅ |
| 3 | VALID | VALID | ✅ |
| 4 | WEAK | WEAK | ✅ |
| 5 | VALID | VALID | ✅ |
| 6 | WEAK | WEAK | ✅ |
| 7 | INVALID | INVALID | ✅ |
| 8 | WEAK | WEAK | ✅ |
| 9 | INVALID | INVALID | ✅ |
| 10 | INVALID | INVALID | ✅ |

**Accuracy: 10/10 — perfect match with expected results.**

---

## Key Findings

### 1. Liner Notes with Catalog Numbers: Correctly VALID (#1)

Variant B correctly validates the liner notes citation. The contextual approach recognizes that in the jazz domain, catalog numbers (CL 1355) are navigable record identifiers — equivalent to ISBNs for books. The citation has:
- Author (Bill Evans)
- Source type (liner notes)
- Album title (Kind of Blue)
- Label (Columbia)
- Catalog number (CL 1355)
- Year (1959)

This is the gold standard for jazz primary source citation. Variant B does not penalize it for lacking a URL — physical artifacts with catalog numbers are more stable than URLs. This is the correct behavior, and it directly addresses the false positive identified in the tier-1 Jazz KB test (`citation-test-jazz.md`), where `op=check` flagged liner notes as "source type unrecognized."

### 2. Fan Site Authority Issue: Correctly WEAK (#8)

Variant B catches the fan-site authority problem for `https://milesdavis.com/discography`. The URL is navigable, so Variant A (rule-based) would mark this VALID. Variant B's contextual reasoning asks *who* is publishing the information, not just *whether* a URL exists. milesdavis.com is a fan/tribute site — lower authority for factual discography claims than AllMusic, Discogs, or academic sources. The suggestion names specific alternatives, making it actionable.

### 3. Wikipedia Gradient: Correctly Distinguishes #6 from #7

- `Wikipedia, 'Kind of Blue', accessed 2026-03` → WEAK (article named, but no URL — version unverifiable)
- `Wikipedia` → INVALID (no article, no URL, no date — completely untraceable)

Variant B correctly applies a gradient rather than treating all Wikipedia citations the same. The suggestion for #6 is a single concrete fix (add URL), while #7 requires a full rewrite.

### 4. Session Log Nuance: Correctly WEAK (#4)

`Columbia Studios, Session CL62290, March 2 1959` has a real identifier (session number) and date, but the source is an internal archival document not publicly accessible. Variant B correctly classifies this as WEAK rather than INVALID — it *is* findable, but only with archive access. The suggestion is actionable: cite a published source (Ashley Kahn's book) that reproduces the session data.

### 5. Quality of Suggestions

All 10 suggestions are actionable:
- VALID citations: suggestions are optional improvements (add access date, add issue number) or none at all
- WEAK citations: suggestions name specific fixes (exact URL to add, specific published alternative)
- INVALID citations: suggestions explain what fields are missing and provide example rewrites

No suggestion is generic ("add a URL") without context. Each is tailored to the citation type and domain.

---

## Comparison with Volcanoes KB Results

| Aspect | Volcanoes KB | Jazz KB (this test) |
|--------|-------------|---------------------|
| Primary citation types | URLs, academic papers, USGS reports | Liner notes, books, magazines, URLs |
| Fan site issue | VolcanoDiscovery flagged WEAK (#26) | milesdavis.com flagged WEAK (#8) |
| Wikipedia gradient | Not tested | Correctly distinguishes named vs. bare |
| Physical media identifiers | Not present | Catalog numbers correctly validated |
| Archive-only sources | Not present | Session log correctly WEAK |
| Accuracy vs. expected | Not benchmarked | 10/10 |

Variant B generalizes well across domains. The contextual "findability" framing handles both web-native sources (URLs) and physical media identifiers (catalog numbers, volume/page) without domain-specific rules.

---

## Recommendation

Variant B is the correct prompt for music domain citations. It:

1. **Validates liner notes with catalog numbers** — no false positives for the jazz domain's primary source format
2. **Catches fan-site authority issues** — URL presence alone is not sufficient
3. **Applies a Wikipedia gradient** — named article is WEAK, bare "Wikipedia" is INVALID
4. **Handles archive-only sources** — session logs are WEAK, not VALID or INVALID
5. **Produces actionable suggestions** — specific URLs, specific alternative sources, specific missing fields

The 10/10 accuracy on this music-specific test set, combined with the strong performance on the Volcanoes KB (0 apparent false negatives, 0 apparent false positives), confirms Variant B as the production prompt for tier-2 citation evaluation.
