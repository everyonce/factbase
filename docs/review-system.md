# Document Review System

## Overview

Factbase can analyze fact documents for inconsistencies, missing data, and ambiguities, then generate questions for human review. This creates a feedback loop where the system identifies issues and users provide clarifications that get applied back to the documents.

## Commands

### `factbase check [--repo <repo>]`

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
- Source definition lacking traceability (e.g., just "Slack message" or "Outlook" with no channel, date, URL, or subject)
- Expected fields missing (configurable per document type in perspective.yaml)

### `@q[ambiguous]` - Unclear Meaning

Triggers:
- Location without context (home vs. work vs. birth)
- Relationship without direction (advisor to whom?)
- Undefined acronyms or abbreviations (e.g., "TAM" without expansion)

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

### `@q[corruption]` - Data Corruption

Triggers:
- Temporal tag contains non-date content (entity names, descriptions, statistics)
- Malformed temporal tag syntax

## Answer Processing

When `review --apply` processes answered questions, it classifies each answer and handles it accordingly:

### Deterministic Processing (no LLM needed)
- **Source citations** (e.g., "LinkedIn profile, 2024-01-15") → adds footnote to the fact
- **Confirmations** (e.g., "still accurate", "confirmed") → updates `@t[~date]` to today, preserving the `~` prefix
- **Deletions** (e.g., "delete") → removes the fact line
- **Dismissals** (e.g., "dismiss", "ignore") → removes the question, no content changes

### LLM-Assisted Processing
- **Corrections** (e.g., "Actually left in March 2024") → LLM rewrites the affected section
- **Complex changes** → LLM interprets the answer in context and rewrites

### Reviewed Markers

After processing, affected fact lines receive a `<!-- reviewed:YYYY-MM-DD -->` marker. Lint skips recently-reviewed facts (within 180 days) to prevent regenerating the same questions.

### Special Answers

- `dismiss` or `ignore` - Remove question without changes
- `defer: <note>` - Keep question in queue with your note for future reviewers
- `delete` - Remove the fact entirely
- `split: ...` - Split fact into multiple lines (LLM interprets)

### Question Lifecycle

```
[Generated]  → [ ] Unanswered, empty blockquote
[Answered]   → [x] Checked, blockquote filled → apply processes it
[Applied]    → Removed from queue, fact updated, reviewed marker added
[Dismissed]  → [x] "dismiss" → removed from queue, reviewed marker added
[Deferred]   → [x] "defer: note" → unchecked, note preserved for future
[Pruned]     → Removed automatically by check when trigger condition no longer exists
```

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
# LLM used for answer processing and cross-validation
# Defaults to the llm.model setting
llm:
  provider: bedrock
  model: us.anthropic.claude-haiku-4-5-20251001-v1:0
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

- `check` can run in parallel across documents
- `review --apply` should process one document at a time to avoid conflicts
