# Post-Maintain Deferred Report

**Generated:** 2026-03-13  
**KB:** /Users/daniel/work/factbase-docs

---

## Queue Status Summary

| Metric | Count |
|--------|-------|
| Original (pre-fix) | 5,462 |
| Pre-maintain | 3,986 |
| **Post-maintain (current)** | **318** |
| Reduction from pre-maintain | 3,668 (92%) |
| Unanswered | 0 |
| Deferred (needs attention) | 318 |
| Answered (pending apply) | 85 |

---

## Remaining Questions by Type

| Type | Count | Description |
|------|-------|-------------|
| stale | 107 | Facts with outdated temporal tags (source date older than tag) |
| weak-source | 101 | Citations flagged as insufficiently specific |
| corruption | 54 | Orphaned footnote definitions (`[^N]` defined but never referenced) |
| temporal | 29 | Facts missing a `@t[...]` tag entirely |
| missing | 19 | Facts with no source citation at all |
| ambiguous | 7 | Acronyms or terms needing clarification |
| precision | 1 | Vague language that could be made more specific |
| **Total** | **318** | |

---

## Remaining Questions by Directory

| Directory | Total | stale | weak-source | corruption | temporal | missing | ambiguous | precision |
|-----------|-------|-------|-------------|------------|----------|---------|-----------|-----------|
| amazon/employees | 148 | 68 | 30 | 0 | 28 | 19 | 3 | 0 |
| services | 127 | 33 | 40 | 53 | 1 | 0 | 0 | 0 |
| customers/xsolis | 25 | 0 | 22 | 0 | 0 | 0 | 2 | 1 |
| customers/collabrios | 14 | 6 | 7 | 1 | 0 | 0 | 0 | 0 |
| customers/healthstream | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 |
| **Total** | **318** | **107** | **101** | **54** | **29** | **19** | **7** | **1** |

---

## Actionability Classification

### Auto-fixable by Agent (estimated ~155 items)

These can be resolved without human input:

| Type | Count | Action |
|------|-------|--------|
| corruption | 54 | Remove orphaned `[^2]` footnote definitions from service docs — all are template artifacts confirmed in the deferred answers |
| weak-source (internal KB cross-refs) | ~50 | Dismiss — citations like "Existing factbase-docs customer documents" and "XSOLIS AWS Infrastructure doc (factbase-docs)" are internal cross-references; all have `believed: dismiss` answers already written |
| weak-source (Slack/email/Phonetool) | ~30 | Dismiss — Slack DMs, email threads, and Phonetool lookups with dates are sufficiently specific for internal KB; answers already written |
| ambiguous (acronyms) | 7 | Resolve — BNA12, P2P, SE, MJ all have `believed:` answers already written |
| precision | 1 | Resolve — DragonFly API blocker clarification has answer written |

**Blocker:** These 155 items all have `believed:` or `dismiss:` answers already written in the deferred queue but were not applied. Running `apply_review_answers` should clear them.

### Needs Human Review (~163 items)

These require either fresh data or a human decision:

| Directory | Type | Count | Why Human Needed |
|-----------|------|-------|-----------------|
| amazon/employees | stale | 68 | Role/location facts from 2018–2023 sources; need Phonetool or LinkedIn check to verify current status |
| amazon/employees | temporal | 28 | Facts with no `@t` tag; need human to confirm approximate date range |
| amazon/employees | missing | 19 | No source at all; need human to confirm or add citation |
| services | stale | 33 | Customer usage facts tagged `@t[~2025]` that may need updating to `@t[~2026]` |
| services | temporal | 1 | HealthStream GWLB/Palo Alto — needs account team confirmation |
| customers/collabrios | stale | 6 | Product facts from 2025-07 Slack; likely still accurate but need re-verification |
| customers/healthstream | ambiguous | 2 | "MJ" — initials of HealthStream contact; full name not in KB |

### Should Be Suppressed / False Positive Pattern (~0 new patterns found)

No new suppression patterns identified. The `weak-source` pattern for internal KB cross-references was already identified in the pre-maintain pass. The remaining `weak-source` items in `services/` follow the same pattern (all have `believed: dismiss` answers written).

---

## Key Findings

1. **92% reduction achieved.** The maintain pass cleared 3,668 questions, dropping from 3,986 to 318.

2. **~155 items are already answered but not applied.** The deferred queue contains items with `believed:` answers written but `answered: false`. These should be clearable by running `apply_review_answers` on the deferred queue.

3. **Dominant pattern in `services/` is corruption (53/127).** All are orphaned `[^2]` footnote template artifacts. This is a systematic issue from the service doc generation template — a bulk fix (remove unused `[^2]` footnotes) would clear ~42% of service questions.

4. **Dominant pattern in `amazon/employees` is stale (68/148).** Most are people docs with 2018–2023 sources. These require Phonetool/LinkedIn verification and cannot be auto-resolved.

5. **`customers/xsolis` weak-source (22/25)** are all internal KB cross-references with `believed: dismiss` answers already written — auto-fixable.

---

## Recommended Next Steps

1. **Run `apply_review_answers`** — should clear ~155 items that already have written answers.
2. **Bulk remove orphaned `[^2]` footnotes** from service docs — clears 53 corruption items.
3. **Phonetool sweep** for `amazon/employees` stale/temporal items — prioritize active account team members.
4. **Update `@t[~2025]` → `@t[~2026]`** on service docs where customer usage is confirmed still current.
