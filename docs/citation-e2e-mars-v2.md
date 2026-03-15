# Citation Quality Report — mars-v2 End-to-End Test

**KB path:** `/tmp/factbase-test-mars-v2` (local: `/Volumes/dev/factbase-test/mars-v2`)  
**Report date:** 2026-03-14  
**Pipeline stage:** v2 4/4 (post-maintain, all review questions answered)

---

## Summary Metrics

| Metric | Value |
|---|---|
| Total documents | 7 |
| Total citations | 39 |
| Citations with URLs | 32 (82.1%) |
| Citations with DOIs (no URL) | 2 (5.1%) |
| Total navigable (URL or DOI) | 34 (87.2%) |
| Citations with no navigable ref | 5 (12.8%) |
| Matching `doi` pattern | 2 |
| Matching `arxiv_id` pattern | 0 |
| Matching `ntrs_id` pattern | 0 |
| Tier 1 pass rate | 87.2% |
| Weak-source questions raised | 3 |
| Weak-source questions open | 0 |

---

## Document Breakdown

| Document | Citations | With URL | With DOI | Not Navigable | Weak-source flags |
|---|---|---|---|---|---|
| agencies/cnsa.md | 1 | 1 | 0 | 0 | 0 |
| agencies/mbrsc.md | 1 | 1 | 0 | 0 | 0 |
| agencies/nasa.md | 2 | 2 | 0 | 0 | 0 |
| missions/hope-probe.md | 6 | 3 | 1 | 2 | 2 |
| missions/ingenuity.md | 16 | 16 | 0 | 0 | 0 |
| missions/perseverance.md | 7 | 7 | 0 | 0 | 0 |
| missions/tianwen-1.md | 6 | 2 | 1 | 3 | 1 |
| **Total** | **39** | **32** | **2** | **5** | **3** |

---

## Non-Navigable Citations (5)

These citations have no URL and no recognized pattern identifier. All were flagged and answered during the maintain workflow with `needs-url` annotations.

### missions/hope-probe.md

- **[^3]** `EMM Science Team, Mission Science Overview, 2021`  
  → Answered: *needs-url — check https://www.emiratesmarsmission.ae or search for journal publication*

- **[^4]** `MBRSC mission extension announcement, 2023`  
  → Answered: *needs-url — add URL to MBRSC press release at https://www.mbrsc.ae*

### missions/tianwen-1.md

- **[^3]** `Andrew Jones, "Zhurong rover drives off lander onto Mars surface", SpaceNews, May 22, 2021`  
  → Not flagged as weak-source (named journalist + publication), but no URL present

- **[^4]** `CNSA mission update, 2022, via SpaceNews`  
  → Answered: *needs-url — add direct URL to SpaceNews article about Zhurong 1900m milestone*

- **[^6]** `Andrew Jones, "China's Zhurong Mars rover enters hibernation", SpaceNews, May 2022`  
  → Not flagged as weak-source (named journalist + publication), but no URL present

---

## Citation Pattern Coverage

Three patterns were defined in `perspective.yaml` during the `create` workflow:

| Pattern name | Regex | Matches found |
|---|---|---|
| `doi` | `10\.\d{4,}/\S+` | 2 |
| `arxiv_id` | `arXiv:\d{4}\.\d{4,5}` | 0 |
| `ntrs_id` | `NTRS[- ]\d{8,}` | 0 |

**DOI matches:**
- `hope-probe.md [^5]`: `10.1038/s41550-022-01617-8` — Deighan et al., Nature Astronomy, 2022
- `tianwen-1.md [^5]`: `10.1038/s41586-022-04685-y` — Li et al., Nature, 2022

Both are peer-reviewed journal articles; both are navigable via `https://doi.org/<doi>`.

---

## Tier 1 Pass Rate

**Tier 1** = citation has at least one navigable reference (URL or recognized pattern match).

- Pass: 34 / 39 = **87.2%**
- Fail: 5 / 39 = **12.8%**

The 5 failures are concentrated in two mission documents (hope-probe, tianwen-1). Three of the five were explicitly flagged by the `weak-source` reviewer and annotated with remediation hints. The remaining two (tianwen-1 [^3] and [^6]) are named-journalist citations that passed the weak-source heuristic but still lack URLs.

---

## Weak-Source Questions

3 weak-source questions were raised across the full pipeline. All 3 are answered (review queue: 0 open). None were dismissed — all were resolved as `needs-url`, meaning the citations are acknowledged as incomplete and annotated with suggested remediation paths inline in the document.

**Outcome:** The weak-source detector correctly identified the 3 most ambiguous citations. It did not flag the two named-journalist citations without URLs (tianwen-1 [^3], [^6]), which is a minor gap — those citations are verifiable in principle but not directly navigable.

---

## Were Citation Patterns Suggested During Create?

**Yes.** The `create` workflow produced a `perspective.yaml` with a `citation_patterns` block containing all three patterns (`doi`, `arxiv_id`, `ntrs_id`). This was generated based on the domain description ("Mars exploration missions, 2020–2026") and the expected source types (NASA technical reports, peer-reviewed science journals, arXiv preprints).

---

## Did the Patterns Help?

**Partially.** The `doi` pattern contributed meaningfully:
- It correctly identified 2 peer-reviewed journal citations as navigable even though they carry no URL.
- Without the pattern, both would have been counted as non-navigable, dropping the Tier 1 pass rate from 87.2% to 82.1%.

The `arxiv_id` and `ntrs_id` patterns had zero matches. This is expected for a KB focused on mission overviews and news coverage rather than technical preprints or NASA internal reports. The patterns are not harmful — they add no false positives — but they reflect an optimistic assumption about source diversity that the actual content did not bear out.

**Net assessment:** The pattern definitions were a net positive. The `doi` pattern provided real value. The other two patterns are appropriate for the domain and would activate if the KB were expanded with more technical content.

---

## Recommendations

1. **Resolve the 5 non-navigable citations.** Three already have `needs-url` hints inline. The two unnamed-URL SpaceNews articles (tianwen-1 [^3], [^6]) should also get URLs added — both are findable at spacenews.com.

2. **Tier 1 target: 100%.** With 5 fixes, the KB would reach full navigability. All 5 are resolvable with a short web search.

3. **Weak-source heuristic gap.** Named-journalist citations without URLs (tianwen-1 [^3], [^6]) were not flagged. Consider whether the heuristic should require a URL even for named sources, or add a separate `missing-url` check tier.

4. **arXiv / NTRS patterns:** Retain in perspective.yaml. They will activate if the KB is extended with instrument papers or mission design documents.
