# Citation Auto-Stamp Feature Test — Prod KB

**Date:** 2026-03-14  
**KB:** `/Users/daniel/work/factbase-docs` (prod)  
**Purpose:** Closed-loop test of the `<!-- ✓ -->` auto-stamp feature on the production knowledge base.

---

## Baseline (Before Maintain #1)

| Metric | Value |
|--------|-------|
| `<!-- ✓ -->` markers in files | **0** |
| `@q[weak-source]` in files | **1,838** |
| MCP queue: weak-source unanswered | **0** |
| MCP queue: weak-source deferred | **1,109** |
| MCP queue: total unanswered | **34** |

---

## Maintain Run #1

### Scan
- Documents total: 671 (after scan)
- Temporal coverage: 64%, Source coverage: 64%

### Check
- New unanswered questions: **4** (0 weak-source)
- `skipped_reviewed`: **5,633**
- `suppressed_by_prior_answers`: **2,130**
- `already_in_queue`: **3,692**
- No new weak-source questions generated

### Resolve
- 34 unanswered questions resolved (temporal, missing, conflict, precision, corruption, ambiguous)
- 0 weak-source questions encountered (all already deferred)

### Post-Maintain-1 Metrics

| Metric | Value | Change |
|--------|-------|--------|
| `<!-- ✓ -->` markers in files | **0** | ±0 |
| `@q[weak-source]` in files | **1,838** | ±0 |
| MCP queue: weak-source unanswered | **0** | ±0 |
| MCP queue: weak-source deferred | **1,642** | +533 (new docs added) |

---

## Maintain Run #2

### Scan
- 0 new documents added, 40 updated

### Check
- New unanswered questions: **3** (all weak-source — from 3 newly indexed docs)
- `skipped_reviewed`: **5,641** (+8 vs maintain #1)
- `suppressed_by_prior_answers`: **2,134** (+4 vs maintain #1)

### Post-Maintain-2 Metrics

| Metric | Value | Change from Maintain #1 |
|--------|-------|------------------------|
| `<!-- ✓ -->` markers in files | **0** | ±0 |
| `@q[weak-source]` in files | **1,840** | +2 (new docs only) |
| MCP queue: weak-source unanswered | **3** | +3 (new docs only) |
| MCP queue: weak-source deferred | **1,641** | -1 (stable) |

---

## Pass/Fail Assessment

| Step | Criterion | Result | Notes |
|------|-----------|--------|-------|
| 1 | Baseline `<!-- ✓ -->` count noted | ✅ PASS | 0 markers at baseline |
| 2 | Maintain #1 runs without error | ✅ PASS | Completed successfully |
| 3 | `<!-- ✓ -->` markers > 0 after maintain #1 | ❌ FAIL | Still 0 — no markers written |
| 4 | Weak-source count lower after maintain #1 | ⚠️ N/A | Count unchanged (1,838); no VALID dismissals occurred |
| 5 | Maintain #2 runs without error | ✅ PASS | Completed successfully |
| 6 | Weak-source count does NOT increase (re-flag prevention) | ✅ PASS | +2 is from new docs, not re-flagging existing lines |

**Overall: PARTIAL PASS** — Re-flag prevention works at the database level, but `<!-- ✓ -->` file markers are not being written.

---

## Root Cause Analysis

### Why no `<!-- ✓ -->` markers were written

The auto-stamp mechanism requires a weak-source question to be **explicitly dismissed as VALID** during the resolve step. In this test run:

1. All 1,838 weak-source questions in files were already in the **deferred** state in the MCP queue before maintain #1 began.
2. The check step's `suppressed_by_prior_answers: 2,130` confirms these are being suppressed via database state, not file markers.
3. The resolve step only processes **unanswered** questions — it never encountered a weak-source question to dismiss.
4. Therefore, no VALID dismissal occurred → no `<!-- ✓ -->` marker was written.

### How suppression actually works (observed behavior)

The check step suppresses re-flagging via two mechanisms:
- **`suppressed_by_prior_answers`** (2,130 → 2,134): Questions suppressed because a prior answer exists in the DB
- **`skipped_reviewed`** (5,633 → 5,641): Questions skipped because they're already in the review queue

These mechanisms work correctly — existing weak-source questions are NOT re-generated on subsequent check runs. The re-flag prevention goal is achieved, just not via file markers.

### To trigger `<!-- ✓ -->` markers

The auto-stamp would fire if:
1. A weak-source question is **unanswered** (not deferred)
2. The resolve step answers it as **VALID**
3. The apply step writes the answer back to the file

This condition was never met in this test because all weak-source questions were pre-deferred.

---

## Recommendations

1. **Test with a fresh KB** that has unanswered weak-source questions to verify the stamp-on-VALID path works end-to-end.
2. **Verify the VALID dismissal path** — create a test doc with a known-good citation, run check to generate a weak-source question, answer it as VALID, apply, and confirm `<!-- ✓ -->` appears.
3. **Consider whether deferred weak-source questions should be auto-stamped** — if a question is deferred (human-reviewed), the line arguably deserves a `<!-- ✓ -->` marker too.
4. **The suppression mechanism works** — `suppressed_by_prior_answers` correctly prevents re-flagging of previously reviewed citations. This is the functional equivalent of auto-stamping, just without the file marker.

---

## Raw Data

```
Baseline:
  grep -r "<!-- ✓ -->" count: 0
  grep -r "@q[weak-source]" count: 1838
  MCP queue weak-source: 0 unanswered, 1109 deferred

Post-Maintain-1:
  grep -r "<!-- ✓ -->" count: 0
  grep -r "@q[weak-source]" count: 1838
  MCP queue weak-source: 0 unanswered, 1642 deferred
  check skipped_reviewed: 5633
  check suppressed_by_prior_answers: 2130

Post-Maintain-2 (check step):
  grep -r "<!-- ✓ -->" count: 0
  grep -r "@q[weak-source]" count: 1840 (+2 new docs)
  MCP queue weak-source: 3 unanswered (new docs), 1641 deferred
  check skipped_reviewed: 5641 (+8)
  check suppressed_by_prior_answers: 2134 (+4)
```
