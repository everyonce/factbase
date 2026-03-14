# Factbase Workflow Test Report: Pacific Ring of Fire Volcanic Research KB

**KB path:** `/tmp/factbase-test-volcanoes`  
**Domain:** Volcanoes, eruption events, volcanic regions, monitoring agencies  
**Test date:** 2026-03-14  
**Final entity count:** 12 documents  

---

## Workflow Results

| Step | Workflow | Duration | Issues | Notes |
|---|---|---|---|---|
| 1 | **create** | ~1 min | None | Created KB with `perspective.yaml` defining 5 allowed types (volcano, eruption-event, volcanic-region, monitoring-agency, geological-concept). Initial commit included 9 documents: 3 volcanoes, 2 concepts, 1 region, 1 agency, plus model/DB artifacts. |
| 2 | **add** | ~8 min | None | Researched and added 3 new documents: `events/1991-pinatubo-eruption.md`, `events/2022-tonga-eruption.md`, `agencies/phivolcs.md`. Also enriched all 9 existing documents with review queues, temporal tags, and citations. Entity count: 9 → 12. |
| 3 | **maintain** | ~2 min | 1 minor | Ran scan + check. Generated 122 review questions across 12 docs (25 ambiguous, 26 stale, 18 temporal, 15 missing, 8 precision, 3 conflict). Minor: `links(suggest)` returned 0 suggestions despite clear cross-references between documents (e.g., Pinatubo ↔ PHIVOLCS ↔ 1991 eruption event). |
| 4 | **resolve** | ~5 min | 1 minor | Answered 95 questions (suppressed by prior answers in subsequent check). 27 items deferred to human review. Minor: some false-positive conflict questions (e.g., flagging simultaneous facts about the 1991 eruption as conflicting when they were simply concurrent). |
| 5 | **refresh** | ~3 min | None | Checked for recent updates on Tonga 2022 eruption VEI classification. Found updated scientific consensus (VEI 6, not 5). Prepared transition. |
| 6 | **transition** | ~1 min | None | Executed transition on `2022-tonga-eruption.md`: VEI reclassified from 5 → 6 effective 2026-03-14. Temporal tags correctly split (`@t[..2026-03-14]` / `@t[2026-03-14..]`). Transition history section added. Committed to KB. |

---

## Entity & Question Counts by Stage

| After Step | Entities | Review Questions | Answered | Deferred |
|---|---|---|---|---|
| 1 (create) | 9 | 0 | 0 | 0 |
| 2 (add) | 12 | ~122 (generated during add) | ~95 | ~27 |
| 3 (maintain) | 12 | 122 total | 95 suppressed | 27 |
| 4 (resolve) | 12 | 122 total | 95 | 27 |
| 5–6 (refresh/transition) | 12 | 122 total | 95 | 27 |

---

## Issues & Observations

**Minor issues:**
- `links(suggest)` returned 0 suggestions despite obvious cross-references (Pinatubo ↔ PHIVOLCS ↔ 1991 eruption). Semantic similarity scoring may not be tuned for short factbase documents with sparse prose.
- Conflict detection produced some false positives: concurrent facts about the 1991 Pinatubo eruption (e.g., "800 killed" and "global temperature drop") were flagged as conflicting because their temporal ranges overlapped, even though they describe independent phenomena.
- Entity types in the DB show as `volcanoe` and `agencie` (missing trailing 's') — likely a pluralization artifact from the `list` op, not a data integrity issue.

**Deferred items (27):** Mostly precision questions on well-established approximate figures (e.g., "~10 km³ ejected", "~800 killed") and stale-check questions on facts tagged `@t[~2024]`. All were appropriately deferred with `believed` answers explaining why approximations are acceptable.

---

## Overall Assessment

Factbase handled the volcanology domain well. Key observations:

- **Temporal tagging** worked correctly throughout — historical eruption dates, ongoing monitoring roles, and the transition workflow all produced clean `@t[...]` annotations.
- **Citation tracking** was thorough; the review system correctly flagged vague citations and missing sources.
- **Transition workflow** was the standout: the VEI reclassification for the Tonga eruption was handled cleanly with split temporal tags and a transition history section.
- **Review question quality** was high for ambiguous/missing/stale types. Conflict detection needs tuning for concurrent-but-independent facts.
- **Link suggestion** did not fire — this is a gap for a domain with clear entity relationships.

**Verdict: ✅ Factbase is well-suited for this domain.** The temporal annotation system, citation enforcement, and transition workflow are particularly valuable for a scientific KB where facts evolve (eruption classifications get revised, monitoring agencies change status, hazard assessments are updated).
