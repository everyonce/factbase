# Fact Document Format Specification

Version: 1.1.0

This document defines the standard format for fact documents in Factbase, including temporal annotations and source attribution.

> **Related guides:**
> - [authoring-guide.md](authoring-guide.md) - Human-focused authoring tips
> - [agent-authoring-guide.md](agent-authoring-guide.md) - Comprehensive guide for AI agents
> - [review-system.md](review-system.md) - Human-in-the-loop review workflow

## Document Structure

```markdown
<!-- factbase:XXXXXX -->
# Document Title

Content with temporal tags and source references...

---
[^1]: Source attribution
[^2]: Source attribution
```

## Temporal Tags

Every fact or detail SHOULD include a temporal tag indicating when the information was valid. Tags use the format `@t[...]` placed immediately after the fact.

### Tag Syntax

| Syntax | Meaning | Use When |
|--------|---------|----------|
| `@t[=YYYY-MM-DD]` | Event — happened at this date | One-time events: founding, graduation, marriage |
| `@t[~YYYY-MM-DD]` | State — true as of this date, may have changed | Changeable facts: location, role, contact info |
| `@t[YYYY..YYYY]` | Date range (inclusive) | Fact was true during period |
| `@t[YYYY..]` | Start date, ongoing | Started and believed current |
| `@t[..YYYY]` | Unknown start, ended | Historical fact with known end |
| `@t[?]` | Unknown / unverified | Temporal context unavailable |

### Date Formats

Dates support multiple granularities:
- Year: `2024`
- Quarter: `2024-Q2`
- Month: `2024-03`
- Day: `2024-03-15`

#### BCE / Negative Years

For historical dates before the Common Era, two notations are supported:

| Notation | Example | Stored As |
|----------|---------|-----------|
| BCE suffix | `@t[=331 BCE]` | `-0331` |
| Negative year | `@t[=-330]` | `-0330` |
| Padded negative | `@t[=-0330]` | `-0330` |

BCE notation is converted to negative years internally. Negative years are zero-padded to 4 digits.

```markdown
- Battle of Gaugamela @t[=331 BCE]
- Greco-Persian Wars @t[490 BCE..479 BCE]
- Augustus reign @t[-31..14]
- Founded circa @t[=-0753]
```

### Examples

```markdown
- CTO at Acme Corp @t[2020..2022]
- Lives in Austin @t[~2024-01]
- Founded the company @t[=2019-06-15]
- Board member @t[2021..]
- Previous advisor role @t[..2020]
- Has PhD in Physics @t[?]
```

### Invalid Tag Content

The `@t[...]` tag MUST contain only dates, date ranges, or `?`. Never put entity names, descriptions, statuses, or statistics inside:

```
❌ @t[Wolfgang Amadeus Mozart]          — entity name, not a date
❌ @t[Complex counterpoint and fugal writing]  — description, not a date
❌ @t[No significant seismic activity]  — status, not a date
❌ @t[Active Production Status: Ongoing] — status, not a date
❌ @t[Total Produced: 650+]            — statistic, not a date
❌ @t[seasonal]                        — vague time word, not a date

✅ @t[=2024]  @t[~2024-03]  @t[2020..2023]  @t[2024..]  @t[?]  @t[=331 BCE]
```

## Source Attribution

Sources are referenced inline using markdown footnote syntax `[^N]` and defined at the document end.

### Inline References

Place source references after the temporal tag:

```markdown
- CTO at Acme Corp @t[2020..2022] [^1]
- VP Engineering at BigCo @t[2022..] [^2]
```

Multiple sources for one fact:

```markdown
- Acquired StartupX for $50M @t[=2023-06] [^1][^2]
```

### Footnote Format

```
[^N]: <source type>, <context or date>
```

### Standard Source Types

| Source Type | Format |
|-------------|--------|
| LinkedIn | `LinkedIn profile (<URL>), scraped YYYY-MM-DD` |
| Website | `Company website (<URL>), accessed YYYY-MM-DD` |
| Press release | `Press release, YYYY-MM-DD` |
| News | `News article, <publication>, YYYY-MM-DD` |
| Filing | `Public filing (<type>), YYYY` |
| Author knowledge | `Author knowledge, see [[id]]` |
| Email | `Email from <person>, subject "<subject>", YYYY-MM-DD` |
| Event | `Conference bio, <event name> YYYY` |
| Slack | `Slack #<channel>, YYYY-MM-DD, <message URL>` |
| Inferred | `Inferred from <description>` |
| Unverified | `Unverified` |

### Source Traceability

Every source definition MUST include enough detail to locate the original data. A platform name alone is never sufficient — include dates, URLs, channel names, subject lines, or other identifiers.

**Traceable** (good): `Slack #project-alpha, 2024-01-10, https://workspace.slack.com/archives/C01234/p1234`
**Untraceable** (bad): `Slack message`

### Author Knowledge

Facts known firsthand by the knowledge base owner belong in dedicated author knowledge documents (placed in an `author-knowledge/` folder). Other documents cite them as a source:

```
[^1]: Author knowledge, see [[a1b2c3]]
```

Author knowledge files are exclusively human-authored. Agents must never create them or use "Author knowledge" as a source for agent-obtained data.

### Complete Example

```markdown
<!-- factbase:a1b2c3 -->
# John Smith

## Career
- CTO at Acme Corp @t[2020..2022] [^1]
- VP Engineering at BigCo @t[2022..] [^2]
- Executive sponsor for Project Alpha @t[2021-Q2..2021-Q4] [^1]

## Personal
- Based in Austin, TX @t[~2024-01] [^3]
- Married @t[=2018] [^4]

## Education
- MBA from Stanford @t[?]
- BS Computer Science, MIT @t[=2008] [^1]

---
[^1]: LinkedIn profile, scraped 2024-01-15
[^2]: BigCo press release, 2022-03-01
[^3]: Speaker bio, AWS re:Invent 2024
[^4]: Public records
```

## Processing Guidelines

### For Document Producers

1. Include `@t[...]` on every fact where temporal context is known
2. Use `@t[?]` explicitly when temporal context is unavailable
3. Prefer specific dates over ranges when known
4. Use `@t[~...]` for changeable facts where you're recording when you last checked (location, role, contact info)
5. Use `@t[=...]` for one-time events (graduation, founding, marriage)
6. Reuse footnote numbers for the same source at the same point in time. Use separate footnotes if the same source was checked on different dates.

### For Document Consumers

1. Facts without `@t[...]` should be treated as `@t[?]`
2. When answering time-sensitive queries, filter by temporal overlap
3. Prefer facts with recent `@t[~...]` dates for current-state questions
4. Facts marked `@t[?]` should be flagged as lower confidence
5. Source footnotes provide provenance for fact verification

## Review Queue

Factbase can analyze documents and generate questions about inconsistencies, missing data, or ambiguities. Questions are appended in a structured Review Queue section.

### Review Queue Format

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

### Question Types

| Tag | Meaning |
|-----|---------|
| `@q[temporal]` | Missing or unclear time information |
| `@q[conflict]` | Contradictory facts detected |
| `@q[missing]` | Missing source or expected data |
| `@q[ambiguous]` | Unclear meaning, needs clarification |
| `@q[stale]` | Data may be outdated (based on `@t[~...]` age) |
| `@q[duplicate]` | Possible duplicate of another entity |

### Answering Questions

1. Check the box `[x]` to mark as answered
2. Add response in the blockquote line below (`> your answer here`)
3. Run `factbase review --apply` to process answers

### Question Lifecycle

```
[Generated]  → [ ] Unanswered, empty blockquote
[Answered]   → [x] Checked, blockquote filled
[Applied]    → Removed from queue, fact updated, reviewed marker added
[Dismissed]  → [x] Checked, blockquote contains "dismiss" or "ignore"
[Deferred]   → [x] Checked, blockquote contains "defer: <note>" → unchecked, note kept
[Pruned]     → Removed by check when trigger condition no longer exists
```

## Validation

A well-formed fact document:
- [ ] Has factbase header comment
- [ ] Has H1 title
- [ ] Has temporal tags on >80% of facts
- [ ] Has source references on sourced facts
- [ ] Has footnote definitions for all references
- [ ] Uses only valid date formats in temporal tags
- [ ] Review Queue questions are properly formatted (if present)

## Inbox Blocks

Inbox blocks stage corrections or updates for LLM-assisted integration:

```markdown
<!-- factbase:inbox -->
Notes, corrections, or new facts to integrate.
<!-- /factbase:inbox -->
```

- Processed by `factbase review --apply` alongside review questions
- LLM integrates content into the document body with appropriate temporal tags and sources
- Block is removed after successful integration
- Multiple inbox blocks per document are supported
- Use `--dry-run` to preview integration
