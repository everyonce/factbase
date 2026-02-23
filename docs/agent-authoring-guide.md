# Factbase Document Authoring Guide for AI Agents

This guide explains how to create markdown documents optimized for ingestion into Factbase—a semantic search and knowledge management system. Follow these conventions to ensure your documents are properly indexed, searchable, and interconnected.

---

## Quick Reference

| Aspect | Requirement |
|--------|-------------|
| Format | Markdown (`.md` files) |
| Title | First `# Heading` in document |
| Type | Determined by parent folder name |
| Minimum length | 100+ characters |
| Optimal length | 500-5000 characters |
| Temporal tags | Required on dynamic facts |
| Sources | Footnote format `[^N]` |

---

## Document Structure

### Basic Template

```markdown
# Document Title

Brief overview or summary paragraph.

## Section 1
Content with facts, each annotated with temporal context.

## Section 2
More content...

---
[^1]: Source attribution
[^2]: Source attribution
```

### How Factbase Processes Your Document

1. **ID Injection**: On first scan, Factbase adds a tracking header:
   ```markdown
   <!-- factbase:a1b2c3 -->
   # Document Title
   ```
   Never create or modify this header manually.

2. **Title Extraction**: Pulled from the first `# Heading`

3. **Type Derivation**: Based on immediate parent folder:
   ```
   /people/alice-chen.md     → type: "person"
   /projects/platform-api.md → type: "project"
   /concepts/caching.md      → type: "concept"
   ```

4. **Embedding Generation**: Content is vectorized for semantic search

5. **Link Detection**: LLM scans for mentions of other entities

---

## Temporal Tags (Critical)

**Every dynamic fact MUST include a temporal tag.** Facts without dates become unreliable and unsearchable by time.

### Tag Syntax

| Syntax | Meaning | Example |
|--------|---------|---------|
| `@t[=2024-03]` | Point in time / as of | `Founded company @t[=2019-06]` |
| `@t[~2024-03]` | Last verified / last known | `Lives in Austin @t[~2024-01]` |
| `@t[2020..2022]` | Date range | `CTO at Acme @t[2020..2022]` |
| `@t[2021..]` | Started, ongoing | `Board member @t[2021..]` |
| `@t[..2020]` | Historical, ended | `Advisor role @t[..2020]` |
| `@t[?]` | Unknown / unverified | `Has PhD @t[?]` |

### Date Granularity

- Year: `2024`
- Quarter: `2024-Q2`
- Month: `2024-03`
- Day: `2024-03-15`

### Static vs Dynamic Facts

| Static (no date needed) | Dynamic (always date) |
|------------------------|----------------------|
| Education degrees | Current job title |
| Historical events | Current employer |
| Past projects completed | Contact information |
| Awards received | Team members |
| Publications | Office location |

### Examples

```markdown
## Career
- CTO at Acme Corp @t[2020..2022] [^1]
- VP Engineering at BigCo @t[2022..] [^2]

## Personal
- Based in Austin, TX @t[~2024-01]
- Married @t[=2018]

## Education
- MBA from Stanford @t[?]
- BS Computer Science, MIT @t[=2008]
```

---

## Source Attribution

Use markdown footnotes to cite sources. This enables fact verification and confidence assessment.

### Format

```markdown
- Fact statement @t[date] [^1]

---
[^1]: Source type, context or date
```

### Standard Source Types

| Type | Format Example |
|------|----------------|
| LinkedIn | `LinkedIn profile, scraped 2024-01-15` |
| Website | `Company website, accessed 2024-01-15` |
| Press release | `Press release, 2024-01-15` |
| News | `News article, TechCrunch, 2024-01-15` |
| Filing | `SEC filing (10-K), 2024` |
| Direct | `Direct conversation, 2024-01-15` |
| Email | `Email from John Smith, 2024-01-15` |
| Event | `Conference bio, AWS re:Invent 2024` |
| Inferred | `Inferred from org chart` |
| Unverified | `Unverified` |

### Multiple Sources

```markdown
- Acquired StartupX for $50M @t[=2023-06] [^1][^2]
```

---

## Optimizing for Search

### Semantic Search (Embeddings)

1. **Front-load key information** - First paragraphs carry more weight
2. **Use clear, specific language**:
   - ❌ "She works on the thing"
   - ✅ "Alice Chen leads the Platform API project"
3. **Include synonyms**: "auth system (also called identity management or IAM)"
4. **Use structured sections** with clear headings

### Link Detection (Entity Recognition)

1. **Use exact entity names** matching other document titles:
   - If `people/alice-chen.md` has title "Alice Chen"
   - Reference as "Alice Chen" not "Alice" or "A. Chen"

2. **Provide context around mentions**:
   - ❌ "Alice approved it"
   - ✅ "Alice Chen approved the Platform API design"

3. **Manual links for precision**:
   ```markdown
   See [[a1b2c3]] for the full specification.
   ```

---

## Document Templates

### Person

```markdown
# Full Name

**Role:** Job Title at Company @t[2023..]
**Location:** City, State @t[~2024-01]

## Background
Brief professional background and expertise areas.

## Current Responsibilities
- Primary responsibility @t[2023..]
- Secondary responsibility @t[2023..]

## Projects
- Leading Project Alpha @t[2024..]
- Previously led Project Beta @t[2022..2023]

## Contact
- Email: name@example.com @t[~2024-01]
- Slack: @handle

---
[^1]: LinkedIn profile, scraped 2024-01-15
```

### Project

```markdown
# Project Name

## Overview
One paragraph describing purpose and goals.

## Status
Current phase @t[2024-Q1..], targeting completion @t[=2024-Q3]

## Team
- Alice Chen - Tech Lead @t[2024..]
- Bob Martinez - Backend @t[2024..]

## Technical Details
Architecture, key technologies, dependencies.

## Related
- Depends on: [[abc123]] Infrastructure Platform
- Used by: [[def456]] Mobile App

---
[^1]: Project charter, 2024-01-10
```

### Company/Organization

```markdown
# Company Name

## Overview
What the company does, industry, size.

## Key Facts
- Founded @t[=2015]
- Headquarters: City, State @t[~2024-01]
- Employees: ~500 @t[~2024-01]
- Funding: Series C, $50M @t[=2023-06] [^1]

## Leadership
- CEO: John Smith @t[2020..]
- CTO: Jane Doe @t[2022..]

## Products/Services
- Product A - description
- Product B - description

---
[^1]: Crunchbase, accessed 2024-01-15
```

### Meeting Notes

```markdown
# Meeting: Topic - 2024-01-15

## Attendees
- Alice Chen
- Bob Martinez

## Summary
Brief overview of what was discussed and decided.

## Key Decisions
- Decision 1: Description @t[=2024-01-15]
- Decision 2: Description @t[=2024-01-15]

## Action Items
- [ ] Alice Chen: Task description (due 2024-01-22)
- [ ] Bob Martinez: Task description (due 2024-01-22)

## Follow-up
Next meeting scheduled for 2024-01-22.
```

---

## File Organization

### Naming Conventions

```
alice-chen.md           ✅ lowercase with hyphens
Alice Chen.md           ❌ spaces
alice_chen.md           ⚠️ works but hyphens preferred
2024-01-15-standup.md   ✅ ISO date prefix for dated docs
```

### Folder Structure

```
knowledge-base/
├── people/           → type: "person"
├── companies/        → type: "company"
├── projects/         → type: "project"
├── concepts/         → type: "concept"
├── meetings/         → type: "meeting"
└── notes/            → type: "note"
```

Folder names are automatically singularized: `people/` → "person"

---

## Common Mistakes to Avoid

| Mistake | Problem | Fix |
|---------|---------|-----|
| No temporal tags | Facts become unreliable | Add `@t[...]` to all dynamic facts |
| Vague references | "the project" won't link | Use exact names: "Platform API project" |
| Undated employment | "Works at Acme" - when? | "Works at Acme @t[2023..]" |
| Duplicate content | Same text in multiple docs | Link instead: "See [[abc123]]" |
| Missing sources | Can't verify facts | Add footnotes with source type and date |
| Orphan documents | No links to/from others | Add context mentioning related entities |

---

## Validation Checklist

Before submitting documents:

- [ ] Has clear `# Title` as first heading
- [ ] Placed in appropriate type folder
- [ ] All dynamic facts have `@t[...]` tags
- [ ] Sources cited with `[^N]` footnotes
- [ ] Entity names match existing document titles
- [ ] Minimum 100 characters of content
- [ ] No duplicate content from other documents

---

## Complete Example

```markdown
# Sarah Chen

**Role:** VP of Engineering at TechCorp @t[2023..]
**Location:** San Francisco, CA @t[~2024-01]

## Background
Sarah Chen is a technology executive with 15 years of experience in 
distributed systems and platform engineering. Previously led infrastructure 
teams at Google and Stripe.

## Career History
- VP Engineering at TechCorp @t[2023..] [^1]
- Senior Director at Stripe @t[2020..2023] [^1]
- Staff Engineer at Google @t[2015..2020] [^1]

## Current Focus
- Leading Platform Modernization Initiative @t[2024..]
- Executive sponsor for [[abc123]] API Gateway Project @t[2023..]
- Hiring for 3 senior engineering roles @t[~2024-01]

## Education
- MS Computer Science, Stanford @t[=2010] [^1]
- BS Computer Science, MIT @t[=2008] [^1]

## Contact
- Email: schen@techcorp.com @t[~2024-01]
- LinkedIn: linkedin.com/in/sarahchen @t[~2024-01]

## Notes
- Frequent speaker at distributed systems conferences
- Published paper on consensus algorithms @t[=2019] [^2]
- Advisor to two early-stage startups @t[2022..]

---
[^1]: LinkedIn profile, scraped 2024-01-15
[^2]: ACM Digital Library, accessed 2024-01-15
```

---

## Processing After Creation

After creating documents, Factbase will:

1. **Scan**: `factbase scan` indexes new/changed files
2. **Embed**: Generate semantic vectors for search
3. **Link**: Detect entity mentions across all documents
4. **Validate**: `factbase lint` checks for quality issues

The document becomes searchable immediately after scanning and will be linked to other documents that mention its title or ID.

## Inbox Blocks

Stage corrections or new information for LLM-assisted integration:

```markdown
<!-- factbase:inbox -->
- CEO changed to Jane Doe in January 2026
- Revenue updated to $50M (source: Q4 earnings report)
<!-- /factbase:inbox -->
```

When `factbase review --apply` runs, the LLM integrates inbox content into the document body (adding temporal tags, sources, etc.) and removes the inbox block. Use `--dry-run` to preview.
