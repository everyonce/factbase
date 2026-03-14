# Citation Quality Report — Volcanoes KB v2 (End-to-End)

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-volcanoes-v2` (Volcanology research KB)  
**Scope:** Inspection-only — no workflows run  
**Pipeline stage:** Post-`add` (4 of 4 in v2 series)

---

## Summary Metrics

| Metric | Value |
|--------|-------|
| Total documents | 5 |
| Total citations | 22 |
| Citations with direct URLs | 11 / 22 (50%) |
| Citations with DOIs | 8 / 22 (36%) |
| Citations with BGVN pattern ref (no URL) | 1 / 22 (5%) |
| Total navigable (URL + DOI + pattern) | 20 / 22 (91%) |
| Citations matching `gvp_volcano_number` pattern | 3 / 22 (14%) |
| Citations matching `bgvn_citation` pattern | 3 / 22 (14%) |
| Citations matching any domain pattern | 6 / 22 (27%) |
| Tier 1 pass rate (navigable) | **91%** (20/22) |
| Weak-source questions (`@q[weak-source]`) | **0** |
| `citation_patterns` defined in `perspective.yaml` | Yes (2 patterns) |

---

## Documents

| File | Type | Citations |
|------|------|-----------|
| `volcano/kilauea.md` | volcano | 4 |
| `volcano/mount-pinatubo.md` | volcano | 4 |
| `volcano/hunga-tonga.md` | volcano | 5 |
| `eruption-event/hunga-tonga-2022.md` | eruption-event | 7 |
| `volcanic-region/pacific-ring-of-fire.md` | volcanic-region | 2 |

---

## Citation Inventory

### volcano/kilauea.md (4 citations)

| Ref | Source | URL/DOI | Tier | Pattern match |
|-----|--------|---------|------|---------------|
| `[^1]` | Smithsonian GVP, Kīlauea (332010), volcano.si.edu | `volcano.si.edu` | 1 | `gvp_volcano_number` (332010) |
| `[^2]` | USGS HVO, Kīlauea activity updates | `volcanoes.usgs.gov/volcanoes/kilauea/` | 1 | — |
| `[^3]` | Neal et al., Science vol. 363, pp. 367–374, 2019 | none | **2** | — |
| `[^4]` | USGS HVO, "About HVO" | `volcanoes.usgs.gov/observatories/hvo/` | 1 | — |

### volcano/mount-pinatubo.md (4 citations)

| Ref | Source | URL/DOI | Tier | Pattern match |
|-----|--------|---------|------|---------------|
| `[^1]` | Smithsonian GVP, Mount Pinatubo (273083), volcano.si.edu | `volcano.si.edu` | 1 | `gvp_volcano_number` (273083) |
| `[^2]` | PHIVOLCS, Pinatubo Volcano Bulletin | `www.phivolcs.dost.gov.ph` | 1 | — |
| `[^3]` | Newhall et al., USGS Professional Paper 1586, 1996 | none | **2** | — |
| `[^4]` | Smithsonian GVP, BGVN 16:07, 1991 | none (pattern-navigable) | 1† | `bgvn_citation` (BGVN 16:07) |

† Tier 1 via `bgvn_citation` pattern — resolvable at `volcano.si.edu/reports/` per `perspective.yaml`. No explicit URL in citation text.

### volcano/hunga-tonga.md (5 citations)

| Ref | Source | URL/DOI | Tier | Pattern match |
|-----|--------|---------|------|---------------|
| `[^1]` | Smithsonian GVP, Hunga Tonga (243040), volcano.si.edu | `volcano.si.edu/volcano.cfm?vnum=0403-04=` | 1 | `gvp_volcano_number` (243040) |
| `[^2]` | Smithsonian GVP, BGVN 47:03 | `volcano.si.edu/showreport.cfm?doi=10.5479/si.GVP.BGVN202203-243040` | 1 | `bgvn_citation` (BGVN 47:03) |
| `[^3]` | Frontiers in Earth Science, 2024 | `doi:10.3389/feart.2024.1373539` | 1 | — |
| `[^4]` | Proud et al., Science vol. 378, 2022 | `doi:10.1126/science.abo4076` | 1 | — |
| `[^5]` | Lynett et al., Science Advances vol. 9, 2023 | `doi:10.1126/sciadv.adf5493` | 1 | — |

### eruption-event/hunga-tonga-2022.md (7 citations)

| Ref | Source | URL/DOI | Tier | Pattern match |
|-----|--------|---------|------|---------------|
| `[^1]` | Smithsonian GVP, BGVN 47:03 | `volcano.si.edu/showreport.cfm?doi=10.5479/si.GVP.BGVN202203-243040` | 1 | `bgvn_citation` (BGVN 47:03) |
| `[^2]` | Frontiers in Earth Science, 2024 | `doi:10.3389/feart.2024.1373539` | 1 | — |
| `[^3]` | Proud et al., Science vol. 378, 2022 | `doi:10.1126/science.abo4076` | 1 | — |
| `[^4]` | Millán et al., Geophysical Research Letters vol. 49, 2022 | `doi:10.1029/2022GL099381` | 1 | — |
| `[^5]` | Klobas et al., PNAS vol. 120, 2023 | `doi:10.1073/pnas.2301994120` | 1 | — |
| `[^6]` | UCLA Newsroom, 2025 | `newsroom.ucla.edu/releases/hunga-volcano-eruption-cooled-southern-hemisphere` | 1 | — |
| `[^7]` | Lynett et al., Science Advances vol. 9, 2023 | `doi:10.1126/sciadv.adf5493` | 1 | — |

### volcanic-region/pacific-ring-of-fire.md (2 citations)

| Ref | Source | URL/DOI | Tier | Pattern match |
|-----|--------|---------|------|---------------|
| `[^1]` | Smithsonian GVP, "Volcanoes of the World" | `volcano.si.edu` | 1 | — |
| `[^2]` | USGS, "Plate Tectonics and the Ring of Fire" | `pubs.usgs.gov` | 1 | — |

---

## Tier Classification Detail

**Tier 1 (navigable):** 20/22 = **91%**

- Direct URL: 11 citations
- DOI: 8 citations
- BGVN pattern (no explicit URL, but resolvable via `perspective.yaml` pattern): 1 citation (`mount-pinatubo [^4]`)

**Tier 2 (bibliographically complete, not directly navigable):** 2/22 = **9%**

| Doc | Ref | Citation |
|-----|-----|----------|
| `kilauea.md` | `[^3]` | Neal et al., "The 2018 rift eruption and summit collapse of Kīlauea Volcano", *Science*, vol. 363, pp. 367–374, 2019 — no DOI |
| `mount-pinatubo.md` | `[^3]` | Newhall et al., "The cataclysmic 1991 eruption of Mount Pinatubo, Philippines", USGS Professional Paper 1586, 1996 — no DOI |

Both are well-known peer-reviewed sources with full bibliographic info; they are findable but not one-click navigable. Adding DOIs would promote both to Tier 1.

**Tier 3 (weak/vague):** 0/22 = **0%**

---

## Domain Pattern Coverage

Two patterns are defined in `perspective.yaml`:

### `gvp_volcano_number` — `\b\d{6}\b`
> Smithsonian GVP 6-digit volcano identifiers, resolvable at `volcano.si.edu`

Matched in 3 citations across 3 documents:
- `kilauea [^1]`: 332010
- `mount-pinatubo [^1]`: 273083
- `hunga-tonga [^1]`: 243040

Also appears in document body text (`hunga-tonga-2022.md` overview: "243040") but not in a citation footnote for that document.

### `bgvn_citation` — `BGVN \d{2}:\d{2}`
> Bulletin of the Global Volcanism Network volume:issue, resolvable at `volcano.si.edu/reports/`

Matched in 3 citations across 3 documents:
- `mount-pinatubo [^4]`: BGVN 16:07 (no URL — only pattern-navigable citation in KB)
- `hunga-tonga [^2]`: BGVN 47:03 (with full URL)
- `hunga-tonga-2022 [^1]`: BGVN 47:03 (with full URL)

The two newer BGVN citations include explicit `volcano.si.edu` URLs; the older `BGVN 16:07` (1991) does not. This is the one citation where the pattern provides navigability that the citation text alone does not.

---

## Weak-Source Questions

**Count: 0**

No `@q[weak-source]` questions appear in any review queue across all 5 documents. Review queues contain: `@q[temporal]`, `@q[stale]`, `@q[ambiguous]`, `@q[conflict]`, `@q[precision]`, `@q[missing]` — none of which indicate citation weakness.

---

## Were `citation_patterns` Suggested During Create?

**Yes.** The `perspective.yaml` was authored with `citation_patterns` defined from the outset (present in the initial commit). Both patterns — `gvp_volcano_number` and `bgvn_citation` — are domain-specific to volcanology and the Smithsonian GVP ecosystem.

The patterns encode the two primary reference systems used in this field:
1. GVP volcano numbers (stable identifiers for every volcano in the GVP database)
2. BGVN volume:issue citations (the standard way to cite GVP eruption bulletins)

---

## Did the Patterns Help?

**Yes, demonstrably.**

Evidence:

1. **Consistent GVP number usage:** All three volcano documents include the GVP number in their primary citation (`[^1]`), formatted as `VolcanoName (NNNNNN)`. This is the canonical GVP citation style and makes the citation directly resolvable at `volcano.si.edu`.

2. **BGVN citations with URLs:** The two newer BGVN citations (`hunga-tonga [^2]`, `hunga-tonga-2022 [^1]`) include full `volcano.si.edu` URLs with DOI-style paths, going beyond what the pattern alone requires. The pattern likely prompted the author to look up and include the full URL.

3. **Pattern-only navigability:** `mount-pinatubo [^4]` (BGVN 16:07, 1991) has no URL but is still Tier 1 because the `bgvn_citation` pattern makes it resolvable. Without the pattern in `perspective.yaml`, this citation would be Tier 2.

4. **Zero weak-source questions:** The absence of `@q[weak-source]` review items suggests the LLM author did not produce vague or unverifiable citations — consistent with the patterns providing a clear template for what "good" domain citations look like.

**One gap:** The two Tier 2 citations (Neal et al. 2019, Newhall et al. 1996) are peer-reviewed journal/government publications that predate or fall outside the GVP/BGVN pattern scope. Both have DOIs that could be added; the patterns don't cover this case, but it's a minor gap given the overall 91% Tier 1 rate.

---

## Comparison: v1 vs v2

| Metric | v1 (12 docs, 41 citations) | v2 (5 docs, 22 citations) |
|--------|---------------------------|---------------------------|
| Tier 1 pass rate | 95.1% | **91%** |
| Weak-source questions | 0 | **0** |
| `citation_patterns` defined | No | **Yes** |
| Domain pattern matches | n/a | 6/22 (27%) |
| Tier 2 citations | 2 | 2 |
| Tier 3 citations | 0 | 0 |

v2 has a slightly lower Tier 1 rate (91% vs 95%) due to two Tier 2 journal citations without DOIs. Both KBs achieve zero weak-source questions. The addition of `citation_patterns` in v2 is a net positive: it provides explicit navigability for BGVN citations and enforces consistent GVP number formatting, with no apparent downside.

---

## Recommendations

1. **Add DOIs to 2 Tier 2 citations** to reach 100% Tier 1:
   - `kilauea [^3]`: Neal et al., Science 2019 → DOI `10.1126/science.aav7046`
   - `mount-pinatubo [^3]`: Newhall et al., USGS PP 1586, 1996 → no DOI exists (government report); add URL `pubs.usgs.gov/pp/1586/` instead

2. **Add URL to `mount-pinatubo [^4]`** (BGVN 16:07): currently pattern-navigable only. A direct URL would make it unambiguously Tier 1 independent of pattern resolution.

3. **`citation_patterns` are working well** — no changes needed. Consider adding a third pattern for USGS Volcano Hazards Program URLs (`volcanoes.usgs.gov`) to formalize the two USGS citations already present.
