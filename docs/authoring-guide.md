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
/companies/acme-corp/acme-corp.md       → type: "company" (entity doc)
/companies/acme-corp/people/jane.md  → type: "person"  (normal)
/companies/acme-corp/overview.md     → type: "acme-corp"  (no match, normal)
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

## ⚠️ Facts Must Be Bullet-Point List Items

**Factbase only indexes lines starting with `- ` as facts.** Prose paragraphs are ignored — they are embedded for semantic search but produce 0 indexed facts, no temporal coverage, and no review questions.

✅ Correct:
```markdown
- The Lambda function supports up to 10GB memory @t[2023-11] [^1]
- Deployed to us-east-1 and eu-west-1 @t[2024-Q1]
```

❌ Wrong (will be silently ignored as facts):
```
Lambda functions support up to 10GB memory (as of November 2023).
Deployed to us-east-1 and eu-west-1 as of Q1 2024.
```

Every section that contains factual claims should use bullet-point list items. Prose is fine for introductory summaries, but the facts themselves must be list items.

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

Factbase uses string matching to find mentions of other entities during scan. To maximize detection:

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
   Use the filename stem (lowercase-with-hyphens) rather than the hex document ID. Readable names make cross-references understandable without looking up IDs. Quality checks flag hex-ID cross-refs and suggest the readable alternative.

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

### Entity Folders

A subfolder named after an entity (e.g., `characters/diluc/`) is an **entity folder**. All files inside it are companion files for that entity. Companion files (`lore.md`, `teams.md`, `notes.md`) intentionally carry the entity name as their type — this is correct and will not generate placement warnings.

```
/characters/
  /diluc/
    /diluc.md      ← entity doc (type: "character")
    /lore.md       ← companion file (type: "diluc", by design)
    /teams.md      ← companion file (type: "diluc", by design)
```

The entity doc (`diluc.md`) is the primary document. Companion files are sub-documents that extend it with additional detail.

### Archiving Documents

Move stable documents into an `archive/` subfolder. They stay indexed and searchable but are skipped by quality checks — no review questions, no deep-check cycles.

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

### Suppressing Question Types

For canonical-source knowledge bases (scripture, legal codes, RFCs, standards) where certain review question types are structurally inapplicable, you can suppress them entirely in `perspective.yaml`:

```yaml
review:
  suppress_question_types: [temporal]   # no @t[] questions for canon-text KBs
```

**When to use:** When a question type will never have a meaningful answer for any document in the KB. For example, a Bible KB has tens of thousands of facts — temporal questions ask "when did this happen?" for scripture verses, which have no useful answer. Rather than running a massive resolve pass to mark each one `not-applicable`, declare the suppression at the perspective level.

**Valid types:** `temporal`, `missing`, `ambiguous`, `precision`, `stale`, `conflict`, `duplicate`, `corruption`, `weak-source`

**Warning:** Suppression is permanent for the KB and skips generation entirely — no questions of that type will appear on any scan or check. If you want selective per-fact suppression instead, use `<!-- reviewed:t:DATE -->` markers on individual fact lines.

**Example — Bible KB:**
```yaml
review:
  suppress_question_types: [temporal, stale]
```
This prevents temporal and stale questions from being generated for any document in the repository.

## Content Length Guidelines

- **Minimum**: 100+ characters of meaningful content (very short docs may be flagged by quality checks)
- **Optimal**: 500-5000 characters for best embedding quality
- **Maximum**: No hard limit, but documents over 100K characters will be chunked for embedding

## ⚠️ Write Complete Facts at Write Time

**Rule: Complete every fact at write time.**

When you write a fact line, you already know the source it came from, the temporal context (when it was true), and the precision of the claim. Do not defer this to a resolve pass. Complete each fact line — citation, temporal tag, and precise language — as you write it. A fact written incomplete will generate review questions that cost time and effort to resolve later, doing work you already had the context to do.

This applies especially to:
- **Canonical source imports** (scripture, RFCs, legal codes, CSV/TSV data files): the citation is the row/verse/section ID, known at write time
- **Date-stamped sources** (news, papers, emails): the `@t[]` tag comes from the source date
- **Structured data files**: every field has a row or record reference

**The general rule:** if you know the citation or temporal data at write time, write it. Never leave a fact line incomplete when the source is in front of you.

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

### Narrative Labels Are Never Valid

The most common mistake is writing narrative labels as temporal tags. These are **always wrong** and will generate `@q[corruption]` review questions even when `suppress_question_types: [temporal]` is set:

```
❌ @t[infancy]                → narrative label, not a date
❌ @t[early ministry]         → narrative label, not a date
❌ @t[burning bush]           → narrative label, not a date
❌ @t[eternal pre-existence]  → theological concept, not a date
❌ @t[early adulthood]        → narrative label, not a date
❌ @t[patriarchal era]        → era name, not a date
❌ @t[origin]                 → descriptive noun, not a date
❌ @t[territory]              → descriptive noun, not a date
```

**Decision rule — ask yourself:**
- Can you express this as a year or year range? → Use `@t[YYYY]` or `@t[YYYY..YYYY]`
- Is the date genuinely unknown? → Use `@t[?]`
- Is the fact permanently undatable (eternal, mythological, definitional)? → **Omit the `@t[]` tag entirely** — do NOT write a narrative label
- Is the fact a permanent attribute that will never change? → Use `<!-- reviewed:t:YYYY-MM-DD -->` instead

**Undatable facts — omit the tag entirely:**
- Eternal/theological attributes: "Jesus is the Son of God" — no `@t[]` tag
- Definitional properties: "The Sabbath is the seventh day" — no `@t[]` tag
- A missing `@t[]` tag is better than a malformed one

### Employment & Roles
```markdown
## Role
- CTO at Acme Corp @t[2021..]
- VP Engineering at Acme @t[2018..2021]
```
Not: "CTO at Acme Corp" (when? still true?)

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

5. **Stale content** - Update documents when information changes. Use quality checks via MCP to find old docs.

6. **Undated dynamic facts** - Employment, contact info, and relationships without dates are unreliable. Always include "as of [date]" for facts that change over time.

7. **Untraceable sources** - "Slack message" or "Outlook" alone can't be verified. Include channel, date, URL, or subject line so the source can be relocated.

## Special Document Types

### Author Knowledge (`author-knowledge/` folder)
For facts the knowledge base owner knows firsthand — things not available from any external source. Only humans should create these files. Other documents cite them as: `[^1]: Author knowledge, see [[id]]`.

### Definitions (`definitions/` folder)
Glossaries for acronyms, jargon, and domain-specific terms. Organize by domain: `definitions/business-terms.md`, `definitions/technical-terms.md`. When check flags an undefined acronym, add it here.

## Obsidian Interop

If you use Obsidian as your editor, enable the Obsidian preset in `perspective.yaml`:

```yaml
format:
  preset: obsidian
```

This changes how factbase writes documents:

- **Links** become `[[folder/filename|Title]]` wikilinks instead of `[[hex_id]]` references. Obsidian renders these as graph edges.
- **Frontmatter** is added with `factbase_id`, `type`, and `tags` fields. The `type` field is the document type derived from the parent folder. The `tags` field is derived from the folder path.
- **Review queue** is wrapped in a `> [!review]-` callout block, which Obsidian renders as a collapsed callout.
- **Reviewed dates** are stored in the `reviewed:` frontmatter field instead of inline HTML comments.

After enabling the preset, run `factbase scan`. Factbase writes `.obsidian/snippets/factbase.css` and `.obsidian/app.json` to the repository root. The CSS snippet styles the review callout and hides internal frontmatter fields from Obsidian's properties panel.

**Editing in Obsidian:** Edit files normally. Changes are picked up on the next scan. Don't modify the `factbase_id` frontmatter field — it's the document's stable identity.

**Renaming in Obsidian:** Obsidian updates its own wikilinks when you rename a file. Run `factbase scan` afterward to sync the database with the new path. The document ID is stable, so no data is lost.

**Dataview:** The `type`, `tags`, and `reviewed` frontmatter fields are queryable with the Dataview plugin. See [docs/obsidian.md](obsidian.md) for example queries.

## Verification

After creating documents:

```bash
# Scan to index
factbase scan

# Verify document was indexed
factbase status

# Find quality issues (via MCP agent)
# Tell your agent: "check factbase for quality issues"
```

## Inbox Blocks

Use inbox blocks to stage corrections or updates for agent-assisted integration:

```markdown
<!-- factbase:inbox -->
- CEO changed to Jane Doe in January 2026
- Revenue updated to $50M (source: Q4 earnings report)
<!-- /factbase:inbox -->
```

The agent integrates inbox content into the document body via the `update_document` MCP operation, adding temporal tags and source footnotes as appropriate. The inbox block is removed after successful integration.

## Git Setup

When you create a factbase KB, a `.gitignore` is written automatically during setup with factbase-specific entries.

### What to commit
- All `.md` documents — these are your source of truth
- `perspective.yaml` — KB configuration
- `.gitignore` — the ignore rules themselves
- `.factbase/config.yaml` if present — user configuration

### What NOT to commit
- `.factbase/factbase.db`, `.factbase/factbase.db-shm`, `.factbase/factbase.db-wal` — the SQLite database is a regenerable index, not source of truth. Never commit it. If you lose it, run `factbase scan` to rebuild from your markdown files.
- `.fastembed_cache/` — downloaded embedding model files (~100MB). Never commit. They are re-downloaded automatically.

### If you accidentally committed the database
Run: `git rm --cached .factbase/factbase.db .factbase/factbase.db-shm .factbase/factbase.db-wal` then commit. The database will be recreated on next scan.
