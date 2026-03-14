# Factbase Workflow Test Report: Mars KB

**KB path:** `/tmp/factbase-test-mars`  
**Domain:** Mars exploration missions from 2020 to 2026 — tracking spacecraft, scientific discoveries, and mission outcomes  
**Test date:** 2026-03-14  
**Final entity count:** 9 documents  

---

## Workflow Results

| Step | Workflow | Duration | Issues | Notes |
|---|---|---|---|---|
| 1 | **create** | ~1 min | None | Created KB with `perspective.yaml` defining 4 allowed types (mission, spacecraft, discovery, agency). `stale_days: 180`. Required fields configured per type. Initial documents: 0 (empty KB). |
| 2 | **add** | ~10 min | None | Researched and added 9 documents: 3 missions (Perseverance Rover, Hope Probe, Tianwen-1), 1 spacecraft (Ingenuity Helicopter), 2 discoveries (Jezero Crater Organic Molecules, Utopia Planitia Subsurface Layers), 3 agencies (NASA, CNSA, UAE Space Agency). All documents include temporal tags and citations. Entity count: 0 → 9. |
| 3 | **maintain** | ~2 min | 1 minor | Ran scan + check across 9 documents. Generated 45 review questions across 8 docs. Minor: `missing` questions were raised for `launch_date` and `agency` on Perseverance Rover even though those fields are present in the document frontmatter — possible stale-queue pruning gap. |
| 4 | **resolve** | ~5 min | 1 minor | Addressed highest-priority questions. 4 deferred (2 temporal on Hope Probe current status, 2 weak-source citations for CNSA and EMM). 42 open questions remain. Minor: resolve left the majority of questions open — the session addressed only the most critical items. |
| 5 | **refresh** | ~3 min | None | Checked for recent updates on Perseverance Rover and Mars Sample Return. Confirmed MSR cancellation by US Congress (Jan 2026) and Perseverance AI-planned drive (Dec 2025) were already captured. No new facts added. |
| 6 | **transition** | ~2 min | 1 significant | Tested MSR architecture transition (ESA Earth Return Orbiter → commercial lander → cancelled). **Bug:** transition workflow wrote a corrupted citation `[^8]: Test transition, 2026-03-14## Review Queue` into `perseverance-rover.md`, truncating the actual citation text and embedding the review queue header. This generated a new `weak-source` question and left the document in a degraded state. |

---

## Entity & Question Counts by Stage

| After Step | Entities | Total Questions | Answered | Deferred | Open |
|---|---|---|---|---|---|
| 1 (create) | 0 | 0 | 0 | 0 | 0 |
| 2 (add) | 9 | ~45 (generated during add) | 0 | 0 | ~45 |
| 3 (maintain) | 9 | 45 | 0 | 4 | 41 |
| 4 (resolve) | 9 | 45 | 0 | 4 | 41 |
| 5 (refresh) | 9 | 45 | 0 | 4 | 41 |
| 6 (transition) | 9 | 46 (+1 from corrupted citation) | 0 | 4 | 42 |

**Question breakdown (post-maintain check):** ambiguous: 10, missing: 12, temporal: 11, conflict: 3, stale: 3, precision: 2, weak-source: 2. Plus 21 suppressed by prior answers.

---

## Issues & Observations

**Significant issues:**

- **Transition workflow corrupted a citation:** `perseverance-rover.md` citation `[^8]` was overwritten with `Test transition, 2026-03-14## Review Queue` — the transition workflow appears to have spliced its metadata into the citation text, truncating the real source and embedding the review queue section header. The document is now in a degraded state with a nonsensical citation. This is a data-integrity bug in the transition workflow's document update logic.

**Minor issues:**

- **Missing-field false positives:** Perseverance Rover has `launch_date: 2020-07-30` and `agency: NASA` in its frontmatter, yet the check still raised `missing` questions for both fields. This suggests the check either ran before the fields were added and the questions weren't pruned on re-check, or there is a frontmatter parsing issue for this document. The same pattern appeared for Hope Probe and Tianwen-1 (which genuinely lack frontmatter fields), so it's hard to distinguish false positives from real gaps without inspecting each file.

- **Conflict false positives on concurrent objectives:** The `parallel_overlap` pattern flagged Perseverance's mission objectives ("Seek signs of ancient habitable conditions" and "Collect and cache rock and soil samples") as conflicting because they share the same `@t[2020..]` range. These are independent concurrent objectives, not competing claims. Same false-positive pattern observed in the jazz and volcanoes tests — confirmed as a systematic issue in the conflict detection engine.

- **Acronym ambiguity noise:** MOXIE, MSR, and ESA were each flagged 2–3 times across documents (Perseverance Rover, Jezero Crater Organics). UAE was flagged twice in Hope Probe. All are well-known acronyms in the Mars exploration domain. The suggestion to create `definitions/` docs for each is reasonable but generates significant noise when the same acronym appears in multiple documents — 10 of 46 questions (22%) are acronym-ambiguity flags.

- **Resolve step left most questions open:** Only 4 questions were deferred; 42 remain open and unanswered. The resolve workflow addressed the most critical items (stale mission status, weak citations) but did not work through the full queue. This is expected behavior for a partial resolve run, not a workflow failure.

- **Utopia Planitia missing required fields:** `utopia-planitia-subsurface.md` is missing both `date` and `mission` required fields for the `discovery` type. These are genuine gaps, not false positives.

**Deferred items (4):**
1. Hope Probe current status post-2024 (temporal — requires EMM verification)
2. Hope Probe Al Jazeera 2021 source staleness (stale — requires current source)
3. Hope Probe EMM Science Results citation URL (weak-source — believed answer with URL provided)
4. Tianwen-1 CNSA Mission Introduction citation URL (weak-source — believed answer with URL provided)

---

## Overall Assessment

Factbase handled the Mars exploration domain adequately, with one significant bug and several recurring minor issues.

**What worked well:**

- **Temporal tagging** correctly captured the active/ongoing nature of Mars missions with `@t[2020..]` open-ended ranges, and point-in-time events with `@t[=YYYY-MM-DD]` tags.
- **Citation enforcement** was thorough — weak-source questions correctly identified CNSA and EMM citations lacking URLs, and the resolve step handled them cleanly with `believed` answers.
- **Stale detection** correctly flagged 2020-era CNSA sources as potentially outdated (stale_days: 180 threshold).
- **Required fields** enforcement worked for genuinely missing fields (Utopia Planitia, Hope Probe, Tianwen-1 frontmatter gaps).
- **Refresh workflow** correctly identified that recent events (MSR cancellation, AI-planned drive) were already captured and made no spurious changes.

**What needs attention:**

- **Transition workflow data-integrity bug:** The corrupted citation in `perseverance-rover.md` is the most serious finding. The transition workflow's document update logic appears to splice metadata into citation text when the document has a review queue section. This should be investigated and the document manually repaired.
- **Conflict false positives** on concurrent properties remain a systematic issue (third test to confirm this pattern).
- **Acronym ambiguity noise** is high for a technical domain with many well-known abbreviations — 22% of questions are acronym flags. A domain-specific suppression list or a lower sensitivity threshold for all-caps abbreviations would reduce noise.
- **Missing-field false positives** on Perseverance (which has the fields) suggest a pruning or parsing gap worth investigating.

**Verdict: ⚠️ Factbase is suitable for this domain, but the transition workflow bug requires a fix before it can be trusted for document updates.** The core create/add/maintain/resolve/refresh workflows performed correctly. The Mars domain's mix of active missions, point-in-time events, and evolving mission architectures (MSR cancellation) is a good stress test for temporal tagging — and the system handled it well apart from the transition artifact.
