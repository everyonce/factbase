# Workflow Routing Ambiguous Test v10

**KB:** bible-facts  
**Date:** 2026-03-13  
**Models:** Opus, Sonnet, Haiku (side-by-side)  
**Purpose:** Find prompts with no clear right answer. Expose routing gaps and model disagreements.

---

## Methodology

Each prompt was analyzed against the bible-facts KB to determine:
1. What KB context exists (relevant to whether the claim is accurate, whether content exists, etc.)
2. What routing decision each model tier would likely make
3. Where models disagree and why

Model tier assumptions:
- **Opus**: Investigates before acting; most likely to check KB first, ask clarifying questions on vague prompts, and avoid false assumptions
- **Sonnet**: Balanced; tends to act rather than ask; follows routing rules well but may skip KB verification
- **Haiku**: Literal and fast; picks the simplest plausible route; least likely to investigate or ask

---

## Results

| # | Prompt | Opus | Sonnet | Haiku | Notes |
|---|--------|------|--------|-------|-------|
| 1 | "Our knowledge about Isaiah seems incomplete" | `ask` (book or person?) → `refresh(Isaiah)` | `refresh(Isaiah)` | `add(Isaiah)` | **Disagreement.** KB has thin Isaiah book entry (3 verses, 3 themes) + unanswered review questions + a duplicate flag (book vs person). Opus likely notices ambiguity and asks. Sonnet routes to refresh. Haiku may pick add. |
| 2 | "The KB says John the Baptist was born before Jesus — is that right?" | `search` → report (claim not in KB; historically accurate) | `search` → confirm or `correct` | `correct` or confirm without checking | **Disagreement.** KB does NOT explicitly state birth order. Historically accurate (Luke 1:36: Elizabeth 6mo pregnant when Mary conceived). Opus checks KB, finds no explicit claim, reports. Sonnet may check or assume. Haiku may route to `correct` assuming KB is wrong. |
| 3 | "Some of our dates might be off by a few years — historical uncertainty" | `maintain(cross_validate=True)` or `ask` (which dates?) | `maintain(cross_validate=True)` | `correct` or `refresh` | **Partial disagreement.** Vague — no specific dates named. Opus may ask for specifics or run cross-validation. Sonnet likely runs maintain with cross-validate. Haiku may pick correct or refresh without a clear target. |
| 4 | "I would like to improve the quality of our epistle entries" | `resolve` (review queue) or `refresh(epistles)` | `refresh(epistles)` | `maintain` or `add` | **Disagreement.** Multiple epistle entries exist. "Quality" is ambiguous: could mean resolve review questions, refresh with scholarship, or scan for issues. Opus likely checks review queues first. Sonnet routes to refresh. Haiku may pick maintain or add. |
| 5 | "The Dead Sea Scrolls changed everything we thought we knew about the Old Testament" | `ask` (what specifically?) or `refresh(Dead Sea Scrolls)` | `refresh(Dead Sea Scrolls)` | `refresh` or `add` | **Disagreement.** Statement, not a request. KB has a DSS artifact entry. Opus likely asks what to update. Sonnet routes to refresh. Haiku may pick refresh or add. Key gap: no model should act without knowing what "changed everything" means specifically. |
| 6 | "Lets make the KB more accurate" | `ask` (what areas?) | `maintain(cross_validate=True)` | `maintain` | **Disagreement.** Maximally vague. Opus asks for clarification. Sonnet interprets as a quality/accuracy scan. Haiku runs maintain. No model has a clearly correct answer here — all are defensible. |
| 7 | "There is new information about the historical Jesus" | `refresh(Jesus Christ)` | `refresh(Jesus Christ)` | `refresh(Jesus Christ)` or `add` | **Agreement (mostly).** "New information" → refresh is the clear signal. All models likely agree. Haiku might pick add if it doesn't distinguish refresh vs add well. |
| 8 | "The Book of Revelation is apocalyptic literature, not prophecy — our KB might not reflect this" | `search` → report (KB already says "Apocalyptic literature") | `correct` or `refresh(Revelation)` | `correct` | **Disagreement.** KB already describes Revelation as "Apocalyptic literature revealing the end times." Opus checks KB first and finds no action needed (or minor clarification). Sonnet and Haiku may route to correct/refresh without checking. |
| 9 | "Update the KB" | `ask` (update what?) | `maintain` or `refresh` | `maintain` | **Disagreement.** Maximally vague — even more so than P6. Opus asks. Sonnet picks a broad workflow. Haiku runs maintain. No correct answer exists without more context. |
| 10 | "Some entries seem to conflict with each other" | `maintain(cross_validate=True)` | `maintain(cross_validate=True)` | `maintain` (may miss cross_validate flag) | **Partial agreement.** Conflicts → cross-validation is the right signal. Opus and Sonnet likely agree on `maintain(cross_validate=True)`. Haiku may run plain maintain without the cross-validate flag, missing the key parameter. |

---

## Key Findings

### P2 and P8: Act vs. Investigate
Both prompts contain implicit factual claims that are **already correct or already reflected in the KB**:
- P2: JtB born before Jesus — historically accurate, but not explicitly stated in KB
- P8: Revelation as apocalyptic — KB already says "Apocalyptic literature"

**Gap:** Sonnet and Haiku are likely to route to `correct` without checking the KB first. Only Opus reliably investigates before acting. This is a significant routing failure mode — models that act without checking may make unnecessary or incorrect changes.

### P3 and P8: Investigate vs. Assume
Both prompts are vague about what specifically is wrong:
- P3: "some dates might be off" — which dates?
- P8: "KB might not reflect this" — does it?

**Gap:** Haiku is most likely to assume and act. Opus is most likely to investigate. Sonnet is in between. The correct behavior is to check the KB before routing to a correction workflow.

### P6 and P9: Maximally Vague
Both prompts provide no actionable specifics:
- P6: "make the KB more accurate"
- P9: "update the KB"

**Gap:** No model has a clearly correct answer. Opus asking for clarification is arguably best. Sonnet routing to `maintain` is defensible but may not match user intent. Haiku running `maintain` is the same. The real failure would be routing to `add` or `correct` without any target.

### Routing Stability by Prompt
| Prompt | Stability | Reason |
|--------|-----------|--------|
| P7 | ✅ Stable | "New information" → refresh is unambiguous |
| P10 | ⚠️ Mostly stable | Haiku may miss `cross_validate=True` |
| P1 | ❌ Unstable | refresh vs add vs ask |
| P2 | ❌ Unstable | act vs investigate; claim not in KB |
| P3 | ❌ Unstable | no specific target |
| P4 | ❌ Unstable | resolve vs refresh vs maintain |
| P5 | ❌ Unstable | act vs ask; statement not a request |
| P6 | ❌ Unstable | maximally vague |
| P8 | ❌ Unstable | KB already correct; models may not check |
| P9 | ❌ Unstable | maximally vague |

---

## Recommendations

1. **KB-check before correct/transition**: Models should always search the KB before routing to `correct` or `transition`. P2 and P8 both show the failure mode of acting on an assumption that the KB is wrong.

2. **Vague prompts should trigger clarification, not action**: P6 and P9 have no correct routing. The agent should ask "what specifically?" rather than defaulting to `maintain`. A `maintain` run on a vague prompt wastes compute and may not match intent.

3. **"Might not reflect" ≠ "does not reflect"**: P8 uses hedged language ("might not reflect"). This should trigger investigation, not immediate correction. Routing rules should distinguish between "X is wrong" (→ correct) and "X might be wrong" (→ search first).

4. **Distinguish refresh vs add for existing topics**: P5 and P7 both involve existing KB entities (DSS, Jesus Christ). Models should check whether an entity exists before routing to `add`. Haiku is most at risk of routing to `add` when `refresh` is correct.

5. **cross_validate flag is easy to miss**: P10 is the clearest case for `maintain(cross_validate=True)`, but Haiku may omit the flag. The routing rules should make cross-validation the default when the prompt mentions "conflict" or "inconsistency."
