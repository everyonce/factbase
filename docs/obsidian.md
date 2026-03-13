# Using Factbase with Obsidian

Factbase and Obsidian work well together. Factbase handles indexing, semantic search, and quality checks; Obsidian handles reading, writing, and graph exploration. Neither tool gets in the other's way.

## Opening a Factbase KB as an Obsidian Vault

Open the repository root as an Obsidian vault — the same directory that contains `perspective.yaml`. Obsidian treats the whole directory tree as its vault, so all your markdown files are immediately visible.

No special Obsidian plugins are required. The integration is based on standard markdown features: wikilinks, YAML frontmatter, and tags.

## Enabling the Obsidian Preset

Add this to `perspective.yaml` in your repository root:

```yaml
format:
  preset: obsidian
```

Then run a scan:

```bash
factbase scan
```

The scan writes three files into `.obsidian/`:

- `.obsidian/snippets/factbase.css` — CSS snippet (see below)
- `.obsidian/app.json` — pre-enables the CSS snippet
- `.gitignore` — updated to track those two files while ignoring the rest of `.obsidian/`

These files are safe to commit. Anyone who clones the repo and opens it in Obsidian gets the CSS applied automatically.

## What the Obsidian Preset Changes

The preset switches several output settings at once:

| Setting | Default | Obsidian preset |
|---------|---------|-----------------|
| Link style | `[[hex_id]]` | `[[folder/filename\|Title]]` |
| Frontmatter | none | YAML frontmatter with `factbase_id`, `type`, `tags` |
| ID placement | HTML comment | `factbase_id:` in frontmatter |
| Review queue | plain section | collapsed `> [!review]-` callout |
| Reviewed dates | inline HTML comments | `reviewed:` frontmatter field |

You can override individual settings while keeping the preset as a base:

```yaml
format:
  preset: obsidian
  inline_links: false   # don't embed wikilinks in body text
```

## The CSS Snippet

The snippet at `.obsidian/snippets/factbase.css` does four things:

1. **Review callout color** — styles `> [!review]` callouts with amber color and a clipboard-check icon, so the review queue is visually distinct from other callouts.

2. **Temporal tag pills** — adds slight padding and border-radius to inline code, which makes `@t[2024..]` tags look like pills rather than raw code.

3. **Hides the inline title** — Obsidian shows the filename as a title above the `# Heading`. Since factbase uses the H1 as the entity name, the duplicate filename title is hidden.

4. **Hides the properties panel** — the `factbase_id` and `type` fields in frontmatter are internal tracking fields. The snippet hides the properties panel in reading view so they don't clutter the display.

To customize the CSS, create `.factbase/obsidian.css` in your repository root. Factbase uses that file instead of the built-in default on the next scan.

## Graph View

When the Obsidian preset is active, factbase writes links as `[[folder/filename|Title]]` wikilinks. Obsidian's graph view renders these as edges between nodes.

Path-based tags are also written to frontmatter. For a file at `customers/acme/people/alice.md`, the frontmatter will include:

```yaml
tags: [acme, people]
```

The top-level folder is omitted (it's too broad to be useful as a tag). Tags from deeper paths are included. In Obsidian's graph view, you can filter by tag to see subsets of your knowledge base — for example, all documents tagged `acme`.

## Scan After Rename

If you rename or move a file in Obsidian, Obsidian updates its own wikilinks automatically. The factbase database still holds the old path until you run a scan.

The document's ID is stable across renames — it lives in the frontmatter (`factbase_id: abc123`), not in the filename. No data is lost. Run a scan to resync:

```bash
factbase scan
```

Or tell your agent: `factbase(op='scan')`.

## Dataview Queries

If you use the [Dataview](https://github.com/blacksmithgu/obsidian-dataview) plugin, the frontmatter factbase writes is queryable.

**List all documents of a given type:**

```dataview
LIST
FROM ""
WHERE type = "person"
SORT file.name ASC
```

**List documents with a specific tag:**

```dataview
LIST
FROM #acme
WHERE type = "person"
SORT file.name ASC
```

**Table of documents reviewed recently:**

```dataview
TABLE reviewed, type
FROM ""
WHERE reviewed != null
SORT reviewed DESC
LIMIT 20
```

**Find documents that haven't been reviewed:**

```dataview
LIST
FROM ""
WHERE reviewed = null AND type != null
SORT file.mtime ASC
```

The `type` field comes from the document's parent folder (singularized). The `tags` field comes from the path. The `reviewed` field is written by factbase when you answer review questions.

## Editing in Obsidian

Any edits you make in Obsidian are picked up on the next scan. You don't need to do anything special — just edit and save as usual.

A few things to be aware of:

- **Don't edit the `factbase_id` field** in frontmatter. It's the document's stable identity. Changing it will cause factbase to treat the document as new on the next scan.
- **The review queue** appears as a collapsed callout at the bottom of documents that have pending questions. You can expand it to read the questions. Answer them via your agent or the web UI — don't edit the callout directly, as the format is parsed by factbase.
- **Temporal tags** (`@t[2024..]`) are plain inline code in Obsidian. They render as styled pills with the CSS snippet applied.

## Git Setup

The `.gitignore` entries added by factbase allow the CSS snippet and app.json to be tracked while keeping the rest of `.obsidian/` out of version control:

```
.obsidian/
!.obsidian/snippets/
!.obsidian/snippets/factbase.css
!.obsidian/app.json
```

Commit these files so that anyone who clones the repo gets the CSS pre-enabled when they open the vault.
