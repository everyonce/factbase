# Factbase Document Authoring Guide

> **For AI Agents:** See [agent-authoring-guide.md](agent-authoring-guide.md) for a comprehensive guide optimized for automated document creation.

Instructions for creating knowledge base documents optimized for Factbase indexing, semantic search, and link detection.

## Quick Reference

| Aspect | Requirement |
|--------|-------------|
| Format | Markdown (`.md` files) |
| Title | First `# Heading` in document |
| Type | Determined by parent folder name |
| Minimum length | 100+ characters |
| Optimal length | 500-5000 characters |
| Temporal tags | Required on all dynamic facts |
| Sources | Footnote format `[^N]` |

## Document Structure

### File Location Determines Type
The **immediate parent folder** determines the document type:
```
/people/alice-chen.md        → type: "person"
/projects/platform-api.md    → type: "project"
/concepts/api-gateway.md     → type: "concept"
/meetings/2024-01-15.md      → type: "meeting"
```

Nested folders use only the immediate parent:
```
/clients/acme-corp/contacts/john-smith.md → type: "contact" (not "client")
```

**Entity folder convention:** When an entity is large enough for its own directory, name the entity file the same as the folder. The type is derived from the grandparent:
```
/companies/xsolis/xsolis.md       → type: "company" (entity doc)
/companies/xsolis/people/jane.md  → type: "person"  (normal)
/companies/xsolis/overview.md     → type: "xsolis"  (no match, normal)
```

Folder names are automatically singularized: `people/` → "person", `projects/` → "project".

### Title from First H1
The document title is extracted from the first `# Heading`:
```markdown
# Alice Chen

Content here...
```

If no H1 exists, the filename (without extension) becomes the title.

### Factbase ID Header
Factbase automatically injects a tracking ID on first scan:
```markdown
<!-- factbase:a1b2c3 -->
# Document Title
```

**Do not manually create or modify this header.** It's managed by Factbase.

## Content Optimization

### For Semantic Search (Embeddings)

1. **Front-load key information** - Put the most important content in the first few paragraphs. Long documents are chunked, and the beginning carries more weight.

2. **Use clear, descriptive language** - Write naturally but be specific:
   - ❌ "She works on the thing"
   - ✅ "Alice Chen leads the Platform API project"

3. **Include synonyms and related terms** - Help semantic matching:
   ```markdown
   ## Authentication
   The auth system (also called identity management or IAM) handles user login...
   ```

4. **Structured sections improve retrieval**:
   ```markdown
   # Project Alpha
   
   ## Overview
   Brief description of what this project does.
   
   ## Team
   - Alice Chen (Tech Lead)
   - Bob Martinez (Backend)
   
   ## Status
   Currently in development, targeting Q2 release.
   ```

### For Link Detection (Entity Recognition)

The LLM scans documents to find mentions of other entities. To maximize detection:

1. **Use exact entity names** - Match titles of other documents:
   - If you have `people/alice-chen.md` with title "Alice Chen"
   - Reference as "Alice Chen" not "Alice" or "A. Chen"

2. **Provide context around mentions**:
   - ❌ "Alice approved it"
   - ✅ "Alice Chen approved the Platform API design"

3. **Be consistent with naming** - Use the same name format throughout:
   ```markdown
   Alice Chen leads the team. Chen presented the roadmap...  ← inconsistent
   Alice Chen leads the team. Alice Chen presented...        ← consistent
   ```

4. **Manual links for precision** - Use `[[name]]` syntax for explicit links, where `name` is the target document's filename stem:
   ```markdown
   See [[platform-api]] for the full specification.
   ```
   Use the filename stem (lowercase-with-hyphens) rather than the hex document ID. Readable names make cross-references understandable without looking up IDs. The `factbase check` command flags hex-ID cross-refs and suggests the readable alternative.

5. **Link blocks** - Add directional link blocks at the bottom of the document (after footnotes) to declare explicit cross-references using hex document IDs:
   ```markdown
   ---
   [^1]: Source one
   [^2]: Source two

   References: [[abc123]] [[def456]] [[ghi789]]
   Referenced by: [[jkl012]]
   ```
   - `References:` = outbound links FROM this document TO those documents (source_id = this doc)
   - `Referenced by:` = inbound links FROM those documents TO this document (target_id = this doc)
   - The `store_links` MCP tool manages both blocks automatically. You can also add them manually.
   - Legacy `Links:` format is treated as `References:` for backward compatibility.
   - During scan, these IDs are detected as links just like inline `[[id]]` references.

## Recommended Document Templates

### Person
```markdown
# Full Name

**Role:** Job Title at Company @t[2023..]
**Location:** City, State @t[~2024-01]

## Background
Brief professional background and expertise areas.

## Career
- Current Role at Company @t[2023..] [^1]
- Previous Role at OtherCo @t[2020..2023] [^1]

## Projects
- Leading [[project-name]] Project Name @t[2024..]
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
One paragraph describing what this project does and why it exists.

## Status
In development @t[2024-Q1..], targeting Q2 release.

## Team
- Alice Chen - Tech Lead @t[2024..]
- Bob Martinez - Backend @t[2024..]

## Technical Details
Architecture, key technologies, dependencies.

## Related
- Depends on: [[infrastructure-platform]] Infrastructure Platform
- Used by: [[mobile-app]] Mobile App
```

### Concept
```markdown
# Concept Name

## Definition
Clear, concise explanation of what this concept is.

## Context
When and why this concept is relevant.

## Examples
Concrete examples demonstrating the concept.

## Related Concepts
- [[abc123]] Related Concept A - how it relates
- [[def456]] Related Concept B - how it relates

## References
Links to external documentation or resources.
```

### Meeting Notes
```markdown
# Meeting: Topic - 2024-01-15

## Attendees
- Alice Chen
- Bob Martinez

## Summary
Brief overview of what was discussed.

## Decisions
- Decision 1: Description @t[=2024-01-15]
- Decision 2: Description @t[=2024-01-15]

## Action Items
- [ ] Alice Chen: Task description (due 2024-01-22)
- [ ] Bob Martinez: Task description (due 2024-01-22)

## Follow-up
Next meeting scheduled for 2024-01-22.
```

## File Naming Conventions

Use lowercase with hyphens:
```
alice-chen.md           ✅
Alice Chen.md           ❌ (spaces)
alice_chen.md           ❌ (underscores work but hyphens preferred)
AliceChen.md            ❌ (camelCase)
```

For dated documents:
```
2024-01-15-standup.md   ✅ (ISO date prefix)
standup-2024-01-15.md   ✅ (date suffix also fine)
```

## Folder Structure Best Practices

Keep it flat when possible:
```
/people/
/projects/
/concepts/
/meetings/
/definitions/          ← glossaries, acronyms
/author-knowledge/     ← human-only facts
```

Use nesting for large entities:
```
/companies/
  /acme-corp/
    /acme-corp.md      ← entity doc (type: "company", see entity folder convention)
    /people/           ← type: "person"
    /archive/          ← skipped by check
```

### Archiving Documents

Move stable documents into an `archive/` subfolder. They stay indexed and searchable but are skipped by `factbase check` — no review questions, no deep-check cycles.

```
/people/archive/former-employee.md    ← searchable, not checked
/projects/archive/completed-2024.md   ← searchable, not checked
```

### Reference Entities

Add `<!-- factbase:reference -->` to documents that exist primarily as link targets — external entities you reference but don't track in depth. Reference docs are indexed, searchable, and participate in link detection, but are skipped by quality checks (check, enrich, resolve workflows).

```markdown
<!-- factbase:reference -->
# AWS Lambda

- Serverless compute service by Amazon Web Services @t[2014..] [^1]

---
[^1]: AWS, aws.amazon.com/lambda
```

Place the marker before the title heading. Use for well-known products, standards, or organizations that your knowledge base references but doesn't own.

### Allowed Types
If your `perspective.yaml` defines `allowed_types`, ensure your folder names match:
```yaml
# perspective.yaml
allowed_types:
  - person
  - project
  - concept
  - meeting
```

Factbase will warn about documents in folders that don't match allowed types.

## Content Length Guidelines

- **Minimum**: 100+ characters of meaningful content (very short docs may be flagged by `factbase check`)
- **Optimal**: 500-5000 characters for best embedding quality
- **Maximum**: No hard limit, but documents over 100K characters will be chunked for embedding

## Temporal Tags (Required)

> **Full specification:** See [fact-document-format.md](fact-document-format.md) for complete temporal tag syntax.

Facts without dates become unreliable quickly. Use `@t[...]` tags on all dynamic facts:

### Tag Syntax Quick Reference

| Syntax | Meaning | Example |
|--------|---------|---------|
| `@t[=2024-03]` | Point in time | `Founded company @t[=2019-06]` |
| `@t[~2024-03]` | Last verified | `Lives in Austin @t[~2024-01]` |
| `@t[2020..2022]` | Date range | `CTO at Acme @t[2020..2022]` |
| `@t[2021..]` | Started, ongoing | `Board member @t[2021..]` |
| `@t[..2020]` | Historical, ended | `Advisor role @t[..2020]` |
| `@t[?]` | Unknown | `Has PhD @t[?]` |

### BCE / Historical Dates

For dates before the Common Era, use either BCE suffix or negative years:

```markdown
- Battle of Gaugamela @t[=331 BCE]
- Greco-Persian Wars @t[490 BCE..479 BCE]
- Augustus reign @t[-31..14]
```

Both `331 BCE` and `-331` are accepted; they are stored as `-0331` internally.

### What NOT to Put in @t Tags

Only dates go inside `@t[...]`. Never put names, descriptions, statuses, or statistics:

```
❌ @t[Wolfgang Amadeus Mozart]     → put the name in fact text, use @t[1756..1791]
❌ @t[Active Production Status]    → use @t[2020..] for "ongoing since 2020"
❌ @t[Total Produced: 650+]        → use @t[~2024] for "as of 2024"
❌ @t[seasonal]                    → put in fact text, tag with @t[~2024]
✅ @t[=2024]  @t[2020..2023]  @t[?]
```

### Employment & Roles
```markdown
## Role
- CTO at XSOLIS @t[2021..]
- VP Engineering at Acme @t[2018..2021]
```
Not: "CTO at XSOLIS" (when? still true?)

### Contact Information
```markdown
## Contact
- Email: name@example.com @t[~2024-01]
- Office: 123 Main St @t[~2023-06]
```

### Relationships
```markdown
## Team
- Reports to: Jane Smith @t[2023..]
- Direct reports: Bob, Alice @t[~2024-01]
```

### Static vs. Dynamic Facts
- **Static** (no date needed): Historical events, past projects, awards
- **Dynamic** (always date): Job title, employer, contact info, team members, office location

Education degrees are static, but graduation years should still use `@t[=YYYY]`.

## What to Avoid

1. **Duplicate content** - Don't copy the same text into multiple documents. Link instead.

2. **Ambiguous references** - "the project" or "that meeting" won't link properly.

3. **Excessive formatting** - Heavy use of tables, code blocks, or images reduces semantic searchability.

4. **Orphan documents** - Documents with no links to/from other docs are harder to discover. Add context.

5. **Stale content** - Update documents when information changes. Use `factbase check --stale-days 365` to find old docs.

6. **Undated dynamic facts** - Employment, contact info, and relationships without dates are unreliable. Always include "as of [date]" for facts that change over time.

7. **Untraceable sources** - "Slack message" or "Outlook" alone can't be verified. Include channel, date, URL, or subject line so the source can be relocated.

## Special Document Types

### Author Knowledge (`author-knowledge/` folder)
For facts the knowledge base owner knows firsthand — things not available from any external source. Only humans should create these files. Other documents cite them as: `[^1]: Author knowledge, see [[id]]`.

### Definitions (`definitions/` folder)
Glossaries for acronyms, jargon, and domain-specific terms. Organize by domain: `definitions/business-terms.md`, `definitions/technical-terms.md`. When check flags an undefined acronym, add it here.

## Verification

After creating documents:

```bash
# Scan to index
factbase scan

# Verify document was indexed
factbase search "your document title"

# Check links were detected
factbase status --detailed

# Find quality issues
factbase check
```

## Inbox Blocks

Use inbox blocks to stage corrections or updates for LLM-assisted integration:

```markdown
<!-- factbase:inbox -->
- CEO changed to Jane Doe in January 2026
- Revenue updated to $50M (source: Q4 earnings report)
<!-- /factbase:inbox -->
```

Run `factbase review --apply` to have the LLM integrate inbox content into the document, adding temporal tags and source footnotes as appropriate. The inbox block is removed after successful integration.

Use `factbase review --apply --dry-run` to preview what will be integrated.
