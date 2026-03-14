# Citation System Validation Report

**Generated:** 2026-03-14  
**Scope:** 3 test KBs + 1 production KB  
**Purpose:** Evaluate citation tier-1 pass rates and weak-source question prevalence

---

## Summary Table

| Metric | Volcanoes | Jazz | Mars | Prod |
|--------|-----------|------|------|------|
| Docs | 15 | 14 | 13 | 1,484 |
| Citations (footnote defs) | 80 | 45 | 26 | 5,233 |
| URL citations (https?://) | 58 | 38 | 26 | 530 |
| Other navigable refs | 7 | 7 | 0 | 4,430 |
| Tier 1 failures | 15* | 0 | 0 | 273 |
| Tier 1 pass rate | 81.2%* | 100% | 100% | 94.8% |
| Total review questions | 150 | 171 | 99 | 5,243 |
| `@q[weak-source]` questions | 4 | 6 | 0 | 1,504 |

*Volcanoes: 14 of 15 failures are intentional test cases in `tests/citation-tier1-test.md`; 1 is a test artifact (`[^transition]: Test transition`) in real content. Excluding the test file: **1 real failure, 98.5% pass rate**.

---

## Per-KB Detail

### Volcanoes (`/tmp/factbase-test-volcanoes`)

**Recent history:** 2 commits — initial creation + transition workflow test

**Citation breakdown:**
- 58 URL citations (72.5%) — all in real content docs
- 7 other navigable: 2 book citations (ISBN/author+title+publisher+year), 1 journal article (vol/pp), 4 DOI/catalog refs
- 15 tier-1 failures:
  - 14 in `tests/citation-tier1-test.md` — **intentional test fixtures** covering the full failure taxonomy (vague Slack DMs, "Wikipedia", "Author knowledge", "Meeting notes", etc.)
  - 1 in `events/2022-tonga-eruption.md`: `[^transition]: Test transition` — leftover test artifact from transition workflow

**Weak-source questions (4):** False positives — the checker is flagging legitimate book citations (Winchester/HarperCollins, Robock/Science) and a USGS web page as "not specific enough." These citations are navigable via ISBN or journal search. The checker's threshold is too strict for print sources.

**Assessment:** ✅ Healthy. Real content has near-perfect citation quality.

---

### Jazz (`/tmp/factbase-test-jazz`)

**Recent history:** 1 commit — initial creation

**Citation breakdown:**
- 38 URL citations (84.4%)
- 7 other navigable: all book/liner-note citations with author+title+publisher+year (e.g., "Ashley Kahn, A Love Supreme: The Story of John Coltrane's Signature Album, Viking Press, 2002. ISBN: 978-0670030835")
- 0 tier-1 failures

**Weak-source questions (6):** All false positives — the checker flags liner notes and book citations (e.g., "Original liner notes, Kind of Blue, Columbia Records, CL 1355 / CS 8163, 1959") as weak. These are standard musicological citations, fully navigable via catalog number or ISBN. The checker does not recognize print/physical media citation formats.

**Assessment:** ✅ Excellent. 100% tier-1 pass rate. Weak-source questions are checker false positives.

---

### Mars (`/tmp/factbase-test-mars`)

**Recent history:** 1 commit — initial creation

**Citation breakdown:**
- 26 URL citations (100%) — mix of https:// (25) and http:// (1: cnsa.gov.cn)
- 0 other navigable (not needed — all citations have URLs)
- 0 tier-1 failures

**Weak-source questions (0):** None. Every citation has a URL.

**Assessment:** ✅ Perfect. All citations are URL-backed. The one http:// citation (CNSA) is expected — the Chinese space agency site does not offer HTTPS on that path.

---

### Production (`/Users/daniel/work/factbase-docs`)

**Recent history:**
```
c8e0534a Daily maintain: scan + link detection (41 updated, 9995 links)
15ca6b02 Full maintain: scan, link detection, resolve 1225 review questions
8075fbca Bulk dismiss ~1,280 false-positive review questions
a14f44de Daily refresh 2026-03-13: XSOLIS EBA path-to-production, ...
005f7fea Correction: 48U is FortyAU
```

**Citation breakdown:**
- 530 URL citations (10.1%) — low URL rate reflects the nature of the domain (internal business knowledge, Slack, email)
- 4,430 other navigable (84.7%):
  - Slack messages with channel ID or date (`Slack #channel, 2025-02`, `Slack C072MCFPFTJ`)
  - Internal doc cross-references via `[[wikilink]]` syntax
  - Email threads with date and subject
  - LinkedIn profiles cross-referenced to factbase entities
  - Glossary/abbreviation footnotes
- 273 tier-1 failures (5.2%):
  - ~53 internal doc refs without `[[]]` wikilink format (e.g., `john-boyd.md`, `customers/xsolis/projects/...`)
  - ~122 email refs without sufficient specificity (e.g., "Outlook email correspondence, 2025" — year only, no subject)
  - ~26 Slack refs without date or channel ID
  - ~50 vague/generic (e.g., "Inferred from role title", "Direct observation / self-authored", "SpecReq system data")
  - ~20 other (calendar invites, account records, "factbase entity" refs without IDs)

**Weak-source questions (1,504):** The checker is flagging Slack DMs, Quip docs, and internal references as weak. Many of these are legitimate internal sources that cannot have public URLs. This is the dominant source of review queue noise in prod — 28.7% of all review questions are `@q[weak-source]`.

**Assessment:** ⚠️ Acceptable but has actionable gaps. The 273 failures are real and fixable.

---

## Review Question Distribution

| Type | Volcanoes | Jazz | Mars | Prod |
|------|-----------|------|------|------|
| `temporal` | 33 | 79 | 41 | 1,548 |
| `stale` | 32 | 50 | 24 | 740 |
| `ambiguous` | 29 | 9 | 6 | 900 |
| `missing` | 26 | 17 | 20 | 301 |
| `precision` | 24 | 8 | 6 | 56 |
| `weak-source` | 4 | 6 | 0 | 1,504 |
| `conflict` | 2 | 2 | 2 | 59 |
| `corruption` | 0 | 0 | 0 | 129 |
| `duplicate` | 0 | 0 | 0 | 6 |
| **Total** | **150** | **171** | **99** | **5,243** |

---

## Gaps and Findings

### Finding 1: Weak-source checker has false positives on print/physical media

The `@q[weak-source]` generator flags book citations and liner notes as weak even when they include author, title, publisher, year, and catalog number. These are fully navigable via ISBN or library catalog. The checker should recognize:
- `Author, Title, Publisher, Year` as tier-1 navigable
- Liner notes with record label + catalog number as tier-1 navigable
- Citations ending in a 4-digit year with publisher as tier-1 navigable

**Impact:** 10 false-positive weak-source questions across test KBs (4 volcanoes + 6 jazz). In prod, the 1,504 weak-source questions likely contain a significant false-positive fraction for internal sources.

### Finding 2: Prod has 273 real tier-1 failures — mostly fixable patterns

The failures cluster into addressable categories:
1. **Internal doc refs without `[[]]`** (~53): Should use `[[path/to/doc|Title]]` wikilink format so they're navigable in Obsidian and detectable by the link system
2. **Email refs without subject line** (~122): "Outlook email correspondence, 2025" should include subject or thread identifier
3. **Slack refs without channel ID or date** (~26): Should follow the pattern `Slack #channel-name (CXXXXXXXX), YYYY-MM`
4. **Vague/inferred** (~50): "Inferred from role title" — acceptable for derived facts but should note the inference chain
5. **Other** (~22): Calendar invites, account records — these are internal systems; acceptable if the system name is specific

### Finding 3: Mars KB is the citation quality gold standard

All 26 citations have URLs. The KB was built with URL-first discipline. This should be the model for future test KB creation.

### Finding 4: Test KB weak-source questions are expected noise

The 4+6 weak-source questions in volcanoes/jazz are not a problem — they reflect the checker's current behavior on print sources. Mars has 0 because it uses URLs exclusively.

---

## Recommendations

1. **Fix weak-source checker for print citations** — Add recognition for `Author, Title, Publisher, YYYY` and liner note patterns as tier-1 navigable. This would eliminate ~10 false positives in test KBs and reduce prod noise.

2. **Migrate prod internal doc refs to `[[]]` format** — The ~53 bare `.md` path references should be converted to wikilinks. This makes them navigable and detectable by the link system.

3. **Add Slack channel ID to bare Slack refs in prod** — The ~26 Slack refs without IDs should be enriched. Pattern: `Slack #channel-name (CXXXXXXXX), YYYY-MM`.

4. **Accept "inferred" citations as tier-2** — Facts derived from other facts ("Inferred from role title") are a legitimate citation type. Consider a formal `[^inferred]` pattern that the checker treats as tier-2 (not a failure, but flagged for review).

5. **Use Mars as the template for new test KBs** — URL-first citation discipline produces zero weak-source questions and 100% tier-1 pass rate.

---

## Overall Assessment

| KB | Status | Notes |
|----|--------|-------|
| Volcanoes | ✅ Healthy | 1 real failure (test artifact); test file failures are intentional |
| Jazz | ✅ Excellent | 100% pass rate; weak-source Qs are false positives |
| Mars | ✅ Perfect | 100% pass rate; 0 weak-source Qs; gold standard |
| Prod | ⚠️ Acceptable | 94.8% pass rate; 273 real failures; 1,504 weak-source Qs need triage |

The citation system is working correctly. The main actionable items are: (1) fix the weak-source checker's false positives on print sources, and (2) address the ~273 prod tier-1 failures, most of which are internal references that can be upgraded to wikilink format.
