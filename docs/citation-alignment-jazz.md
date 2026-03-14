# Citation Alignment: Jazz KB — Weak-Source Verification

**Date:** 2026-03-14  
**KB:** `/tmp/factbase-test-jazz`  
**Purpose:** Verify that liner notes and book citations are no longer flagged as weak-source after PR #607 (print citation fix).

---

## Summary

**Expected:** 0 weak-source questions  
**Actual:** 7 weak-source questions (6 deferred + 1 new open)

The fix did **not** fully resolve the false positives.

---

## Findings

### New questions generated (this run)

`factbase(op=check)` generated **1 new weak-source question**:

| Doc | Citation | Status |
|-----|----------|--------|
| Blue Note Records (`760fe0`) | Richard Cook, *Blue Note Records: The Biography*, Justin, Charles & Co., 2003 | open |

This is a book citation — the type the fix was intended to suppress. A new weak-source question was generated despite #607.

### Deferred weak-source questions (pre-existing, 6 total)

These were previously answered with `believed` status but remain in the queue as deferred:

| Doc | Citation type | Citation |
|-----|--------------|----------|
| Kind of Blue (`aedd2a`) | Liner notes | Bill Evans, liner notes, *Kind of Blue*, Columbia Records, CL 1355 / CS 8163, 1959 |
| Miles Davis (`15c3a1`) | Liner notes | Nat Hentoff, liner notes, *Sketches of Spain*, Columbia Records, CL 1480 / CS 8271, 1960 |
| Modal Jazz (`6c631f`) | Liner notes | Ira Gitler, liner notes, *My Favorite Things*, Atlantic Records, SD 1361, 1961 |
| A Love Supreme (`de5b07`) | Liner notes | Original liner notes, *A Love Supreme*, Impulse! Records, A-77, 1965 |
| Blue Note Records (`760fe0`) | Book | Michael Cuscuna & Michel Ruppli, *The Blue Note Label: A Discography*, Greenwood Press, 2001 |
| Blue Note Records (`760fe0`) | Book | Richard Cook, *Blue Note Records: The Biography*, Justin, Charles & Co., 2003 |

### Specific checks requested

- **Bill Evans liner notes / Columbia CL 1355** — still flagged (deferred, Kind of Blue doc)
- **Book citations (Ashley Kahn / ISBN)** — Ashley Kahn not present in queue; Richard Cook and Cuscuna/Ruppli book citations still flagged (deferred + 1 new open)

---

## Conclusion

PR #607 did **not** achieve the expected result of 0 weak-source questions:

- 6 pre-existing weak-source questions remain in the deferred queue (liner notes × 4, books × 2)
- 1 **new** weak-source question was generated on a book citation (Richard Cook, Blue Note Records), indicating the fix does not suppress book citations during `check`

The fix may need to be revisited to ensure the suppression logic applies during question generation, not just at answer time.
