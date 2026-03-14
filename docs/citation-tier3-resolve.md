# Citation Tier 3 Resolve Loop Test — Mars KB

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-mars`  
**Purpose:** Test whether the tier 3 resolve loop can fix weak citations using web search tools.

---

## Setup

Five deliberately weak citations were submitted for resolution:

| # | Weak Citation | Expected Outcome |
|---|---------------|-----------------|
| 1 | "NASA press release about Perseverance landing" | FIXED with nasa.gov URL |
| 2 | "Wikipedia, Ingenuity helicopter" | FIXED with wikipedia.org URL |
| 3 | "SpaceX website, Starship Mars plans" | FIXED or DEFER |
| 4 | "Research paper about Jezero crater organics" | FIXED with DOI or journal URL |
| 5 | "Meeting with NASA team, early 2024" | DEFER (too vague) |

---

## Results

### Citation 1 — NASA press release about Perseverance landing

**Status: ✅ FIXED**  
**Tool used:** `brave_web_search` → query: `NASA Perseverance rover landing press release site:nasa.gov`  
**Fixed URL:** https://www.nasa.gov/news-release/touchdown-nasas-mars-perseverance-rover-safely-lands-on-red-planet/  
**Notes:** Direct hit on first search. Official NASA news release titled "Touchdown! NASA's Mars Perseverance Rover Safely Lands on Red Planet." Verifiable, canonical URL.

---

### Citation 2 — Wikipedia, Ingenuity helicopter

**Status: ✅ FIXED**  
**Tool used:** `brave_web_search` → query: `Wikipedia Ingenuity helicopter Mars URL`  
**Fixed URL:** https://en.wikipedia.org/wiki/Ingenuity_(helicopter)  
**Notes:** URL was constructable from the citation text alone (Wikipedia + subject name), confirmed by search. First result was the exact article. Matches expected outcome exactly.

---

### Citation 3 — SpaceX website, Starship Mars plans

**Status: ✅ FIXED**  
**Tool used:** `web_search` → query: `SpaceX Starship Mars colonization plans spacex.com`  
**Fixed URL:** https://www.spacex.com/humanspaceflight/mars/  
**Notes:** Found directly in search results as a spacex.com result with snippet "SpaceX is planning to launch the first Starships to Mars in 2026." Specific page on the SpaceX site dedicated to Mars human spaceflight. Verifiable.

---

### Citation 4 — Research paper about Jezero crater organics

**Status: ⚠️ PARTIAL FIX**  
**Tool used:** `web_search` → query: `Farley et al 2022 "Jezero crater" organics Science journal DOI`  
**KB citation text:** Farley et al., "Astrobiologically Relevant Sulfur Redox Chemistry on the Martian Surface", *Science*, 2022  
**Finding:** The exact title in the KB does not appear to match any real published paper. The actual Farley et al. 2022 *Science* paper is:

> Farley et al., "Overview and integration of the Mars 2020 Perseverance rover science results from the Jezero Crater floor campaign," *Science*, 2022. DOI: **10.1126/science.abo4856**

**Fixed URL:** https://www.science.org/doi/10.1126/science.abo4856  
**Notes:** The KB citation title appears to be fabricated or hallucinated. The closest real paper by Farley et al. in *Science* 2022 covers Jezero Crater floor results and is the primary Mars 2020 overview paper. Recommend flagging the title discrepancy as a `@q[weak-source]` issue in addition to adding the DOI. A second relevant paper (Sharma et al. 2023, *Nature*) covers organic-mineral associations more directly: https://www.nature.com/articles/s41586-023-06143-z

---

### Citation 5 — Meeting with NASA team, early 2024

**Status: ⛔ DEFERRED**  
**Tool used:** None (no search attempted — citation is inherently private/internal)  
**Notes:** Internal meeting records are not publicly searchable. No public NASA press release or document could be identified that corresponds to "a meeting with NASA team, early 2024." Deferred as expected.

---

## Summary

| # | Citation | Result | URL Found |
|---|----------|--------|-----------|
| 1 | NASA Perseverance landing press release | ✅ FIXED | https://www.nasa.gov/news-release/touchdown-nasas-mars-perseverance-rover-safely-lands-on-red-planet/ |
| 2 | Wikipedia, Ingenuity helicopter | ✅ FIXED | https://en.wikipedia.org/wiki/Ingenuity_(helicopter) |
| 3 | SpaceX Starship Mars plans | ✅ FIXED | https://www.spacex.com/humanspaceflight/mars/ |
| 4 | Jezero crater organics paper | ⚠️ PARTIAL | https://www.science.org/doi/10.1126/science.abo4856 (title mismatch in KB) |
| 5 | Meeting with NASA team, early 2024 | ⛔ DEFER | — |

**Fixed:** 3 clean, 1 partial (title discrepancy flagged)  
**Deferred:** 1  
**Total searches run:** 4 (`brave_web_search` ×2, `web_search` ×2)

---

## Observations

**Did the agent use search tools?** Yes — both `brave_web_search` and `web_search` were used. Brave was rate-limited (1 req/s on Free plan) after 2 calls; `web_search` was used for the remaining queries.

**Did it construct actual URLs?** Yes for citations 1–3. Citation 2 (Wikipedia) was constructable from the text alone and confirmed by search. Citations 1 and 3 required search to find the specific page.

**Quality of fixed citations:** High for 1–3 (real, canonical, verifiable URLs). Citation 4 is problematic — the DOI found is for the real Farley et al. paper, but the title stored in the KB is incorrect, suggesting the original citation was hallucinated.

**Time per citation (estimated):**
- Citation 1: ~3s (single search, direct hit)
- Citation 2: ~3s (single search, direct hit)
- Citation 3: ~4s (rate limit delay + fallback search)
- Citation 4: ~6s (two searches, title mismatch investigation)
- Citation 5: ~1s (no search needed, immediate defer)

---

## Recommendations

1. **Tier 3 resolve works well** for well-known public sources (NASA, Wikipedia, SpaceX). 3/4 searchable citations were resolved in one query each.
2. **Title hallucination is a real risk** — Citation 4 shows that a plausible-sounding paper title can be stored in the KB that doesn't match any real publication. The resolve loop should flag title mismatches, not just add a URL.
3. **Rate limiting** on Brave Free plan (1 req/s) is a practical constraint for batch resolution. Consider adding a small delay between searches or using a higher-tier plan.
4. **Internal/meeting citations** (Citation 5) should be pre-filtered before reaching tier 3 — they will always defer and waste a search slot.
5. **Partial fixes should be surfaced** — Citation 4 should not be marked FIXED; it needs human review of the title before the DOI is accepted.
