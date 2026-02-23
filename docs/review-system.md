# Document Review System

## Overview

Factbase can analyze fact documents for inconsistencies, missing data, and ambiguities, then generate questions for human review. This creates a feedback loop where the system identifies issues and users provide clarifications that get applied back to the documents.

## Commands

### `factbase lint --review [--repo <repo>]`

Analyzes documents and appends a Review Queue section with generated questions.

**Behavior:**
- Scans documents for issues (see Detection Rules below)
- Appends `<!-- factbase:review -->` section if questions generated
- Preserves existing unanswered questions
- Skips documents with no issues detected

**Flags:**
- `--repo <repo>` - Limit to specific repository
- `--stale-days <n>` - Flag `@t[~...]` facts older than N days (default: 365)
- `--dry-run` - Show questions without modifying files

### `factbase review --apply [--repo <repo>]`

Processes answered questions and updates documents.

**Behavior:**
- Finds questions marked `[x]` with non-empty blockquotes
- Collects ALL answered questions for a document
- Uses LLM to interpret answers together and rewrite affected sections
- Batch processing produces more consistent results than line-by-line patches
- Removes applied questions from Review Queue
- Removes Review Queue section if empty after processing

**Flags:**
- `--repo <repo>` - Limit to specific repository
- `--dry-run` - Show proposed changes without applying

### `factbase review --status [--repo <repo>]`

Shows summary of pending questions across documents.

**Output:**
```
Review Status
=============
Documents with questions: 12
Total questions: 34
  - temporal: 15
  - conflict: 8
  - missing: 6
  - ambiguous: 3
  - stale: 2

Answered (ready to apply): 7
```

## Detection Rules

### `@q[temporal]` - Missing Time Information

Triggers:
- Fact has no `@t[...]` tag
- Role/position without end date and `@t[...]` is >1 year old
- Date range with missing start or end where context suggests it's known

### `@q[conflict]` - Contradictory Facts

Triggers:
- Overlapping date ranges for mutually exclusive facts (e.g., two full-time jobs)
- Same fact with different values (e.g., two different graduation years)
- Timeline gaps that seem implausible

### `@q[missing]` - Missing Data

Triggers:
- Fact without source reference `[^N]`
- Footnote reference without definition
- Expected fields missing (configurable per document type in perspective.yaml)

### `@q[ambiguous]` - Unclear Meaning

Triggers:
- LLM-detected ambiguity in phrasing
- Location without context (home vs. work vs. birth)
- Relationship without direction (advisor to whom?)

### `@q[stale]` - Potentially Outdated

Triggers:
- `@t[~...]` date older than threshold (default: 365 days)
- `@t[YYYY..]` ongoing facts older than threshold
- Source scraped date significantly older than fact date

### `@q[duplicate]` - Possible Duplicates

Triggers:
- High similarity score with another document (>95%)
- Same name/title with different IDs
- Cross-document: same entity mentioned with conflicting details

## Answer Processing

When `review --apply` processes answered questions, it:

1. Collects all answered questions for the document
2. Groups questions by affected section/lines
3. Sends to LLM with full context:
   ```
   Document section:
   ## Career
   - CTO at Acme Corp @t[2020..2022] [^1]
   - VP Engineering at BigCo @t[2022..] [^2]
   
   Answered questions:
   1. Q: "VP Engineering at BigCo" has no end date - is this role still current?
      A: "No, left in March 2024"
   2. Q: CTO ended 2022, VP started 2022 - same month? Overlap?
      A: "CTO ended Feb 2022, VP started March 2022, no overlap"
   
   Rewrite the section incorporating all answers with correct temporal tags.
   ```
4. Replaces entire section with LLM output
5. Removes processed questions from Review Queue

### Special Answers

- `dismiss` or `ignore` - Remove question without changes
- `delete` - Remove the fact entirely
- `split: ...` - Split fact into multiple lines (LLM interprets)

## File Format

Review Queue is always at the end of the document, after footnotes:

```markdown
<!-- factbase:a1b2c3 -->
# Document Title

Content...

---
[^1]: Source

---
<!-- factbase:review -->
## Review Queue

- [ ] `@q[type]` Description
  > 
```

The `<!-- factbase:review -->` comment marks the section for programmatic detection.

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

In `config.yaml` (global):

```yaml
review:
  model: rnj-1-extended  # LLM for question generation and answer processing
  # Defaults to llm.model if not specified
```

## Implementation Notes

### LLM Prompts

Question generation prompt should:
- Receive full document content
- Receive list of all entity titles (for duplicate detection)
- Return structured JSON of questions with line numbers

Answer application prompt should:
- Receive original line, question, and answer
- Return replacement line(s) with proper `@t[...]` and `[^N]` formatting
- Handle multi-line responses for `split:` answers

### Database Schema

No schema changes required. Review Queue lives in the markdown files only.

### Concurrency

- `lint --review` can run in parallel across documents
- `review --apply` should process one document at a time to avoid conflicts
