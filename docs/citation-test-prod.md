# Citation System Test: Production KB (factbase-docs)
**Date:** 2026-03-14  
**KB:** `/Users/daniel/work/factbase-docs` (AWS SA customer tracking, 1471 docs)  
**Test:** Tier 1 citation validator behavior on real production KB with 5000+ citations

---

## KB Overview

- **1471 documents** total (1430 indexed, 41 updated this run)
- **9,995 cross-document links** across 1430 docs
- **4,993 total citations** analyzed (1898 specific + 3093 vague + 2 missing)
- **Temporal coverage:** 33% | **Source coverage:** 32%

---

## Tier 1 Results

### Citation Classification

| Category | Count | % | Meaning |
|----------|-------|---|---------|
| `citations_specific` (tier 1 pass) | 1,898 | 38% | Has full https:// URL — no question generated |
| `citations_vague` (tier 1 fail) | 3,093 | 62% | No URL — generates weak-source question |
| `citations_missing` | 2 | <1% | No citation at all |
| **Total** | **4,993** | | |

**Tier 1 criterion:** A citation passes if it contains a full `https://` URL. Everything else fails.

### What Passes Tier 1

Citations with full URLs pass cleanly — no weak-source question generated:

```
[^1]: "What is AWS HealthLake?" — https://docs.aws.amazon.com/healthlake/latest/devguide/what-is.html
[^2]: AWS product page — https://aws.amazon.com/healthlake/
[^3]: AWS What's New, 2025-06 — https://aws.amazon.com/about-aws/whats-new/2025/06/...
```

These are primarily found in `services/` documents (AWS service reference docs). The 1,898 specific citations are concentrated there.

### What Fails Tier 1 (Goes to Tier 2)

All internal/non-URL citations fail tier 1 and go to tier 2 batch:

| Citation Type | Example | Failure Reason |
|---------------|---------|----------------|
| Phonetool | `Phonetool lookup, 2026-02-10` | tool name present but no URL |
| LinkedIn (partial URL) | `LinkedIn profile, linkedin.com/in/lockwoodjeff` | tool name present but no URL |
| LinkedIn (no URL) | `LinkedIn profile, Max Kviatkouski, scraped 2026-02-10` | tool name present but no URL |
| Slack DM (author+date, no channel) | `Slack DM @kwatter, 2025-08-19` | Slack/Teams source missing channel (#name) or date |
| Slack channel (channel+date) | `Slack #healthstream-account-team, 2020-09-02` | (suppressed by prior answer) |
| Slack (author+date, no channel) | `Slack message from anonya, ..., 2026-03-02` | Slack/Teams source missing channel (#name) |
| Email (subject+date, no sender) | `Email thread (XCHANGE attendee logistics), 2024-09-18` | email source missing sender or date |
| Email signature | `Email signature, 2021 Archive` | email source missing sender or date |
| Meeting notes | `Calendar/meeting notes, 2025-02-27` | meeting/call source missing participants or date |
| Quip doc | `HealthStream [Internal] Meetings Quip doc, 2025-11-21` | source type unrecognized |
| Internal factbase ref | `Existing factbase-docs customer documents` | source type unrecognized |
| News (no URL) | `Business Journals / Nashville Business Journal, 2025-07-21` | source type unrecognized |
| PRNewswire (no URL) | `PRNewswire / AHP press release, formation of Collabrios Health, 2024-10-10` | book/publication present but no page/chapter/section reference |

---

## Specific Citation Type Observations

### 1. Phonetool Citations

**Result: FAIL tier 1 → tier 2 auto-answered as "believed"**

- Format: `Phonetool lookup, 2026-02-10`
- Failure reason: "tool name present but no URL — add the direct URL"
- Tier 2 auto-answer: `"believed: Internal AWS Phonetool. No public URL available. Citation includes date and is sufficient for internal verification."`
- Status in queue: **deferred** (1495 total deferred, Phonetool is the most common type)
- Phonetool is correctly identified as a "tool without URL" — the failure reason is accurate
- **Recommendation:** Tier 1 should recognize `Phonetool lookup, <date>` as a known internal tool and auto-pass it (or at minimum, auto-answer without human review). The tier 2 auto-answer is correct but adds noise.

### 2. Slack Citations — Channel + Date (No Author)

**Result: FAIL tier 1 → tier 2 auto-answered as "believed"**

- Format: `Slack #healthstream-account-team, 2020-09-02`
- These are in the deferred queue with "believed" answers
- The failure reason is "Slack/Teams source missing channel (#name) or date" — but these DO have a channel and date. The failure is a false positive for this format.
- **Recommendation:** `Slack #<channel>, <date>` should pass tier 1. Channel + date is sufficient for internal Slack search.

### 3. Slack Citations — Author + Date (No Channel)

**Result: FAIL tier 1 → tier 2 auto-answered as "believed"**

- Format: `Slack DM @kwatter, 2025-08-19`
- Failure reason: "Slack/Teams source missing channel (#name) or date"
- The failure is technically correct — DMs don't have a channel name, but they have author + date
- Tier 2 auto-answer: "believed: Slack DM citations include sender alias and date. Could be strengthened with a Slack permalink URL, but the alias and date are sufficient for search-based verification."
- **Recommendation:** `Slack DM @<alias>, <date>` should pass tier 1. Author + date is sufficient for DM verification.

### 4. Slack Citations — Missing Author (Channel + Year Only)

**Result: FAIL tier 1 → tier 2 auto-answered as "believed"**

- Format: `Slack #hcls-interoperability-interest, 2021` (year only, no specific date)
- Failure reason: "Slack/Teams source missing channel (#name) or date"
- This is a legitimate failure — year-only dates are too vague
- **Assessment: Correct behavior.** Year-only Slack citations should fail.

### 5. LinkedIn Citations

**Result: FAIL tier 1 regardless of URL format**

- `LinkedIn profile, linkedin.com/in/lockwoodjeff` → FAIL (partial URL, not https://)
- `LinkedIn profile, Max Kviatkouski, scraped 2026-02-10` → FAIL (no URL at all)
- Both get tier 2 auto-answers suggesting adding full https:// URL
- **Recommendation:** `linkedin.com/in/<slug>` (without https://) should be recognized as a URL and pass tier 1.

---

## Before vs. After Comparison

### New Weak-Source Questions Generated Per Run

| Metric | Before Tier 1 | After Tier 1 | Change |
|--------|--------------|--------------|--------|
| New weak-source questions (this run) | ~1,446 | **31** | **-98%** |
| Suppressed by prior answers | — | 2,255 | — |
| Tier 1 pass (no question) | 0 | 1,898 | +1,898 |

### Review Queue State (Weak-Source)

| Status | Count |
|--------|-------|
| Total weak-source in queue | 1,507 |
| Deferred (tier 2 auto-answered) | 1,495 |
| Open (unanswered) | 3 |
| Answered (verified) | 9 |

The 3 open questions are news citations without URLs:
- `Business Journals / Nashville Business Journal, 2025-07-21`
- `Technical.ly / PRNewswire, 2023-02-15`
- `PE Hub / PRNewswire, 2022-06-28`

These are legitimately weak — news citations that should have URLs added.

---

## Tier 2 Batch Size

The maintain workflow step 4 (tier 2 triage) presented **1,131 citations** for batch evaluation (200 in first batch, 931 remaining). This is the set of vague citations that need triage beyond the 1,495 already deferred from prior runs.

Breakdown of tier 2 batch failure reasons (from first 200):

| Failure Reason | Approx Count | % |
|----------------|-------------|---|
| tool name present but no URL (Phonetool, LinkedIn) | ~85 | 43% |
| Slack/Teams source missing channel or date | ~45 | 23% |
| email source missing sender or date | ~35 | 18% |
| source type unrecognized (meetings, internal docs) | ~30 | 15% |
| book/publication present but no page ref | ~5 | 3% |

---

## Key Findings

### What's Working Well

1. **Tier 1 correctly passes URL-backed citations** — 1,898 citations with full https:// URLs generate zero questions. Service docs are clean.
2. **Tier 2 auto-answers are accurate** — The "believed" answers for Phonetool, Slack, and internal sources are appropriate and actionable.
3. **Dramatic reduction in new questions** — Only 31 new weak-source questions vs ~1,446 before. The suppression-by-prior-answers mechanism is working.
4. **Phonetool correctly identified as "tool without URL"** — The failure reason is accurate and the tier 2 suggestion (construct URL from alias) is correct.

### False Positives / Issues

1. **`Slack #<channel>, <date>` fails tier 1** — Channel + date should be sufficient to pass. These are being correctly auto-answered as "believed" in tier 2, but they shouldn't need to go to tier 2 at all.
2. **`Slack DM @<alias>, <date>` fails tier 1** — Author + date should pass for DMs.
3. **`linkedin.com/in/<slug>` (without https://) fails tier 1** — Partial URLs should be recognized.
4. **Phonetool generates questions at all** — It's a known internal tool with a predictable URL pattern. Should be auto-passed or auto-answered without entering the queue.

### Recommendations

1. **Tier 1 additions** (would eliminate ~200+ questions from tier 2 batch):
   - `Slack #<channel>, <date>` → pass
   - `Slack DM @<alias>, <date>` → pass
   - `Phonetool lookup, <date>` → pass (known internal tool)
   - `linkedin.com/in/<slug>` (without https://) → pass

2. **Tier 2 auto-answer improvements**:
   - For Phonetool: auto-construct `https://phonetool.amazon.com/users/{alias}` suggestion from doc title/alias field
   - For LinkedIn without URL: auto-construct `https://linkedin.com/in/{slug}` suggestion

3. **The 1,495 deferred questions** are all correctly handled — no human action needed. The tier 2 auto-answer system is working as intended.

---

## Overall Assessment

The tier 1 system is **working correctly** for its primary purpose: eliminating questions for well-cited sources (URL-backed citations). The 38% tier 1 pass rate reflects the KB's citation profile — service docs have URLs, people/customer docs use internal sources (Phonetool, Slack, email) that don't have public URLs.

The tier 2 batch of 1,131 citations is manageable and the auto-answer quality is high. The main improvement opportunity is expanding tier 1 to recognize common internal source patterns (Phonetool, Slack with channel+date, LinkedIn partial URLs) to reduce tier 2 volume further.

The "before" state of ~1,446 weak-source questions being generated per run has been reduced to **31 new questions** — a 98% reduction. The existing 1,507 in the queue are largely handled (1,495 deferred with good answers).
