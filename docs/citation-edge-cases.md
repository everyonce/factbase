# Citation Edge Cases: Dead URLs, Paywalls, Fabricated Sources, Self-Referential

Test document exploring how the two-tier citation validator handles hard-to-classify sources.

**Tier 1** = Rust structural validator (`detect_citation_type` + `validate_citation` in `src/processor/citations.rs`).  
**Tier 2** = Batch LLM triage (maintain workflow step 4, `format_citation_triage_batch`).  
Tier 2 only sees citations that **fail** tier 1.

---

## Test Document

```markdown
<!-- factbase:edge01 -->
# Volcanic Activity — Citation Edge Cases

Mount Fuji last erupted in 1707. [^1]
The 1707 eruption deposited ash across the Kanto plain. [^2]
Paywalled Nature study confirms eruption magnitude. [^3]
JSTOR archive contains historical accounts. [^4]
USGS fact sheet provides geological context. [^5]
Smith (2024) documents activity patterns. [^6]
See [[volcanic-history]] for prior eruptions. [^7]
This KB's own analysis supports the timeline. [^8]
Dr. Chen confirmed the 1707 date. [^9]
Internal Confluence page has field notes. [^10]

---
[^1]: https://example.com/deleted-page-404
[^2]: https://web.archive.org/web/2020/https://example.com/old-page
[^3]: https://www.nature.com/articles/s41586-024-12345-6
[^4]: https://www.jstor.org/stable/12345
[^5]: https://www.usgs.gov/volcanoes/mount-fuji-fact-sheet
[^6]: Smith, J. (2024) "Volcanic Activity Patterns", Journal of Made-Up Science, vol.1, p.1-10
[^7]: See [[other-doc-in-kb]]
[^8]: factbase-docs customer analysis, 2026
[^9]: Personal communication with Dr. Chen, 2025-11-20
[^10]: Company internal Confluence page, last updated 2025-09
```

---

## Results

### 1. Dead URL — `https://example.com/deleted-page-404`

| | Result |
|---|---|
| **Tier 1 type** | `Url` |
| **Tier 1 verdict** | **PASS** — not sent to tier 2 |
| **Tier 2 verdict** | N/A |

**Makes sense?** Yes. Tier 1 validates structure, not liveness. A URL is navigable by definition — whether it returns 404 is a runtime concern tier 1 cannot check. Tier 2 would flag this as WEAK if it saw it (dead link, consider archiving), but since tier 1 passes it, tier 2 never runs.

**Gap**: No tier catches dead URLs. If liveness matters, the KB author must verify manually or use a link-checker tool.

---

### 2. Wayback Machine URL — `https://web.archive.org/web/2020/https://example.com/old-page`

| | Result |
|---|---|
| **Tier 1 type** | `Url` |
| **Tier 1 verdict** | **PASS** — not sent to tier 2 |
| **Tier 2 verdict** | N/A |

**Makes sense?** Yes. A Wayback URL is a fully valid, navigable archival source. Tier 1 correctly passes it. Tier 2 would label it VALID — archival URLs are a legitimate citation form.

---

### 3. Paywalled Nature article — `https://www.nature.com/articles/s41586-024-12345-6`

| | Result |
|---|---|
| **Tier 1 type** | `Url` |
| **Tier 1 verdict** | **PASS** — not sent to tier 2 |
| **Tier 2 verdict** | N/A |

**Makes sense?** Yes. A paywall does not invalidate a citation — the URL is specific and navigable (even if access requires a subscription). Tier 2 would label it VALID with a note that access requires institutional login.

---

### 4. Paywalled JSTOR — `https://www.jstor.org/stable/12345`

| | Result |
|---|---|
| **Tier 1 type** | `Url` |
| **Tier 1 verdict** | **PASS** — not sent to tier 2 |
| **Tier 2 verdict** | N/A |

**Makes sense?** Yes. Same reasoning as #3. JSTOR stable URLs are a standard academic citation form.

---

### 5. Possibly fabricated USGS URL — `https://www.usgs.gov/volcanoes/mount-fuji-fact-sheet`

| | Result |
|---|---|
| **Tier 1 type** | `Url` |
| **Tier 1 verdict** | **PASS** — not sent to tier 2 |
| **Tier 2 verdict** | N/A |

**Makes sense?** Partially. Tier 1 correctly passes the URL structurally. However, USGS is a US agency and does not publish fact sheets on Mount Fuji (a Japanese volcano). Tier 2 would flag this as WEAK with fabrication risk — but since tier 1 passes it, tier 2 never runs.

**Gap**: Fabricated URLs that look plausible pass tier 1 silently. Tier 2 would catch this if it saw it, but it doesn't. This is an inherent limitation: tier 1 cannot make HTTP requests.

---

### 6. Fabricated journal — `Smith, J. (2024) "Volcanic Activity Patterns", Journal of Made-Up Science, vol.1, p.1-10`

| | Result |
|---|---|
| **Tier 1 type** | `Book` (matched `journal` keyword) |
| **Tier 1 verdict** | **PASS** — `p.1-10` satisfies page reference requirement |
| **Tier 2 verdict** | N/A |

**Makes sense?** Structurally yes, semantically no. Tier 1 sees "Journal" → Book type, then "p.1" → page reference → passes. It cannot evaluate whether "Journal of Made-Up Science" is a real publication. Tier 2 would flag this as WEAK with high fabrication risk and suggest verifying the journal exists (e.g., via CrossRef or ISSN lookup).

**Gap**: Fabricated academic citations with page references pass tier 1. Tier 2 is the only defense here, but it only runs on tier-1 failures. This is a known limitation: structural validation cannot detect invented journal names.

**Note**: The citation uses `Smith, J. (2024)` format (year in parentheses), which does NOT match `ACADEMIC_REGEX` (requires `Author Year` with whitespace, not `Author, I. (Year)`). So it falls through to `BOOK_REGEX` via the `journal` keyword.

---

### 7. Internal KB cross-reference — `See [[other-doc-in-kb]]`

| | Result |
|---|---|
| **Tier 1 type** | `Unknown` |
| **Tier 1 verdict** | **FAIL** → sent to tier 2 |
| **Tier 2 verdict** | WEAK — internal KB link is not an external source; acceptable for cross-referencing but cannot be independently verified |

**Makes sense?** Yes. `[[wikilink]]` syntax is not a recognized citation type. Tier 1 correctly fails it. Tier 2 would label it WEAK: it's a valid internal cross-reference but not a verifiable external source. The agent would likely suggest either keeping it as a cross-reference (not a footnote) or adding an external source alongside it.

---

### 8. Self-referential KB citation — `factbase-docs customer analysis, 2026`

| | Result |
|---|---|
| **Tier 1 type** | `Unknown` |
| **Tier 1 verdict** | **FAIL** → sent to tier 2 |
| **Tier 2 verdict** | INVALID — circular citation; the KB cannot cite itself as a source for its own facts |

**Makes sense?** Yes. Tier 1 correctly fails it (no recognizable structure). Tier 2 would label it INVALID: citing the KB itself as a source is circular reasoning. The agent would suggest replacing with the original external source that the KB analysis was based on.

---

### 9. Personal communication — `Personal communication with Dr. Chen, 2025-11-20`

| | Result |
|---|---|
| **Tier 1 type** | `Conversation` (after fix: `communication` added to `CONVERSATION_REGEX`) |
| **Tier 1 verdict** | **PASS** — has date (`2025-11-20`) + participant indicator (`with`) |
| **Tier 2 verdict** | N/A |

**Makes sense?** Yes, after the fix. "Personal communication" is a standard academic citation format (APA, Chicago, etc.). Before the fix, `communication` was not in `CONVERSATION_REGEX`, so this fell through to `Unknown` and failed tier 1 — a false negative.

**Fix applied**: Added `communication` to `CONVERSATION_REGEX`. The validate logic for `Conversation` requires `has_date && has_participants`; both are present here.

**Before fix**: Tier 1 FAIL → tier 2 would label VALID (named person + date is sufficient).  
**After fix**: Tier 1 PASS → correctly handled without LLM overhead.

---

### 10. Confluence without URL — `Company internal Confluence page, last updated 2025-09`

| | Result |
|---|---|
| **Tier 1 type** | `NavigableTool` (matched `confluence` in `KNOWN_TOOL_REGEX`) |
| **Tier 1 verdict** | **FAIL** → sent to tier 2 (no URL present) |
| **Tier 2 verdict** | WEAK — Confluence is navigable but URL is missing; add the direct page URL |

**Makes sense?** Yes. Confluence is a navigable tool — tier 1 correctly requires a URL. Without it, the citation cannot be independently verified. Tier 2 would label it WEAK and suggest adding the Confluence page URL (e.g., `https://company.atlassian.net/wiki/spaces/PROJ/pages/12345`).

---

## Summary Table

| # | Citation | Tier 1 Type | Tier 1 | Tier 2 | Sensible? |
|---|---|---|---|---|---|
| 1 | Dead URL | `Url` | PASS | N/A | ✓ (by design) |
| 2 | Wayback URL | `Url` | PASS | N/A | ✓ |
| 3 | Paywalled Nature | `Url` | PASS | N/A | ✓ |
| 4 | Paywalled JSTOR | `Url` | PASS | N/A | ✓ |
| 5 | Fabricated USGS URL | `Url` | PASS | N/A | ⚠ gap |
| 6 | Fabricated journal | `Book` | PASS | N/A | ⚠ gap |
| 7 | Internal KB ref | `Unknown` | FAIL | WEAK | ✓ |
| 8 | Self-referential | `Unknown` | FAIL | INVALID | ✓ |
| 9 | Personal communication | `Conversation` | PASS | N/A | ✓ (after fix) |
| 10 | Confluence no URL | `NavigableTool` | FAIL | WEAK | ✓ |

---

## Gaps Identified

### Gap A: Tier 1 cannot detect dead/fabricated URLs (cases 1, 5)
Tier 1 is a static validator — it cannot make HTTP requests. Any URL passes tier 1 regardless of whether it returns 404 or doesn't exist. This is by design (no network dependency), but means fabricated-looking URLs slip through. Tier 2 would catch case 5 if it saw it, but it doesn't because tier 1 passes it first.

**Mitigation**: Tier 2 triage prompt explicitly asks about "fabrication risk" — if tier 2 ever sees a URL citation, it can flag it. But tier 2 only sees tier-1 failures.

### Gap B: Fabricated academic citations with page refs pass tier 1 (case 6)
A citation like `Smith, J. (2024) "...", Journal of Made-Up Science, p.1` passes tier 1 because it has a journal keyword + page reference. Tier 1 cannot evaluate whether the journal exists.

**Mitigation**: Tier 2 is the right place to catch this. Consider routing all `Book`-type citations with suspicious journal names to tier 2 — but this requires heuristics that may be domain-specific.

### Gap C: `communication` missing from `CONVERSATION_REGEX` (case 9) — **fixed**
"Personal communication" is a standard academic citation format but was not recognized by tier 1 because `communication` was not in `CONVERSATION_REGEX`. This caused a false negative: a valid citation was sent to tier 2 unnecessarily.

**Fix**: Added `communication` to `CONVERSATION_REGEX`. Tests updated to reflect the corrected behavior.

---

## Code Change

`src/processor/citations.rs` — `CONVERSATION_REGEX`:

```rust
// Before
r"(?i)\b(?:meeting|call|conversation|discussion|interview|one-on-one|standup|sync)\b"

// After
r"(?i)\b(?:meeting|call|conversation|communication|discussion|interview|one-on-one|standup|sync)\b"
```
