# Document Review System

## Overview

Factbase analyzes fact documents for inconsistencies, missing data, and ambiguities, then generates questions for human or agent review. This creates a feedback loop where the system identifies issues and users provide clarifications that get applied back to the documents.

## How It Works

The review system operates through MCP tools. An AI agent (or human via the web UI) drives the process:

1. **Generate questions** — `factbase(op='check')` analyzes documents and appends a Review Queue section
2. **Answer questions** — Agent or human provides answers via `factbase(op='answer')` or by editing markdown files
3. **Apply answers** — Agent rewrites documents via `factbase(op='update')` based on answers
4. **Repeat** — Each cycle produces fewer questions until documents stabilize

## Review Queue Format

Questions are appended to documents in a structured Review Queue section:

```markdown
---
<!-- factbase:review -->
## Review Queue

- [ ] `@q[temporal]` Line 5: "VP Engineering at BigCo" has no end date - is this role still current?
  > 

- [ ] `@q[conflict]` Lines 4-5: CTO ended 2022, VP started 2022 - same month? Overlap?
  > 

- [x] `@q[ambiguous]` "Based in Austin" - is this home or work location?
  > Home address, works remote
```

The `<!-- factbase:review -->` comment marks the section for programmatic detection.

## Question Types

| Tag | Meaning |
|-----|---------|
| `@q[temporal]` | Missing or unclear time information |
| `@q[conflict]` | Contradictory facts detected |
| `@q[missing]` | Missing source or expected data |
| `@q[ambiguous]` | Unclear meaning, needs clarification |
| `@q[stale]` | Data may be outdated (based on `@t[~...]` age) |
| `@q[duplicate]` | Possible duplicate of another entity |
| `@q[corruption]` | Data corruption (malformed temporal tags, non-date content in `@t[...]`) |
| `@q[precision]` | Imprecise language that could change truth value (vague qualifiers) |
| `@q[weak-source]` | Source citation lacks traceability |

## Detection Rules

### `@q[temporal]` - Missing Time Information
- Fact has no `@t[...]` tag
- Role/position without end date and `@t[...]` is >1 year old
- Date range with missing start or end where context suggests it's known

### `@q[conflict]` - Contradictory Facts
- Overlapping date ranges for mutually exclusive facts
- Same fact with different values
- Timeline gaps that seem implausible
- Cross-document: fact-level embeddings detect semantically similar facts across documents

### `@q[missing]` - Missing Data
- Fact without source reference `[^N]`
- Footnote reference without definition
- Source definition lacking traceability
- Expected fields missing (configurable per document type in perspective.yaml)

### `@q[ambiguous]` - Unclear Meaning
- Location without context (home vs. work vs. birth)
- Relationship without direction
- Undefined acronyms or abbreviations

### `@q[stale]` - Potentially Outdated
- `@t[~...]` date older than threshold (default: 365 days)
- `@t[YYYY..]` ongoing facts older than threshold

### `@q[duplicate]` - Possible Duplicates
- High similarity score with another document (>95%)
- Same name/title with different IDs

### `@q[corruption]` - Data Corruption
- Temporal tag contains non-date content
- Malformed temporal tag syntax

### `@q[precision]` - Imprecise Language
- Vague qualifiers whose interpretation could change truth value
- Ambiguous quantities, vague time references, ambiguous scope

## Answering Questions

### Via Markdown Editing

Check the checkbox `[x]` and add your answer in the blockquote:

```markdown
- [x] `@q[temporal]` Line 5: "VP Engineering at BigCo" - when was this true?
  > Started March 2022, left December 2024
```

### Via MCP

Use `factbase(op='answer')` to submit answers programmatically.

### Special Answers

- `dismiss` or `ignore` - Remove question without changes
- `defer: <note>` - Keep question in queue with your note for future reviewers
- `delete` - Remove the referenced fact entirely
- `split: <instruction>` - Split fact into multiple lines

## Answer Processing

The agent processes answered questions by:

1. Reading the answered questions from the review queue
2. Interpreting answers to determine changes needed
3. Rewriting the document via `factbase(op='update')` with appropriate temporal tags and sources
4. Removing applied questions from the Review Queue

### Deterministic Processing
- **Confirmations** (e.g., "still accurate") → updates `@t[~date]` to today
- **Deletions** (e.g., "delete") → removes the fact line
- **Dismissals** (e.g., "dismiss") → removes the question, no content changes

### Agent-Driven Processing
- **Corrections** (e.g., "Actually left in March 2024") → agent rewrites the affected section
- **Complex changes** → agent interprets the answer in context and rewrites

### Reviewed Markers

After processing, affected fact lines receive a `<!-- reviewed:YYYY-MM-DD -->` marker. Quality checks skip recently-reviewed facts (within 180 days) to prevent regenerating the same questions.

#### Type-specific markers

A bare marker suppresses **all** question types for 180 days. You can also target a specific question type using an optional prefix:

```
<!-- reviewed:YYYY-MM-DD -->        ← suppresses all question types (backward compatible)
<!-- reviewed:p:YYYY-MM-DD -->      ← suppresses precision questions only
<!-- reviewed:t:YYYY-MM-DD -->      ← suppresses temporal questions only
<!-- reviewed:a:YYYY-MM-DD -->      ← suppresses ambiguous questions only
<!-- reviewed:s:YYYY-MM-DD -->      ← suppresses stale questions only
```

Multiple markers on the same line are allowed, each suppressing a different type independently:

```markdown
- ATK is primarily @t[~2024] [^1] <!-- reviewed:p:2026-03-22 --> <!-- reviewed:t:2026-03-22 -->
```

This is useful when a fact's temporal claim has been verified but its precision hasn't been addressed yet (or vice versa).

## Question Lifecycle

```
[Generated]  → [ ] Unanswered, empty blockquote
[Answered]   → [x] Checked, blockquote filled → agent processes it
[Applied]    → Removed from queue, fact updated, reviewed marker added
[Dismissed]  → [x] "dismiss" → removed from queue, reviewed marker added
[Deferred]   → [x] "defer: note" → unchecked, note preserved for future
[Pruned]     → Removed automatically by check when trigger condition no longer exists
```

## Configuration

In `perspective.yaml` (per-repository):

```yaml
review:
  stale_days: 365
  required_fields:
    person:
      - current_role
      - location
    company:
      - founded
      - headquarters
  ignore_patterns:
    - "*.draft.md"
```
