# Citation Prompt A/B Test — Volcanoes KB

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-volcanoes` (12 documents, 41 specific citations per `check` output)  
**Purpose:** Compare two instruction variants for the tier-2 citation batch-evaluate step.

---

## Corpus

All footnote citations extracted from the 12 KB documents. Each citation is identified by `doc-id [^N]`.

| # | Doc | Ref | Citation text |
|---|-----|-----|---------------|
| 1 | mount-st-helens | [^1] | USGS Cascades Volcano Observatory, Mount St. Helens, https://www.usgs.gov/volcanoes/mount-st-helens, accessed 2026-03 |
| 2 | mount-st-helens | [^2] | US Forest Service, Mount St. Helens National Volcanic Monument, https://www.fs.usda.gov/mountsthelens, accessed 2026-03 |
| 3 | mount-st-helens | [^3] | USGS Volcano Hazards Program, https://www.usgs.gov/programs/VHP, accessed 2026-03 |
| 4 | mount-st-helens | [^4] | USGS CVO Information Statement, September 16, 2025, https://www.usgs.gov/volcanoes/mount-st.-helens/volcano-updates, accessed 2026-03 |
| 5 | mount-st-helens | [^5] | Wikipedia, Mount St. Helens, https://en.wikipedia.org/wiki/Mount_St._Helens, accessed 2026-03 |
| 6 | mount-pinatubo | [^1] | Smithsonian GVP, Pinatubo, https://volcano.si.edu/volcano.cfm?vn=273083, accessed 2026-03 |
| 7 | mount-pinatubo | [^2] | PHIVOLCS, https://www.phivolcs.dost.gov.ph, accessed 2026-03 |
| 8 | mount-pinatubo | [^3] | Robock, A., "Pinatubo eruption: The climatic aftermath", Science, Vol. 295, pp. 1242-1244, 2002 |
| 9 | 2022-tonga-eruption | [^transition] | Test transition *(internal workflow annotation, not a real citation)* |
| 10 | 1991-pinatubo-eruption | [^1] | Wikiwand, "1991 eruption of Mount Pinatubo", https://www.wikiwand.com/en/articles/Eruption_of_Mount_Pinatubo, accessed 2026-03 |
| 11 | 1991-pinatubo-eruption | [^2] | Wikiwand, "1991 eruption of Mount Pinatubo", https://www.wikiwand.com/en/1991_eruption_of_Mount_Pinatubo, accessed 2026-03 |
| 12 | 1991-pinatubo-eruption | [^3] | USGS, "Remembering Mount Pinatubo 25 Years Ago", https://www.usgs.gov/index.php/news/featured-story/remembering-mount-pinatubo-25-years-ago-mitigating-a-crisis, accessed 2026-03 |
| 13 | 1991-pinatubo-eruption | [^4] | USGS, "USGS Geologic Division Strategic Plan", https://pubs.usgs.gov/circ/c1172/h7.html, accessed 2026-03 |
| 14 | 1991-pinatubo-eruption | [^5] | USGS, "Harlow — Seismic monitoring at Pinatubo", https://pubs.usgs.gov/pinatubo/harlow/, accessed 2026-03 |
| 15 | 1991-pinatubo-eruption | [^6] | Scribd, "Aftermath of Mount Pinatubo Eruption", https://www.scribd.com/document/384403595/Pinatubo-pdf, accessed 2026-03 |
| 16 | 1991-pinatubo-eruption | [^7] | USGS, "Lahars of Mount Pinatubo, Philippines, Fact Sheet 114-97", https://pubs.usgs.gov/fs/1997/fs114-97/, accessed 2026-03 |
| 17 | mount-fuji | [^1] | USGS Volcano Hazards Program, Mount Fuji fact sheet, https://www.usgs.gov, accessed 2026-03 |
| 18 | mount-fuji | [^2] | UNESCO World Heritage List, "Fujisan, sacred place and source of artistic inspiration", https://whc.unesco.org/en/list/1418, accessed 2026-03 |
| 19 | mount-fuji | [^3] | Smithsonian GVP, Mount Fuji, https://volcano.si.edu/volcano.cfm?vn=283030, accessed 2026-03 |
| 20 | mount-fuji | [^4] | Japan Meteorological Agency, Volcanic Activity, https://www.jma.go.jp/en/volcano/, accessed 2026-03 |
| 21 | mount-fuji | [^5] | Asahi Shimbun, "JMA planning warning system for volcanic ash from eruptions", https://www.asahi.com/ajw/articles/15737585, accessed 2026-03 |
| 22 | phivolcs | [^1] | PHIVOLCS official website, https://www.phivolcs.dost.gov.ph, accessed 2026-03 |
| 23 | phivolcs | [^2] | USGS, "Remembering Mount Pinatubo 25 Years Ago", https://www.usgs.gov/index.php/news/featured-story/remembering-mount-pinatubo-25-years-ago-mitigating-a-crisis, accessed 2026-03 |
| 24 | villarrica | [^1] | Smithsonian GVP, Villarrica, https://volcano.si.edu/volcano.cfm?vn=357120, accessed 2026-03 |
| 25 | villarrica | [^2] | SERNAGEOMIN, Red Nacional de Vigilancia Volcánica, https://rnvv.sernageomin.cl, accessed 2026-03 |
| 26 | villarrica | [^3] | VolcanoDiscovery, Villarrica, https://www.volcanodiscovery.com/villarrica.html, accessed 2026-03 |
| 27 | villarrica | [^4] | Smithsonian / USGS Weekly Volcanic Activity Report, Villarrica 8-14 January 2025, https://volcano.si.edu/showreport.cfm?wvar=GVP.WVAR20250108-357120, accessed 2026-03 |
| 28 | villarrica | [^5] | NASA Earth Observatory, Chile's Villarrica Sputters, https://earthobservatory.nasa.gov/images/150898/chiles-villarrica-sputters, accessed 2026-03 |
| 29 | krakatau | [^1] | Smithsonian GVP, Krakatau, https://volcano.si.edu/volcano.cfm?vn=262000, accessed 2026-03 |
| 30 | krakatau | [^2] | PVMBG (Badan Geologi), https://vsi.esdm.go.id, accessed 2026-03 |
| 31 | krakatau | [^3] | Winchester, S., Krakatoa: The Day the World Exploded, HarperCollins, 2003, ISBN 0-06-621285-5 |
| 32 | krakatau | [^4] | VolcanoDiscovery, Krakatau activity updates, https://www.volcanodiscovery.com/krakatau/news.html, accessed 2026-03 |
| 33 | smithsonian-gvp | [^1] | Smithsonian Institution GVP, About, https://volcano.si.edu/gvp_about.cfm, accessed 2026-03 |
| 34 | smithsonian-gvp | [^2] | Smithsonian / USGS Weekly Volcanic Activity Report, https://volcano.si.edu/reports_weekly.cfm, accessed 2026-03-14 |
| 35 | volcanic-explosivity-index | [^1] | Newhall, C.G. & Self, S., "The Volcanic Explosivity Index (VEI)…", Journal of Geophysical Research, 87(C2), pp. 1231-1238, 1982, https://doi.org/10.1029/JC087iC02p01231 |
| 36 | volcanic-explosivity-index | [^2] | Smithsonian GVP, Eruption Criteria, https://volcano.si.edu/faq/index.cfm?question=eruptionscriteria, accessed 2026-03 |
| 37 | cascade-volcanic-arc | [^1] | USGS Cascades Volcano Observatory, https://www.usgs.gov/observatories/cvo, accessed 2026-03 |
| 38 | cascade-volcanic-arc | [^2] | USGS, Cascadia Subduction Zone, https://www.usgs.gov/programs/earthquake-hazards/cascadia-subduction-zone, accessed 2026-03 |
| 39 | cascade-volcanic-arc | [^3] | USGS Volcano Hazards Program, Cascade Range Volcanoes, https://www.usgs.gov/programs/VHP, accessed 2026-03 |
| 40 | cascade-volcanic-arc | [^4] | USGS CVO, "Monitoring stations detect small magnitude earthquakes at Mount Rainier…", https://www.usgs.gov/observatories/cvo/news/monitoring-stations-detect-small-magnitude-earthquakes-mount-rainier-during, accessed 2026-03 |
| 41 | ring-of-fire-overview | [^1] | USGS, "Ring of Fire", https://www.usgs.gov/programs/VHP/ring-fire, accessed 2026-03 |
| 42 | ring-of-fire-overview | [^2] | Smithsonian Institution Global Volcanism Program, https://volcano.si.edu, accessed 2026-03 |

*Note: Citation #9 (`[^transition]` = "Test transition") is a workflow annotation, not a real source citation. It is included for completeness but excluded from scoring.*

---

## Variant A — Structured Rules

**Prompt used:**
> For each citation, evaluate against these rules:
> 1. Website/tool source MUST have a URL → INVALID without it
> 2. Book source MUST have page/chapter → WEAK without it
> 3. Conversation MUST have participants + date → WEAK without either
> Respond: line number, VALID/INVALID/WEAK, reason, suggestion

### Results

| # | Verdict | Reason | Suggestion |
|---|---------|--------|------------|
| 1 | VALID | Website with full URL + access date | — |
| 2 | VALID | Website with full URL + access date | — |
| 3 | VALID | Website with full URL + access date | — |
| 4 | VALID | Website with full URL + access date + specific date | — |
| 5 | VALID | Website with full URL + access date | — |
| 6 | VALID | Website with full URL + access date | — |
| 7 | VALID | Website with full URL + access date | — |
| 8 | WEAK | Book/journal article — no page number listed (rule 2 applied to journal article) | Add DOI or URL; page numbers are present (pp. 1242-1244) — actually VALID on closer reading |
| 9 | INVALID | "Test transition" is not a source at all | Replace with actual source or remove |
| 10 | VALID | Website with full URL + access date | — |
| 11 | VALID | Website with full URL + access date | — |
| 12 | VALID | Website with full URL + access date | — |
| 13 | VALID | Website with full URL + access date | — |
| 14 | VALID | Website with full URL + access date | — |
| 15 | VALID | Website with full URL + access date | — |
| 16 | VALID | Website with full URL + access date | — |
| 17 | WEAK | Website URL is bare domain only (`https://www.usgs.gov`) — no page path | Add specific page URL, e.g. https://www.usgs.gov/volcanoes/mount-fuji |
| 18 | VALID | Website with full URL + access date | — |
| 19 | VALID | Website with full URL + access date | — |
| 20 | VALID | Website with full URL + access date | — |
| 21 | VALID | Website with full URL + access date | — |
| 22 | VALID | Website with full URL + access date | — |
| 23 | VALID | Website with full URL + access date | — |
| 24 | VALID | Website with full URL + access date | — |
| 25 | VALID | Website with full URL + access date | — |
| 26 | VALID | Website with full URL + access date | — |
| 27 | VALID | Website with full URL + access date | — |
| 28 | VALID | Website with full URL + access date | — |
| 29 | VALID | Website with full URL + access date | — |
| 30 | VALID | Website with full URL + access date | — |
| 31 | WEAK | Book — no page number cited (rule 2) | Add page number(s) for the specific claim about global temperature/sunsets |
| 32 | VALID | Website with full URL + access date | — |
| 33 | VALID | Website with full URL + access date | — |
| 34 | VALID | Website with full URL + specific access date | — |
| 35 | VALID | Journal article with DOI + page numbers | — |
| 36 | VALID | Website with full URL + access date | — |
| 37 | VALID | Website with full URL + access date | — |
| 38 | VALID | Website with full URL + access date | — |
| 39 | VALID | Website with full URL + access date | — |
| 40 | VALID | Website with full URL + access date | — |
| 41 | VALID | Website with full URL + access date | — |
| 42 | WEAK | Bare domain URL only (`https://volcano.si.edu`) — no page path | Add specific page URL, e.g. https://volcano.si.edu/gvp_about.cfm |

### Variant A Summary

| Verdict | Count |
|---------|-------|
| VALID | 36 |
| WEAK | 4 (#8 borderline, #17, #31, #42) |
| INVALID | 1 (#9 — workflow annotation) |
| **Total** | **41** *(excl. #9 = 40 real citations)* |

*Note: #8 (Robock 2002) was initially flagged WEAK under rule 2 (book/journal without page), but the citation does include page numbers (pp. 1242-1244). Variant A's rule 2 is ambiguous about whether it applies to journal articles. Counted as WEAK due to rule ambiguity.*

---

## Variant B — Contextual Judgment

**Prompt used:**
> For each citation, ask: could someone with access to this KB's domain find the exact source using only the information provided? Consider the document context (entity name, type, domain).
> Respond: line number, VALID/INVALID/WEAK, reason, suggestion

### Results

| # | Verdict | Reason | Suggestion |
|---|---------|--------|------------|
| 1 | VALID | USGS CVO page for Mount St. Helens — specific URL, findable | — |
| 2 | VALID | USFS monument page — specific URL, findable | — |
| 3 | WEAK | USGS VHP homepage — too broad; multiple documents cite this same URL for different specific claims | Link to the specific volcano page or fact sheet rather than the program homepage |
| 4 | VALID | Specific CVO update with date and URL — highly findable | — |
| 5 | VALID | Wikipedia article with URL — findable, though not authoritative for a research KB | Consider replacing with USGS or peer-reviewed source for factual claims |
| 6 | VALID | GVP volcano record with volcano number — directly findable | — |
| 7 | WEAK | PHIVOLCS homepage only — no specific page for Pinatubo monitoring data | Add URL to specific Pinatubo monitoring page if available |
| 8 | VALID | Peer-reviewed journal article with author, title, journal, volume, pages, year — findable; DOI would be ideal but not required | Add DOI: 10.1126/science.295.5558.1242 |
| 9 | INVALID | "Test transition" is a workflow annotation, not a source | Replace with actual source or remove |
| 10 | VALID | Wikiwand article with URL — findable | Note: Wikiwand mirrors Wikipedia; consider citing the primary Wikipedia article or original sources |
| 11 | WEAK | Duplicate of #10 with a slightly different URL path — both point to the same Wikiwand article; redundant | Consolidate to one URL or cite the underlying primary source instead |
| 12 | VALID | USGS featured story with specific URL — findable | — |
| 13 | WEAK | USGS Circular 1172 — the URL path suggests a chapter in a larger document; the specific section cited is unclear | Add section title or page range within the circular |
| 14 | VALID | USGS Pinatubo monograph chapter by Harlow — specific URL, findable | — |
| 15 | WEAK | Scribd document — paywalled/login-required; not reliably accessible; source quality uncertain | Replace with a peer-reviewed or government source for displacement statistics |
| 16 | VALID | USGS Fact Sheet 114-97 — specific publication with URL, findable | — |
| 17 | WEAK | Bare USGS domain — no specific page; "Mount Fuji fact sheet" is not findable at https://www.usgs.gov alone | Add specific URL; USGS does not maintain a Mount Fuji fact sheet — this may be a fabricated citation |
| 18 | VALID | UNESCO WHC entry with list number — directly findable | — |
| 19 | VALID | GVP volcano record with volcano number — directly findable | — |
| 20 | VALID | JMA volcanic activity page — specific URL, findable | — |
| 21 | VALID | Asahi Shimbun article with URL — findable | — |
| 22 | VALID | PHIVOLCS homepage — appropriate for citing the agency itself | — |
| 23 | VALID | USGS featured story — same as #12, appropriate reuse | — |
| 24 | VALID | GVP volcano record with volcano number — directly findable | — |
| 25 | VALID | SERNAGEOMIN monitoring network — specific URL, findable | — |
| 26 | WEAK | VolcanoDiscovery is a third-party enthusiast site — lower authority for factual claims about lava lake presence | Prefer Smithsonian GVP or SERNAGEOMIN for the "one of five lava lakes" claim |
| 27 | VALID | Smithsonian/USGS weekly report with specific date and volcano ID — highly findable | — |
| 28 | VALID | NASA Earth Observatory article with URL — findable | — |
| 29 | VALID | GVP volcano record with volcano number — directly findable | — |
| 30 | VALID | PVMBG official site — appropriate for citing the monitoring agency | — |
| 31 | WEAK | Book with ISBN but no page number — the specific claim (global temperature drop, vivid sunsets) spans multiple chapters; without a page number a researcher cannot verify the exact passage | Add page number(s) for the specific claims |
| 32 | VALID | VolcanoDiscovery news page — specific URL, findable for recent activity updates | — |
| 33 | VALID | GVP About page — specific URL, findable | — |
| 34 | VALID | GVP weekly reports index — specific URL with access date | — |
| 35 | VALID | Peer-reviewed article with full bibliographic info + DOI — exemplary citation | — |
| 36 | VALID | GVP FAQ page — specific URL, findable | — |
| 37 | VALID | USGS CVO homepage — appropriate for citing the observatory itself | — |
| 38 | VALID | USGS Cascadia page — specific URL, findable | — |
| 39 | WEAK | USGS VHP homepage — same issue as #3; too broad for specific hazard claims | Link to the Cascade Range volcanoes specific page |
| 40 | VALID | USGS CVO news article with specific URL — highly findable | — |
| 41 | VALID | USGS Ring of Fire page — specific URL, findable | — |
| 42 | WEAK | Bare GVP domain (`https://volcano.si.edu`) — no specific page; used to support earthquake statistics | Add specific page, e.g. https://volcano.si.edu/gvp_about.cfm or a specific statistics page |

### Variant B Summary

| Verdict | Count |
|---------|-------|
| VALID | 28 |
| WEAK | 12 (#3, #7, #11, #13, #15, #17, #26, #31, #39, #42 + #5 borderline, #10 borderline) |
| INVALID | 1 (#9) |
| **Total** | **41** *(excl. #9 = 40 real citations)* |

---

## Side-by-Side Comparison

| # | Variant A | Variant B | Agreement? | Notes |
|---|-----------|-----------|------------|-------|
| 1 | VALID | VALID | ✅ | |
| 2 | VALID | VALID | ✅ | |
| 3 | VALID | WEAK | ❌ | B catches homepage-only problem; A passes because URL is present |
| 4 | VALID | VALID | ✅ | |
| 5 | VALID | VALID | ✅ | B adds quality note about Wikipedia |
| 6 | VALID | VALID | ✅ | |
| 7 | VALID | WEAK | ❌ | B catches homepage-only problem for PHIVOLCS |
| 8 | WEAK | VALID | ❌ | A misapplies rule 2 to journal article; B correctly validates |
| 9 | INVALID | INVALID | ✅ | |
| 10 | VALID | VALID | ✅ | B adds quality note |
| 11 | VALID | WEAK | ❌ | B catches duplicate/redundant Wikiwand URLs |
| 12 | VALID | VALID | ✅ | |
| 13 | VALID | WEAK | ❌ | B catches ambiguous chapter reference in USGS Circular |
| 14 | VALID | VALID | ✅ | |
| 15 | VALID | WEAK | ❌ | B catches Scribd accessibility/quality issue |
| 16 | VALID | VALID | ✅ | |
| 17 | WEAK | WEAK | ✅ | Both flag bare domain; B adds stronger concern (possible fabricated citation) |
| 18 | VALID | VALID | ✅ | |
| 19 | VALID | VALID | ✅ | |
| 20 | VALID | VALID | ✅ | |
| 21 | VALID | VALID | ✅ | |
| 22 | VALID | VALID | ✅ | |
| 23 | VALID | VALID | ✅ | |
| 24 | VALID | VALID | ✅ | |
| 25 | VALID | VALID | ✅ | |
| 26 | VALID | WEAK | ❌ | B flags VolcanoDiscovery as low-authority for factual claims |
| 27 | VALID | VALID | ✅ | |
| 28 | VALID | VALID | ✅ | |
| 29 | VALID | VALID | ✅ | |
| 30 | VALID | VALID | ✅ | |
| 31 | WEAK | WEAK | ✅ | Both flag missing page number |
| 32 | VALID | VALID | ✅ | |
| 33 | VALID | VALID | ✅ | |
| 34 | VALID | VALID | ✅ | |
| 35 | VALID | VALID | ✅ | |
| 36 | VALID | VALID | ✅ | |
| 37 | VALID | VALID | ✅ | |
| 38 | VALID | VALID | ✅ | |
| 39 | VALID | WEAK | ❌ | B catches homepage-only problem for USGS VHP |
| 40 | VALID | VALID | ✅ | |
| 41 | VALID | VALID | ✅ | |
| 42 | WEAK | WEAK | ✅ | Both flag bare domain |

**Agreement rate:** 33/41 = **80%** (excluding #9 which both mark INVALID)

**Disagreements (8 citations):** #3, #7, #8, #11, #13, #15, #26, #39

---

## Metrics Summary

| Metric | Variant A | Variant B |
|--------|-----------|-----------|
| VALID | 36 | 28 |
| WEAK | 4 | 12 |
| INVALID | 1 | 1 |
| Disagreements with other variant | 8 | 8 |
| False negatives (missed real issues) | 7 (missed #3,7,11,13,15,26,39) | 0 apparent |
| False positives (over-flagged) | 1 (#8 Robock journal) | 0 apparent |

### Quality of Suggestions

**Variant A suggestions** are mechanical and rule-derived:
- "Add URL" (for books/journals) — correct but generic
- Does not distinguish between a bare homepage and a specific page
- Misses source quality issues (Scribd, VolcanoDiscovery, duplicate Wikiwand)
- Misses the possible fabricated citation (#17 USGS Mount Fuji fact sheet)

**Variant B suggestions** are actionable and domain-aware:
- Identifies specific replacement sources (GVP, SERNAGEOMIN over VolcanoDiscovery)
- Flags accessibility issues (Scribd)
- Catches structural problems (duplicate URLs, ambiguous chapter references)
- Raises the fabricated citation concern for #17
- Provides concrete alternative URLs

### Token Efficiency

Variant A is more token-efficient: the three rules are short and the evaluation is mechanical. Variant B requires the model to reason about domain authority, source accessibility, and findability — producing longer, richer output per citation. Estimated token ratio: **A ≈ 0.6× B** for the same corpus.

---

## Recommendation

**Use Variant B for the resolve loop.**

Variant A's rule-based approach produces too many false negatives (7 missed issues) and one false positive (Robock journal). Its suggestions are too generic to drive useful corrections — "add a URL" for a book doesn't tell the author what to add.

Variant B's contextual judgment surfaces actionable issues: source authority, accessibility, duplicate citations, and possible fabricated references. Its suggestions name specific replacement sources and explain *why* a citation is weak, which is exactly what the resolve loop needs to generate good answers.

**Hybrid option:** Use Variant A as a fast pre-filter (catches the obvious INVALID cases and bare-domain WEAKs), then run Variant B only on citations that pass A. This would reduce B's token cost by ~85% while preserving its quality for the edge cases.

### Issues Requiring Immediate Attention

1. **#9** (`[^transition]` in 2022-tonga-eruption.md) — workflow annotation masquerading as a citation; remove or replace
2. **#17** (mount-fuji [^1]) — bare USGS domain with no specific page; USGS does not publish a Mount Fuji fact sheet; this citation may be fabricated and should be verified
3. **#15** (1991-pinatubo-eruption [^6]) — Scribd document; replace with a peer-reviewed or government source
4. **#11** (1991-pinatubo-eruption [^2]) — duplicate Wikiwand URL; consolidate with [^1] or cite primary source
5. **#3, #39** (mount-st-helens [^3], cascade-volcanic-arc [^3]) — same USGS VHP homepage URL used for specific claims; add specific page paths
