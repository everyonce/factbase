# Prompt Eval v2 — KB Setup Notes

**KB path:** `/Volumes/dev/factbase-test/prompt-eval-v2`  
**Git baseline tag:** `eval-v2-baseline`  
**Created:** 2026-03-16  
**Purpose:** Shared starting point for all 30×3 = 90 isolated test sessions

---

## Domain

History of jazz standards — Miles Davis, Thelonious Monk, John Coltrane, Kind of Blue, Blue Note Records, Village Vanguard venue.

## KB Structure

```
prompt-eval-v2/
├── perspective.yaml
├── musicians/
│   ├── miles-davis.md          ← FALSE: birth year 1928 (correct: 1926)
│   ├── thelonious-monk.md
│   ├── john-coltrane.md        ← missing @t on "Signature Style" section
│   ├── john-lewis.md           ← missing @t on career/style facts
│   └── john-mclaughlin.md
├── recordings/
│   ├── kind-of-blue.md         ← FALSE: recorded 1960 (correct: 1959)
│   ├── a-love-supreme.md
│   └── monks-dream.md
├── labels/
│   └── blue-note-records.md    ← FALSE: founded 1942 (correct: 1939); uses undefined acronym "BNR"
├── venues/
│   └── village-vanguard.md     ← FALSE: capacity 200 seats (correct: ~123)
└── glossary/
    ├── aaba-form.md             ← AABA defined here (known acronym)
    └── ii-v-i-progression.md
```

## Scan Results (baseline)

- **Documents:** 12 active (5 musician, 3 recording, 1 label, 1 venue, 2 glossary)
- **Facts:** 159 total
- **Links detected:** 26
- **Temporal coverage:** 92% (147/159 facts have @t tags)
- **Source coverage:** 78% (124/159 facts have citations)
- **Documents below 80% source threshold:** 2

## Pre-loaded Test Fixtures

### False Claims (for steps 7, 8, 15, 17)

| Document | False Claim | Correct Value |
|---|---|---|
| `musicians/miles-davis.md` | Born 1928 | Born 1926 |
| `recordings/kind-of-blue.md` | Recorded 1960 | Recorded 1959 |
| `labels/blue-note-records.md` | Founded 1942 | Founded 1939 |
| `venues/village-vanguard.md` | Capacity ~200 seats | Capacity ~123 seats |

### Multiple "John" Entities (for step 11 clarification test)

Three musicians named "John":
- `john-coltrane.md` — tenor saxophonist, Miles Davis collaborator
- `john-lewis.md` — pianist, co-founder of Modern Jazz Quartet
- `john-mclaughlin.md` — guitarist, jazz fusion pioneer

### Missing @t Tags (for steps 22-25)

Facts without temporal tags appear in:
- `musicians/john-coltrane.md` — "Signature Style" section (3 facts)
- `musicians/john-lewis.md` — career/style narrative facts (4 facts)
- `recordings/a-love-supreme.md` — "Spiritual and devotional in character" fact
- `recordings/monks-dream.md` — "Best-selling album of Monk's career" fact
- All glossary entries (structural music theory facts tagged @t[?])

### Acronyms (for steps 26-27)

- **AABA** — defined in `glossary/aaba-form.md` (known acronym)
- **BNR** — used in `labels/blue-note-records.md` body text but NOT defined anywhere in the KB (unknown acronym)

## Resetting to Baseline

Each test session should start from a clean copy:

```bash
# Option 1: Clone the baseline tag into a fresh directory
git clone /Volumes/dev/factbase-test/prompt-eval-v2 /tmp/eval-session-N
cd /tmp/eval-session-N
git checkout eval-v2-baseline

# Option 2: Reset in-place (destructive)
cd /Volumes/dev/factbase-test/prompt-eval-v2
git reset --hard eval-v2-baseline
git clean -fd
```

## Notes

- The `.fastembed_cache/` is committed so sessions don't need to re-download the BGE-small model.
- The `.factbase/factbase.db` is committed at baseline state so sessions start with a pre-indexed KB.
- `perspective.yaml` configures required fields: `musician` needs `birth_year` + `primary_instrument`; `recording` needs `year` + `label`; `label` needs `founded_year` + `location`.
- The `label` type has a `location` required field — `blue-note-records.md` is missing this, which will surface as a review question.
