# Obsidian Footnote Hover Preview: Sources Callout Investigation

## Research Question

Would wrapping footnote definitions in a `> [!sources]- Sources` callout break
Obsidian's hover preview for `[^1]` references in the document body?

## Test Cases

### Test A — Baseline (current format, definitions at document level)

```markdown
- Some fact that was verified [^1]
- Another fact from a different source [^2]

---

[^1]: LinkedIn profile, scraped 2024-01-15
[^2]: Company website, accessed 2024-03
```

**Expected**: Hovering `[^1]` shows "LinkedIn profile, scraped 2024-01-15". ✅ Works.

---

### Test B — Definitions inside `> [!sources]- Sources` callout

```markdown
- Some fact that was verified [^1]
- Another fact from a different source [^2]

> [!sources]- Sources
> [^1]: LinkedIn profile, scraped 2024-01-15
> [^2]: Company website, accessed 2024-03
```

**Expected**: Hovering `[^1]` shows the source definition.
**Actual**: ❌ **BROKEN** — see findings below.

---

## Findings

### Why It Breaks

Obsidian's footnote parser follows the CommonMark/GFM specification: footnote
definitions must appear at the **document block level**, not inside blockquotes.
A callout is syntactic sugar over a blockquote (`> `-prefixed lines). When
definitions are prefixed with `> `, the parser does not recognise them as
footnote definitions — they are treated as plain blockquote text.

Consequences:
1. `[^1]` references in the body render as plain text `[^1]` (no superscript,
   no hover preview) in Live Preview mode.
2. The `> [^1]: ...` lines inside the callout render as plain text, not as
   footnote definitions.
3. Reading View also loses the footnote link — clicking `[^1]` does nothing.

### Evidence from Obsidian Community

Multiple confirmed reports on the Obsidian forum:

- *"Footnotes are currently not supported in Callouts and in Tables"*
  — [forum.obsidian.md/t/75904](https://forum.obsidian.md/t/footnotes-are-not-rendered-in-live-preview-mode/75904)

- *"Footnote References in Callouts Don't Display Correctly in Live Preview"*
  — [forum.obsidian.md/t/62784](https://forum.obsidian.md/t/footnote-references-in-callouts-dont-display-correctly-in-live-preview/62784)
  (filed as bug, moved to Bug Graveyard — not fixed)

- *"Live Preview: Support Footnotes in Callouts"* — open feature request since 2022,
  still unresolved as of 2024.
  — [forum.obsidian.md/t/36606](https://forum.obsidian.md/t/live-preview-support-footnotes-in-callouts/36606)

Note: the forum posts discuss references inside callouts; the same underlying
limitation applies to definitions inside callouts — the parser does not process
footnote syntax inside blockquote/callout blocks.

## Decision

**Do not implement sources callout wrapping.**

The `[!review]- Review Queue` callout works because factbase controls the entire
review section and strips the `> ` prefix before processing. Source footnotes are
different: Obsidian itself must parse `[^N]` references for hover preview, and it
cannot do so when definitions are inside a callout.

Keeping footnote definitions at the document level (current behaviour) is the
only way to preserve Obsidian's native hover preview for `[^N]` references.

## Alternative Considered

A `> [!sources]- Sources` callout would visually collapse the sources section
(useful for long footnote lists). However, the broken hover preview is a
significant UX regression that outweighs the cosmetic benefit.

If Obsidian ever fixes footnote support inside callouts, this decision should be
revisited.
