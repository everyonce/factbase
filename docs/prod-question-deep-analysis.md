# Prod KB Review Queue — Deep Analysis

**Date:** 2026-03-14  
**KB:** /Users/daniel/work/factbase-docs  
**Scope:** All 5,224 review questions (5,207 at task creation; small delta from new questions generated since)

---

## Executive Summary

**The most important finding: 98.5% of questions are DEFERRED, not unanswered.**

Of 5,224 total questions, only **78 are truly unanswered**. The remaining **5,146 were previously attempted** by agents but couldn't be resolved — they sit in a "believed" or "deferred" state with agent-written answers that were never applied. The queue is not a backlog of unexamined questions; it's a graveyard of questions that agents tried and failed to close.

**Second finding: ~2,900 questions (~56%) are bulk-dismissable false positives or known-internal-source flags.** The question generators have systematic blind spots that produce noise at scale.

### Actual Queue State

| Type | Total | Unanswered | Deferred |
|------|-------|-----------|---------|
| temporal | 1,547 | 6 | 1,541 |
| weak-source | 1,504 | 3 | 1,493 |
| ambiguous | 893 | 52 | 840 |
| stale | 733 | 0 | 733 |
| missing | 295 | 7 | 288 |
| corruption | 131 | 7 | 122 |
| conflict | 59 | 1 | 58 |
| precision | 56 | 2 | 54 |
| duplicate | 6 | 0 | 6 |
| **TOTAL** | **5,224** | **78** | **5,146** |

---

## Per-Type Deep Analysis

---

### 1. Temporal (1,547 total)

**Sample questions:**
- `"Co-founder of the AWS Jam program (formerly AWS Security Jam)" - when was this true?`
- `"Security expert and Amazon S3 expert" - when was this true?`
- `"Security & Compliance domain" - when was this true?`
- `"Amazon Kendra (enhanced search with NLP)" - when was this true?`
- `"AWS Step Functions (orchestrate NLP pipelines)" - when was this true?`
- `"Per unit (100 characters) of text processed" - when was this true?`
- `"Free tier: 50,000 units per month for 12 months" - when was this true?`
- `"Logan Buchanan (Partner at FortyAU) is embedded at Tivity Health @t[2024..]" - is this role still current?`
- `"Solutions Architect on Northeast Enterprise team @t[2021..]" - is this role still current?`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| AWS service feature bullets | ~600 | Bullet points in service reference docs (features, integrations, pricing) flagged as needing dates. Agents already answered "stable capability, not time-bound." |
| Open-ended @t[YYYY..] currency | ~500 | Facts with open-ended dates (role still current?) where source is old (2021-2022). Agents deferred: "Cannot verify." |
| Missing @t on historical/static facts | ~200 | Co-founder credits, expert designations, past achievements — arguably static facts |
| Section headers / role labels | ~150 | Section headings like "Security & Compliance domain" or "HCLS Providers segment" flagged as needing dates |
| AWS pricing model descriptions | ~97 | Pricing bullet points in service docs flagged as needing dates |

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (false positive) | ~700 | AWS service feature/pricing bullets in service reference docs — add `@t[=LAUNCH_DATE..]` or dismiss as stable |
| Bulk-dismiss (static facts) | ~200 | Co-founder credits, historical achievements — add `@t[=YEAR]` or mark static |
| Bulk-dismiss (section headers) | ~150 | Section headings are not facts; temporal checker shouldn't flag them |
| Human review | ~497 | Open-ended roles needing Phonetool/LinkedIn verification |

**Root cause of 1,541 deferred:** Agents correctly identified most as "stable capability" or "cannot verify" but the answers were never applied (deferred state). The apply step was skipped or failed.

**Bug/Feature gap:** The temporal checker fires on AWS service feature bullet points in reference docs. These are structural content, not time-bound facts. The checker needs a doc-type exemption: service reference docs (`concepts/`, `services/`) should not require `@t` on feature bullets.

---

### 2. Weak-Source (1,504 total)

**Sample questions:**
- `Citation [^1] "Phonetool lookup, 2026-02-10" is not specific enough to independently verify.`
- `Citation [^6] "Phonetool lookup, 2026-02-10---" is not specific enough to independently verify.`
- `Citation [^4] "Calendar/meeting notes, 2025-02-27" is not specific enough to independently verify.`
- `Citation [^2] "Factbase entity dewentze, manager field" is not specific enough to independently verify.`
- `Citation [^18] "Slack XSOLIS EBC channel, 2025-09-09" is not specific enough to independently verify.`
- `Citation [^1] "HealthStream [Internal] Meetings Quip doc, 2025-11-21" is not specific enough to independently verify.`
- `Citation [^2] "Email from Ryan O'Keeffe re: FSx NetApp, CC Glen Stummer, 2024-12-10" is not specific enough to independently verify.`
- `Citation [^1] "Business Journals / Nashville Business Journal, 2025-07-21" is not specific enough to independently verify.`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Phonetool citations | ~654 | "Phonetool lookup, YYYY-MM-DD" — internal AWS directory, no public URL possible |
| Slack citations | ~300 | "Slack #channel-name, YYYY-MM-DD" — internal, no public URL |
| Calendar/meeting notes | ~200 | "Calendar/meeting notes, YYYY-MM-DD" — internal, no public URL |
| Quip documents | ~150 | "Quip - [Doc Title], YYYY-MM-DD" — internal, no public URL |
| Email citations | ~100 | "Email from Person re: Subject, YYYY-MM-DD" — internal, no public URL |
| Factbase entity refs | ~50 | "Factbase entity X, field Y" — internal KB reference |
| News articles without URLs | ~50 | "Business Journals / Nashville Business Journal, 2025-07-21" — public but no URL added |

**Why weren't the 654 Phonetool citations fixed?**

They weren't "not fixed" — they were correctly identified as unfixable. Phonetool is an internal AWS employee directory with no public URL. The agent correctly answered: *"Internal AWS Phonetool. No public URL available. Citation includes date and is sufficient for internal verification."* and deferred them as "believed."

The problem is the question generator doesn't distinguish between:
- Public sources that should have URLs (news articles, press releases, LinkedIn)
- Internal sources that inherently cannot have public URLs (Phonetool, Slack, Quip, Calendar, email)

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (internal sources) | ~1,454 | Phonetool + Slack + Calendar + Quip + Email + Factbase refs — these are valid internal citations |
| Human review (add URLs) | ~50 | News articles, press releases — agent could web-search for URLs |

**Bug/Feature gap:** The weak-source checker needs an allowlist of known-internal source types: `Phonetool`, `Slack`, `Calendar/meeting notes`, `Quip`, `Email from`, `Factbase entity`. These should either be exempt from the check or auto-resolved as "internal source, no public URL required."

---

### 3. Ambiguous (893 total)

**Sample questions:**
- `"EDW" - what does "EDW" mean in this context?` (Enterprise Data Warehouse)
- `"SA" - what does "SA" mean in this context?` (Solutions Architect — appears ~50+ times)
- `"HCLS" - what does "HCLS" mean in this context?` (Healthcare & Life Sciences)
- `"EBA" - what does "EBA" mean in this context?` (Enterprise Business Agreement)
- `"DNB" - what does "DNB" mean in this context?` (Digital Native Business)
- `"PFR" - what does "PFR" mean in this context?` (Product Feature Request)
- `"TREND" - what does "TREND" mean in this context?` (Trend Health Partners brand name — appears 5+ times in one doc)
- `"KLAS" - what does "KLAS" mean in this context?` (healthcare IT research firm)
- `"ICD", "PHI", "FHIR", "SNOMED CT", "PII"` — standard healthcare/HIPAA terms
- `"PUT", "GB", "KPU", "GA"` — standard computing/AWS terms
- `"IBM" - what does "IBM" mean in this context?` (IBM — a Fortune 500 company)
- `Filed under 'Tivity Health' but 5 of 13 entity links point to 'XSOLIS'. Is this document filed correctly?`
- `"Based in the New York area" - is this home, work, or another type of location?`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Standard AWS acronyms | ~200 | SA, HCLS, EBA, DNB, PFR, WWPS, C&P, ADOT, AZ, EKS, ECS, SNS, etc. |
| Standard healthcare acronyms | ~200 | ICD, PHI, FHIR, SNOMED, HIPAA, EDW, KLAS, NLP, etc. |
| Standard computing terms | ~100 | PUT, GB, KPU, GA, API, ML, AI, etc. |
| Company brand names used as shorthand | ~100 | TREND (Trend Health Partners), CAVO, FLPPS, RHIO, XSOLIS |
| Document filing questions | ~50 | "Filed under X but links point to Y" |
| Location type ambiguity | ~50 | "home, work, or another type?" |
| Legitimate ambiguity | ~193 | Terms that genuinely need clarification in context |

**How many are just AWS acronyms?**

Approximately **200 of 893 (22%)** are standard AWS acronyms. Another ~300 are standard healthcare/computing terms. Combined, ~500 (56%) are industry-standard terms that any domain expert would recognize without a definitions doc.

The deferred queue confirms agents already answered most with "believed: dismiss: X = Y; standard term" — but answers were never applied.

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (standard AWS terms) | ~200 | SA, HCLS, EBA, DNB, WWPS, etc. — add to system glossary |
| Bulk-dismiss (standard industry terms) | ~300 | ICD, PHI, FHIR, GB, GA, IBM, etc. — add to system glossary |
| Bulk-dismiss (brand names in context) | ~100 | TREND, CAVO, RHIO — defined elsewhere in KB |
| Human review (filing questions) | ~50 | Cross-entity filing questions need human judgment |
| Human review (genuine ambiguity) | ~243 | Location type, context-dependent terms |

**Bug/Feature gap:** The ambiguity checker needs a glossary of known terms to skip. At minimum: all standard AWS service names and acronyms, all standard healthcare acronyms (ICD, PHI, FHIR, HIPAA, SNOMED, KLAS), and common computing terms (GB, API, ML, AI, GA, PUT). Filing questions ("Filed under X but links to Y") are a separate, legitimate check — keep those.

---

### 4. Stale (733 total — ALL deferred)

**Sample questions:**
- `"Solutions Architect, US-NE Enterprise team, AWS @t[2021..]" - Slack source from 2021 may be outdated`
- `"Analytics Specialist at Amazon Web Services @t[~2021]" - Email source from 2021-12-14 may be outdated`
- `"Based in Tennessee (US Southeast territory) @t[~2021]" - Email correspondence source from 2020 may be outdated`
- `"Support for 100+ languages @t[~2024]" - may be outdated` (AWS Transcribe capability)
- `"Cell: 404-668-9905 @t[~2019]" - may be outdated`
- `"Collecting customer feedback on agent collaboration capabilities for Amazon Connect @t[~2025-02]" - Calendar source from 2025-02-27 may be outdated`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Old role/title (source 2019-2022) | ~300 | Person docs with roles sourced from old Slack/email |
| Old contact info (phone, email) | ~100 | Phone numbers, email addresses from old sources |
| AWS service capabilities (stable) | ~100 | Service features flagged as stale despite being stable |
| Recent facts (@t[~2025]) | ~133 | Facts from 2025 flagged as stale — overly aggressive threshold |
| Historical activities (past tense) | ~100 | Past activities incorrectly flagged as needing currency check |
| Customer engagement facts | ~100 | Customer-specific facts from 2021-2023 |

**All 733 are deferred** — agents previously tried and couldn't verify. The pattern is consistent: "Cannot verify current role. Source is from 20XX."

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (stable AWS capabilities) | ~100 | Service features don't go stale; dismiss |
| Bulk-dismiss (recent facts @t[~2025]) | ~133 | Facts less than 18 months old shouldn't be flagged as stale |
| Bulk-dismiss (historical/past-tense) | ~100 | Past activities (qualified engagements, attended events) are historical facts |
| Human review (roles/titles) | ~300 | Need Phonetool or LinkedIn to verify current role |
| Human review (contact info) | ~100 | Need direct verification |

**Bug/Feature gap:** The stale checker's threshold is too aggressive. Recommendations:
1. Facts with `@t[~2025]` or newer should not be flagged as stale until 18+ months old
2. Past-tense activities (qualified, attended, built, authored) should be exempt — they're historical facts
3. AWS service capability facts should be exempt from stale checks

---

### 5. Missing (295 total)

**Sample questions:**
- `"#aws-usse-tn-sa (Tennessee SA team)" - what is the source?`
- `"#east-area-sa-community" - what is the source?`
- `"#healthstream-account-team" - what is the source?`
- `"Email: gfelipe@amazon.com @t[~2019]" - what is the source?`
- `"Active in #aws-cert-prep, #mac-users, #tools-at-amazon Slack channels @t[~2021]" - what is the source?`
- `"LinkedIn: linkedin.com/in/rondel-tooley-86b86373 @t[=2026]" - what is the source?`
- `"Bar Raiser, AWS Certified Solutions Architect-Associate" - what is the source?`
- `"Authored book 'Mastering AngularJS Directives'" - what is the source?`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Slack channel memberships | ~100 | "#channel-name" listed as a fact — the channel IS the source |
| LinkedIn URLs | ~30 | LinkedIn URL listed as a fact — the URL IS the source |
| Email addresses | ~30 | Email address listed — the email itself is the source |
| Certifications/credentials | ~50 | AWS certs, professional credentials — source is cert transcript |
| Role descriptions (inferred from LinkedIn) | ~50 | Facts inferred from LinkedIn profile without explicit citation |
| General facts without source | ~35 | Legitimate missing-source cases |

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (self-referential facts) | ~160 | Slack channel memberships, LinkedIn URLs, email addresses — the fact IS the source |
| Auto-fix (certifications) | ~50 | Agent can search certmetrics/LinkedIn for cert sources |
| Human review | ~85 | Facts that genuinely need source attribution |

**Bug/Feature gap:** The missing-source checker fires on self-referential facts: a Slack channel membership's source is the channel itself; a LinkedIn URL's source is LinkedIn. The checker should recognize that `#channel-name` and `linkedin.com/in/...` are self-evidencing.

---

### 6. Corruption (131 total)

**Sample questions:**
- `Temporal tag year 2021 matches footnote [^5] citation year — verify this is the intended date, not a copy-paste from the source`
- `Temporal tag year 2019 matches footnote [^2] citation year — verify this is the intended date, not a copy-paste from the source`
- `Temporal tag year 2023 matches footnote [^3] citation year — verify this is the intended date, not a copy-paste from the source`
- `Duplicate fact line (same as line 29)`
- `Duplicate fact line (same as line 22)`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| "Temporal year matches citation year" | ~100 | @t[=2021] with [^5] citing a 2021 source — flagged as possible copy-paste |
| Duplicate fact lines | ~31 | Actual duplicate lines in documents |

**Are these real corruption or template artifacts?**

The "temporal year matches citation year" pattern is **almost entirely a false positive**. If you cite a 2021 source for a 2021 event, the years will naturally match — that's correct behavior, not corruption. Example: `"Joined AWS @t[=2020-11] [^1]"` where [^1] is a 2020 Phonetool lookup. The years match because the fact and the source are from the same time.

The duplicate fact lines (~31) are legitimate corruption worth fixing.

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (year-match false positives) | ~100 | Year matching citation year is expected, not corruption |
| Human review (duplicate lines) | ~31 | Actual duplicates need manual dedup |

**Bug:** The "temporal year matches citation year" check is a false positive generator. It should be removed or significantly narrowed (e.g., only flag if the EXACT date matches, not just the year, AND the fact text is identical to the citation text).

---

### 7. Conflict (59 total)

**Sample question:**
- `"Joined AWS @t[=2020-11]" overlaps with "Senior SA, WWPS C&P team @t[2020-11..2022-02]" - were both true simultaneously?`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Join date overlaps with first role | ~30 | Joining a company and starting in a role happen simultaneously — expected overlap |
| Genuine conflicts | ~29 | Contradictory facts that need resolution |

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (join + first role overlap) | ~30 | Joining a company and starting a role are the same event |
| Human review | ~29 | Genuine conflicts need human judgment |

**Bug:** The conflict checker should not flag a join date overlapping with the first role at that company. This is a known-safe pattern.

---

### 8. Precision (56 total)

**Sample questions:**
- `"key" in "AWS Nitro for Secure Blockchain Key Operations" — "key" in what sense?`
- `"Trained 43 TechU members in Brazil (onsite) and Singapore (virtually)" — are there exceptions?`

**Sub-patterns:**

| Sub-pattern | Est. Count | Description |
|-------------|-----------|-------------|
| Technical jargon clear in context | ~20 | "key" in a blockchain/cryptography context is unambiguous |
| Vague quantifiers | ~36 | "trained 43 members" — legitimate precision question |

**Actionability:**

| Action | Count | Method |
|--------|-------|--------|
| Bulk-dismiss (technical jargon in context) | ~20 | "key" in blockchain context is unambiguous |
| Human review | ~36 | Quantifier precision questions are low-value but legitimate |

---

### 9. Duplicate (6 total — all deferred)

All 6 are deferred. Need human review to determine merge vs. keep.

---

## Consolidated Actionability Estimates

| Action | Count | Notes |
|--------|-------|-------|
| **Bulk-dismiss (false positives)** | **~2,900** | See breakdown below |
| **Human review** | **~1,834** | Roles, contacts, genuine conflicts, filing questions |
| **Auto-fixable by agent** | **~100** | News article URLs (web search), cert sources |
| **Legitimate but low-value** | **~390** | Precision questions, old contact info |

### Bulk-Dismissable Breakdown (~2,900)

| Category | Count | Reason |
|----------|-------|--------|
| Internal source citations (Phonetool, Slack, Quip, Calendar, Email) | ~1,454 | No public URL possible; valid internal citations |
| AWS service feature/pricing bullets | ~700 | Stable capabilities in reference docs |
| Standard AWS/healthcare/computing acronyms | ~500 | SA, HCLS, ICD, PHI, FHIR, GB, GA, etc. |
| Temporal year = citation year (corruption FP) | ~100 | Expected behavior, not corruption |
| Self-referential facts (Slack channels, LinkedIn URLs) | ~100 | The fact IS the source |
| Join date + first role overlap (conflict FP) | ~30 | Same event, not a conflict |
| Recent facts flagged as stale (@t[~2025]) | ~16 | Too recent to be stale |

---

## Bug Reports Needed

These are systematic false positive generators that should be filed as bugs/feature requests:

### Bug 1: weak-source — Internal source allowlist missing
**Impact:** ~1,454 questions  
**Description:** The weak-source checker flags Phonetool, Slack, Quip, Calendar, and email citations as "not specific enough" because they lack public URLs. These are inherently internal sources. The checker needs an allowlist: if the citation starts with "Phonetool", "Slack", "Calendar/meeting notes", "Quip", "Email from", or "Factbase entity", it should be exempt or auto-resolved.

### Bug 2: temporal — Service reference docs exempt needed
**Impact:** ~700 questions  
**Description:** AWS service feature bullet points in reference/concept docs are flagged as needing `@t` tags. These are structural content describing stable capabilities. The temporal checker should exempt docs in `concepts/` or `services/` folders, or recognize bullet-point feature lists as non-temporal.

### Bug 3: ambiguous — Glossary/allowlist missing
**Impact:** ~500 questions  
**Description:** Standard AWS acronyms (SA, HCLS, EBA, DNB, WWPS) and healthcare terms (ICD, PHI, FHIR, SNOMED, KLAS) are repeatedly flagged as ambiguous. A system glossary of known terms would eliminate these. IBM, GB, API, ML, AI should never be flagged.

### Bug 4: corruption — "Year matches citation year" is a false positive
**Impact:** ~100 questions  
**Description:** The corruption checker flags facts where the temporal tag year matches the citation year. This is expected behavior (you cite a 2021 source for a 2021 event). The check should be removed or narrowed to only flag when the exact date AND fact text are identical to the citation text.

### Bug 5: stale — Threshold too aggressive
**Impact:** ~233 questions  
**Description:** Facts with `@t[~2025]` are flagged as stale in 2026. The stale threshold should be at least 18 months. Additionally, past-tense activities (attended, qualified, authored, built) are historical facts and should be exempt from stale checks.

### Bug 6: conflict — Join date + first role overlap
**Impact:** ~30 questions  
**Description:** The conflict checker flags "joined company @t[=DATE]" overlapping with "first role @t[DATE..]" as a conflict. These are the same event. The checker should recognize this pattern as safe.

### Bug 7: missing — Self-referential facts
**Impact:** ~160 questions  
**Description:** Slack channel memberships (`#channel-name`) and LinkedIn URLs are flagged as missing sources. The fact itself is the source. The checker should recognize that `#channel-name` and `linkedin.com/in/...` are self-evidencing.

---

## Recommended Remediation Plan

### Phase 1: Fix the generators (highest leverage)
File the 7 bugs above. If fixed, ~2,900 questions would never be generated again, and existing ones could be bulk-dismissed.

### Phase 2: Bulk-dismiss existing false positives
With a bulk-answer tool, an agent could dismiss ~2,900 questions in a single pass:
- All weak-source questions where citation contains "Phonetool", "Slack", "Calendar", "Quip", "Email from", "Factbase entity"
- All temporal questions on docs in `concepts/` or `services/` folders
- All ambiguous questions for terms in a standard glossary
- All corruption questions matching "temporal year = citation year" pattern
- All conflict questions matching "join date + first role" pattern

### Phase 3: Human review queue (~1,834)
After bulk-dismissing false positives, the remaining ~1,834 questions are genuinely valuable:
- Stale roles/titles: verify via Phonetool or LinkedIn (~400)
- Genuine conflicts: resolve with human judgment (~29)
- Document filing questions: human judgment (~50)
- Missing sources for real facts (~85)
- Duplicate lines: manual dedup (~31)

### Phase 4: Agent-assisted enrichment (~100)
- News articles without URLs: agent web-searches for URLs (~50)
- Certification sources: agent searches certmetrics/LinkedIn (~50)

---

## Effort Estimate

| Phase | Questions | Estimated Time |
|-------|-----------|---------------|
| File 7 bugs | — | 30 min |
| Bulk-dismiss FPs (agent) | ~2,900 | 2-4 hours (needs bulk-answer tool) |
| Human review | ~1,834 | 10-20 hours |
| Agent enrichment | ~100 | 1-2 hours |

**Without a bulk-answer tool, Phase 2 is impractical** — answering 2,900 questions one at a time at the current rate would take weeks. The highest-leverage action is filing Bug 1 (internal source allowlist) and Bug 3 (acronym glossary), which together cover ~2,000 questions.
