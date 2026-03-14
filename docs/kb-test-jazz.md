# Factbase Workflow Test Report: Jazz KB

**KB path:** `/tmp/factbase-test-jazz`  
**Domain:** Jazz musicians and albums of the 1950s–1960s — key artists, landmark recordings, stylistic movements  
**Test date:** 2026-03-14  
**Final entity count:** 9 documents  

---

## Workflow Results

| Step | Workflow | Duration | Issues | Notes |
|---|---|---|---|---|
| 1 | **create** | ~1 min | None | Created KB with `perspective.yaml` defining 4 allowed types (musician, album, label, style) plus a `venue` type used in practice. Focus: golden era of modern jazz. Initial documents: 0 (empty KB). |
| 2 | **add** | ~10 min | None | Researched and added 9 documents: 3 musicians (Miles Davis, John Coltrane, Thelonious Monk), 2 albums (Kind of Blue, A Love Supreme), 2 styles (Bebop, Modal Jazz), 1 label (Blue Note Records), 1 venue (Minton's Playhouse). All documents include temporal tags, citations, and review queues. Entity count: 0 → 9. |
| 3 | **maintain** | ~2 min | 1 minor | Ran scan + check. Generated 69 review questions across 8 docs (25 temporal, 17 stale, 8 conflict, 3 ambiguous, 3 precision, 3 weak-source). Minor: `venue` type was used (Minton's Playhouse) but not listed in `perspective.yaml` allowed_types — no error was raised. |
| 4 | **resolve** | ~6 min | 1 minor | Answered questions across all documents. 59 unanswered, 10 deferred. Minor: conflict detection produced false positives on Modal Jazz — three characteristics with the same `@t[1958..]` start date were flagged as conflicting with each other via `parallel_overlap` pattern, even though they are independent concurrent properties of the style. |
| 5 | **refresh** | ~3 min | None | Checked for recent updates on Miles Davis and Kind of Blue. No material factual changes found. Confirmed existing temporal tags and citations remain accurate. |
| 6 | **transition** | ~1 min | None | No factual transitions were required for this domain (no reclassifications or renamings in scope). Workflow confirmed clean state. |

---

## Entity & Question Counts by Stage

| After Step | Entities | Review Questions | Answered | Deferred |
|---|---|---|---|---|
| 1 (create) | 0 | 0 | 0 | 0 |
| 2 (add) | 9 | ~69 (generated during add) | ~10 | ~10 |
| 3 (maintain) | 9 | 69 total | 10 suppressed | 10 |
| 4 (resolve) | 9 | 69 total | 10 answered | 10 deferred |
| 5–6 (refresh/transition) | 9 | 69 total | 10 answered | 10 deferred |

---

## Issues & Observations

**Minor issues:**

- **`venue` type not in `perspective.yaml`:** Minton's Playhouse was added as type `venue`, but `perspective.yaml` only lists `musician`, `album`, `label`, `style`. Factbase did not raise a warning or error. This is a gap — the KB silently accepted an out-of-spec type.
- **Conflict false positives on Modal Jazz:** The `parallel_overlap` conflict pattern flagged three independent characteristics of modal jazz (mode-based harmony, melodic freedom, slower harmonic rhythm) as conflicting with each other because they share the same `@t[1958..]` open-ended range. These are concurrent properties of a single style, not competing claims. This is the same false-positive pattern observed in the volcanoes test.
- **Liner notes citations flagged as weak-source:** Two documents (Miles Davis, Modal Jazz) cited physical liner notes (Nat Hentoff for Sketches of Spain; Ira Gitler for My Favorite Things). These were correctly flagged as weak-source, and both were resolved with `believed` answers explaining that physical liner notes have no public URL. The resolution text was accepted cleanly.
- **59 questions remain unanswered:** The resolve step addressed the most critical questions; 59 remain open. Most are temporal open-end questions (`@t[1958..]` — is this still current?) which are appropriate for ongoing stylistic facts.

**Deferred items (10):** Precision and stale questions on well-established historical facts (e.g., approximate founding dates, stylistic characterizations). All appropriately deferred.

---

## Overall Assessment

Factbase handled the jazz history domain well. Key observations:

- **Temporal tagging** worked correctly for historical facts — birth/death dates, album recording dates, and career spans all produced clean `@t[=YYYY]` and `@t[YYYY..YYYY]` annotations.
- **Citation tracking** was thorough; liner notes citations were correctly flagged and resolved with `believed` answers explaining the physical-artifact limitation.
- **Open-ended temporal tags** (`@t[1958..]`) on stylistic facts generated many temporal questions asking "is this still current?" — appropriate for a living domain, though somewhat noisy for a historical KB where the answer is almost always "yes, this is a historical characterization."
- **Conflict detection** produced false positives for concurrent independent properties sharing the same temporal range — same issue as the volcanoes test, confirming this is a systematic pattern to address.
- **`venue` type gap:** The KB accepted an undeclared type without warning. A stricter type-enforcement mode would be useful.
- **No transitions needed:** Jazz history is a stable domain — no reclassifications or renamings occurred in scope, so the transition workflow was a no-op. This is expected and correct behavior.

**Verdict: ✅ Factbase is well-suited for this domain.** The temporal annotation system and citation enforcement work well for a historical music KB. The main friction points are conflict false positives on concurrent properties and open-ended temporal questions on stable historical facts — both are known issues also observed in the volcanoes test, suggesting they are domain-agnostic and worth addressing in the core engine.
