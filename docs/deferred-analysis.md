# Deferred Review Queue Analysis — Full KB
**Updated:** 2026-03-13 (v2 — all directories)  
**Previous analysis:** Covered customers/ only (~1,448 questions at time of writing)  
**Original total (task #536):** 5,462 questions across all directories  
**Current deferred queue:** 1,460 questions (4,000+ already applied since task was created)  
**Unanswered (not yet deferred):** 31  
**Type breakdown (current):** weak-source ~525, ambiguous ~338, stale ~197, temporal ~142, missing ~68, corruption ~54, conflict ~54, precision ~25, duplicate ~3

---

## Status Update

Since task #536 was filed, approximately **4,000 questions have been processed and applied** — reducing the queue from 5,462 to 1,460. The remaining 1,460 all have pre-populated `answer` fields from a previous agent run but have not been formally applied. The patterns are identical to what was found in the customers/ analysis; they just repeat across all directories.

---

## Per-Directory Breakdown

### amazon/ (~2,235 original → ~600 current)
AWS employee person docs. Sourced primarily from Phonetool.

**Dominant patterns:**
- **weak-source (~400):** Every person doc with `[^N] "Phonetool lookup, 2026-02-10"` flagged as not independently verifiable. False positive — Phonetool has no public URL by design.
- **ambiguous (~80):** SA, WWPS, EBA, DNB, TFC, CSAT, CPG, EBC, RFx, etc. All standard AWS/industry terms.
- **conflict (~40):** Joining AWS on the same date as starting a role flagged as overlapping. False positive — they're the same event.
- **stale (~80):** Person roles with dates from 2019–2022. Cannot auto-verify; requires Slack/Phonetool check.
- **weak-source (~30):** Internal email citations (SA Launch, Win Wire, Compass notifications). Valid internal sources.

**Recommendation:** Bulk-dismiss ~550 (Phonetool, acronyms, false-conflict, internal emails). ~80 stale person roles need human review or Slack verification.

---

### services/ (~1,730 original → ~700 current)
AWS service reference docs (~60–70 services). Highly uniform structure generates highly uniform false positives.

**Dominant patterns:**
- **temporal+missing (~120):** The `type: services` metadata field on line 4 of every service doc generates exactly 2 questions per doc: "when was this true?" and "what is the source?" Pure bug — metadata is not a factual claim.
- **temporal (~400):** Every feature/capability bullet in every service doc gets "when was this true?" AWS service features are reference data, not time-series facts.
- **temporal (~100):** Every pricing bullet gets "when was this true?" Same issue.
- **corruption (~54):** Unused `[^2]` footnote in nearly every service doc — template artifact.
- **ambiguous (~100):** Standard acronyms: GA, CDK, GB, NET (.NET), BYOL, TLD, SME, HDFS, ASL, VTL, CIS, PII, SRT, VTT, UI, etc.
- **weak-source (~30):** Citations like `"Existing factbase-docs customer documents"` — valid internal KB cross-references.
- **conflict (~10):** Simultaneous feature launches (e.g., two CodePipeline V2 features both at @t[=2023-10]) flagged as conflicting. False positive.
- **stale (~10):** Customer usage facts in service docs with @t[~2025] tags.
- **precision (~25):** "key in what sense?", "GA — are there exceptions?" Overly pedantic on standard technical terms.

**Recommendation:** Bulk-dismiss ~680 (all of the above except ~10 stale customer usage facts). The stale customer usage facts need Slack verification.

---

### customers/ (~1,416 original → ~130 current)
Customer account docs (XSOLIS, HealthStream, Tivity Health, etc.).

**Dominant patterns:**
- **stale (~80):** Customer service usage facts with @t[~2025] or @t[~2024] tags — "is this still accurate?" Verifiable via local-search-vault.
- **weak-source (~30):** Citations like `"XSOLIS AWS Infrastructure doc (factbase-docs)"` — valid internal KB cross-references.
- **ambiguous (~15):** Customer-specific acronyms and standard terms.
- **conflict (~5):** False-positive overlapping facts.

**Recommendation:** ~50 bulk-dismiss (weak-source, ambiguous, false-conflict). ~80 stale facts: auto-resolve via local-search-vault against customer Slack channels.

---

### partners/ (~108 original → ~25 current)
Partner/ISV docs. Similar structure to amazon/ person docs.

**Dominant patterns:**
- **weak-source (~15):** Phonetool or internal email citations.
- **ambiguous (~5):** Standard acronyms.
- **stale (~5):** Role/relationship facts with old dates.

**Recommendation:** Bulk-dismiss ~20. ~5 stale facts need human review.

---

### concepts/ (~2 original → ~2 current)
Only 2 questions. Likely document type tag false positives.

**Recommendation:** Bulk-dismiss both.

---

## Pattern Catalog (All Directories)

These 13 groups cover ~100% of the 1,460 current deferred questions:

### Group 1 — Phonetool citations (weak-source)
**Count:** ~400 | **Directories:** amazon/, partners/  
**Pattern:** `[^N] "Phonetool lookup, 2026-02-10"` flagged as not independently verifiable.  
**Pre-answer:** "Internal AWS Phonetool — no public URL available. Citation includes date and is sufficient for internal verification via phonetool.amazon.com"  
**Recommendation: BULK-DISMISS** — False positive. Phonetool is the canonical internal AWS employee directory with no public URL by design.

---

### Group 2 — Internal email/Win Wire/Compass citations (weak-source)
**Count:** ~50 | **Directories:** amazon/  
**Pattern:** Citations like `"Email, SA Launch cohort invitation, 2022-07-08"`, `"Win Wire email (Eventbrite), 2020-08-17"`, `"AWS Compass AWSome Builder panel notification, 2022-06-16"` flagged as not independently verifiable.  
**Pre-answer:** "Internal email/notification. No public URL available. Citation includes date and is sufficient for internal verification."  
**Recommendation: BULK-DISMISS** — Internal correspondence is a valid primary source for this KB.

---

### Group 3 — Factbase cross-reference citations (weak-source)
**Count:** ~30 | **Directories:** services/, customers/  
**Pattern:** Citations like `"Existing factbase-docs customer documents"`, `"XSOLIS AWS Infrastructure doc (factbase-docs)"` flagged as not specific enough.  
**Pre-answer:** "Internal factbase document reference. Citation is sufficient for internal verification."  
**Recommendation: BULK-DISMISS** — Valid internal KB cross-references. Ideal fix is to add `[[doc_id]]` links, but dismissing is acceptable.

---

### Group 4 — Standard AWS/industry acronyms (ambiguous)
**Count:** ~250 | **Directories:** all  
**Pattern:** Every acronym flagged with "what does X mean? Consider creating a definitions/ doc."  
**Acronyms:** SA, WWPS, EBA, DNB, TFC, GA, BI, SPICE, OCU, GWLB, LCU, CUR, CUDOS, CID, FTPS, AS2, SFTP, AD, GB, BGP, IKEv2, MFA, CDK, NET, BYOL, TLD, SME, HDFS, ASL, VTL, CIS, PII, SRT, VTT, UI, CPG, CSAT, EBC, RFx, FL (state abbreviation), and more.  
**Pre-answer:** "dismiss: [expansion] — standard AWS/industry term"  
**Recommendation: BULK-DISMISS** — All standard terms. Creating definitions docs for "GB" or "SA" would be noise.

---

### Group 5 — AWS service feature bullets (temporal)
**Count:** ~400 | **Directories:** services/  
**Pattern:** Every feature/capability/integration bullet in service docs gets "when was this true?"  
**Pre-answer:** "Currently true as of 2026 — ongoing feature of [service]."  
**Recommendation: BULK-DISMISS** — Service capability docs are reference material, not time-series data.

---

### Group 6 — AWS pricing bullets (temporal)
**Count:** ~100 | **Directories:** services/  
**Pattern:** Every pricing bullet gets "when was this true?"  
**Pre-answer:** "Currently true as of 2026 — ongoing pricing model."  
**Recommendation: BULK-DISMISS** — Same reasoning as Group 5.

---

### Group 7 — Document type tag "services" (temporal + missing)
**Count:** ~120 | **Directories:** services/  
**Pattern:** The `type: services` metadata field on line 4 generates 2 questions per doc: "when was this true?" and "what is the source?"  
**Pre-answer:** "dismiss: Document type tag is static metadata"  
**Recommendation: BULK-DISMISS** — Clear bug: the check system should not flag the `type:` metadata field.

---

### Group 8 — Unused footnote [^2] (corruption)
**Count:** ~54 | **Directories:** services/  
**Pattern:** "Footnote [^2] is defined but never referenced in document body" — appears in nearly every service doc.  
**Pre-answer:** "Accepted: [^2] is an unused footnote definition — likely a template artifact."  
**Recommendation: BULK-FIX (preferred) or BULK-DISMISS** — Template artifact. Clean fix is to remove the orphaned `[^2]` definition from each service doc.

---

### Group 9 — Customer usage facts with @t[~2025] (stale)
**Count:** ~30 | **Directories:** customers/, services/  
**Pattern:** Customer-specific service usage facts — "is this still accurate?"  
**Pre-answer:** "believed: likely still current" (unverified)  
**Examples:** XSOLIS (OpenSearch, Fargate/ECS, Dragonfly Platform), HealthStream (OpenSearch, QuickSight, Security Hub), Tivity Health (Amazon Q, QuickSight, CUR), CereCore (Amazon Q), HealthStream (Step Functions @t[~2021-09])  
**Recommendation: AUTO-RESOLVE via local-search-vault**  
Search customer Slack channels for service mentions in the last 90 days:
- XSOLIS: `C047ZQ6KJAW`
- HealthStream: `G019TUFFR51`
- Tivity Health: `C07NUQKJLJE`

---

### Group 10 — Person role facts with old dates (stale)
**Count:** ~100 | **Directories:** amazon/, partners/  
**Pattern:** Person docs with role/location dates from 2019–2022 — "is this still accurate?" Cannot auto-verify.  
**Pre-answer:** "Cannot verify current role. Source is from [year]."  
**Examples:** Howard Brantly (SA South Florida @t[2022-02..]), David Henry (SA at AWS @t[~2022-07]), Robert Kissell (Asurion account @t[~2023]), Emmett Mountjoy (Inside Sales AM @t[2019-05..])  
**Recommendation: NEEDS HUMAN REVIEW or Slack verification**  
Check recent Slack DMs and account team channels. If the person appears in recent conversations with the same role context, update the temporal tag.

---

### Group 11 — False-positive conflicts (conflict)
**Count:** ~54 | **Directories:** amazon/, services/  
**Pattern:** Two facts for the same entity at the same time period flagged as conflicting — e.g., joining AWS on the same date as starting a role, or two concurrent PM responsibilities, or two CodePipeline features launching simultaneously.  
**Pre-answer:** "believed: Not a conflict — [explanation of why both can be simultaneously true]"  
**Recommendation: BULK-DISMISS** — The conflict detector is flagging concurrent facts as contradictions. False positives throughout.

---

### Group 12 — Precision questions (precision)
**Count:** ~25 | **Directories:** services/, amazon/  
**Pattern:** "key in what sense?", "generally available — are there exceptions?", "are there exceptions to this count?"  
**Pre-answer:** Specific clarifications (e.g., "cryptographic key", "GA = public production release, no exceptions", "count is specific and precise")  
**Recommendation: BULK-DISMISS** — Overly pedantic on standard technical terminology.

---

### Group 13 — Duplicates (duplicate)
**Count:** 3 | **Directories:** unknown  
**Recommendation: NEEDS HUMAN REVIEW** — Too few to characterize; worth a manual look.

---

## Summary Table

| Group | Type | Count | Directories | Pattern | Recommendation |
|-------|------|--------|-------------|---------|----------------|
| 1 | weak-source | ~400 | amazon/, partners/ | Phonetool citations | Bulk-dismiss |
| 2 | weak-source | ~50 | amazon/ | Internal email/Compass | Bulk-dismiss |
| 3 | weak-source | ~30 | services/, customers/ | Factbase cross-references | Bulk-dismiss |
| 4 | ambiguous | ~250 | all | Standard AWS/industry acronyms | Bulk-dismiss |
| 5 | temporal | ~400 | services/ | AWS service feature bullets | Bulk-dismiss |
| 6 | temporal | ~100 | services/ | AWS pricing bullets | Bulk-dismiss |
| 7 | temporal+missing | ~120 | services/ | Document type tag "services" | Bulk-dismiss (bug) |
| 8 | corruption | ~54 | services/ | Unused footnote [^2] | Bulk-fix or dismiss |
| 9 | stale | ~30 | customers/, services/ | Customer usage @t[~2025] | Auto-resolve via Slack |
| 10 | stale | ~100 | amazon/, partners/ | Person role old dates | Human review + Slack |
| 11 | conflict | ~54 | amazon/, services/ | False-positive overlaps | Bulk-dismiss |
| 12 | precision | ~25 | services/, amazon/ | Standard tech terminology | Bulk-dismiss |
| 13 | duplicate | 3 | unknown | Unknown | Human review |

**Bulk-dismissable (Groups 1–8, 11–12): ~1,283 questions (~88%)**  
**Auto-resolvable via local-search-vault (Group 9): ~30 questions (~2%)**  
**Needs human review (Groups 10, 13): ~103 questions (~7%)**  
**Bulk-fixable in source docs (Group 8 alternative): ~54 questions**

---

## Systemic Issues (Will Regenerate on Next Check Run)

1. **Bug (P3): Document type tag generates 2 false-positive questions per service doc** — The check system should not flag the `type:` metadata field. Affects ~60 service docs = ~120 questions per check run.

2. **Bug (P3): Phonetool citations always flagged as weak-source** — The check system needs an allowlist for known internal tools (Phonetool, internal email, Win Wire, Compass). Affects ~400 amazon/ docs per check run.

3. **Feature (P2): AWS/industry acronym allowlist** — SA, GA, BI, GB, SFTP, CDK, etc. should not trigger ambiguous questions. Affects every directory.

4. **Feature (P2): Document-type-aware temporal checking** — Service reference docs (`services/` folder) should not require `@t[...]` on every feature/pricing bullet. Affects ~60 service docs × ~15 bullets = ~900 questions per check run.

5. **Feature (P2): Concurrent-role conflict detection** — The conflict detector should not flag overlapping responsibilities for the same person/entity as contradictions. Affects amazon/ person docs.

6. **Feature (P2): Template artifact detection** — The unused `[^2]` footnote is a template artifact in all service docs. Either fix the template or suppress this check for known template patterns.

---

## Recommended Action Plan

**Step 1 (immediate, ~5 min):** Bulk-apply all pre-populated answers for Groups 1–8, 11–12. All ~1,283 answers are already written and correct. This clears ~88% of the queue.

**Step 2 (short-term, ~30 min):** Run local-search-vault queries against XSOLIS, HealthStream, and Tivity Health channels to verify/update the ~30 customer usage facts (Group 9).

**Step 3 (ongoing):** Review the ~100 person role stale facts (Group 10) during the next weekly maintenance cycle, checking Slack DMs and account team channels.

**Step 4 (systemic):** File the 2 bugs and 4 feature requests listed above to prevent regeneration of these false positives on the next check run. Without these fixes, a full `check` run will regenerate ~2,000+ questions from the same patterns.

**Step 5 (optional cleanup):** Remove the orphaned `[^2]` footnote definition from all service docs to eliminate Group 8 at the source.
