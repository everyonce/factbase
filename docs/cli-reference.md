# CLI Reference

Complete command reference for factbase. See also the [README](../README.md) for a quick overview.

## `factbase init <path> [--name <name>] [--id <id>] [-j]`

Initialize a new repository at the given path.

- `--name` - Repository display name (default: directory name)
- `--id` - Repository ID (default: "default")
- `-j, --json` - Output as JSON

```bash
factbase init .
# Initialized factbase at /path/to/dir
# Next: Add markdown files and run `factbase scan`

factbase init . --json
# {"config_path": "/path/to/dir/.factbase", "created": true, "message": "..."}
```

## `factbase repo add <id> <path> [--name <name>]`

Register a directory as a knowledge base.

## `factbase repo remove <id> [--force]`

Unregister a repository and delete its indexed documents.

## `factbase repo list [-j] [-q]`

List all registered repositories with document counts.

- `-j, --json` - Output as JSON
- `-q, --quiet` - Output only repo IDs, one per line (useful for scripting)

## `factbase scan [--repo <repo>] [-v] [-q] [-j] [--dry-run] [-w] [--check-duplicates] [--stats] [--since <date>]`

Index documents. Generates document embeddings for semantic search, fact-level embeddings for cross-document validation, and detects entity links. Scans specific repo or all repos if omitted.

- `-v, --verbose` - Show per-file processing details
- `-q, --quiet` - Suppress output except errors (useful for scripts)
- `-j, --json` - Output as JSON (includes stats)
- `--dry-run` - Preview changes without modifying database or calling inference backend
- `-w, --watch` - Watch for file changes and rescan automatically
- `--check-duplicates` - Check for duplicate or near-duplicate documents (similarity > 95%)
- `--stats` - Show timing statistics for each scan phase (file discovery, embedding, link detection)
- `--since` - Only process files modified since date (ISO 8601 or relative: 1h, 1d, 1w)
- `--stats-only` - Show quick statistics without modifying database or calling inference backend
- `--check` - Validate index integrity for CI (check embeddings exist and dimensions match)
- `--verify` - Verify document integrity (check file exists, hash matches, ID header present)
- `--fix` - Auto-fix integrity issues found by --verify (re-inject headers, update database)
- `--prune` - Remove orphaned database entries for deleted files (soft delete by default)
- `--hard` - Permanently remove orphaned entries instead of soft delete (use with --prune)
- `--reindex` - Force re-generation of all embeddings (document and fact-level) even if content unchanged (useful after model upgrade)
- `--batch-size <n>` - Batch size for embedding generation (default: 10, from config)
- `--no-links` - Skip link detection phase for faster indexing (links can be detected in subsequent scan)
- `-y, --yes` - Skip confirmation prompts when using --fix or --prune

## `factbase search <query> [-t <type>] [-T <type>] [-r <repo>] [-l <n>] [-j] [--title] [--offline] [--count]`

Semantic search across documents.

- `-t, --doc-type` - Filter by document type
- `-T, --exclude-type` - Exclude documents of this type (can be repeated)
- `-r, --repo` - Filter by repository
- `-l, --limit` - Max results (default: 10)
- `-j, --json` - Output as JSON
- `-c, --compact` - Compact output: one line per result (`[score] id: title`)
- `--title` - Search by title instead of semantic search (faster, no inference needed)
- `--offline` - Offline mode: use title search without inference backend (implies --title)
- `--as-of <date>` - Filter to facts valid at specific date (YYYY, YYYY-MM, or YYYY-MM-DD)
- `--during <range>` - Filter to facts valid during date range (YYYY..YYYY or YYYY-MM..YYYY-MM)
- `--exclude-unknown` - Exclude facts with unknown temporal context
- `--boost-recent` - Boost ranking of facts with recent `@t[~...]` dates
- `-f, --filter <expr>` - Filter results by metadata (can be repeated)
- `-x, --exclude <expr>` - Exclude results matching metadata (same syntax as --filter)
- `--sort <order>` - Sort results: `relevance` (default), `date` (newest first), `title` (alphabetical), `type` (grouped by type)
- `-w, --watch` - Watch for file changes and re-run search (live updating results)
- `--count` - Output only the count of matching results

### Compact Output

Use `--compact` for one-line results, useful for scripting:

```bash
factbase search -c "project deadlines"
# [95%] a1b2c3: Project Alpha Deadlines
# [87%] d4e5f6: Q4 Project Timeline
# [82%] g7h8i9: Sprint Planning Notes

# Pipe to other tools
factbase search -c "API" | head -5
factbase search -c "meeting" | grep "2024"
```

### Post-Search Filtering

Filter search results by document metadata using `--filter`:

```bash
# Filter by document type
factbase search "query" --filter "type:person"

# Filter to documents with temporal tags
factbase search "query" --filter "has:temporal"

# Filter to documents with source citations
factbase search "query" --filter "has:sources"

# Filter by link count
factbase search "query" --filter "links:>5"    # More than 5 outgoing links
factbase search "query" --filter "links:<3"    # Fewer than 3 outgoing links
factbase search "query" --filter "links:0"     # No outgoing links (orphans)

# Combine multiple filters (all must match)
factbase search "query" --filter "type:person" --filter "has:temporal"
```

Exclude results using `--exclude` (same syntax as `--filter`):

```bash
# Exclude draft documents
factbase search "query" --exclude "type:draft"

# Exclude archived documents
factbase search "query" --exclude "type:archived"

# Exclude orphan documents (no links)
factbase search "query" --exclude "links:0"

# Combine include and exclude filters
factbase search "query" --filter "type:person" --exclude "has:temporal"
```

### Type Exclusion

Exclude specific document types using `-T/--exclude-type` (can be repeated):

```bash
# Exclude draft documents
factbase search "API" --exclude-type draft

# Exclude multiple types
factbase search "meeting" --exclude-type draft --exclude-type archived

# Combine with type filter
factbase search "engineer" -t person --exclude-type archived
```

### Temporal Filtering

Filter search results by when facts were valid:

```bash
# Find facts valid in June 2021
factbase search --as-of 2021-06 "CTO"

# Find facts valid during 2020-2022
factbase search --during 2020..2022 "role"

# Exclude unverified facts
factbase search --exclude-unknown "location"
```

### Confidence Levels

Temporal tags indicate confidence in fact accuracy:

| Tag Type | Confidence | Meaning |
|----------|------------|---------|
| `@t[=DATE]` | High | Verified at specific point in time |
| `@t[DATE..DATE]` | High | Verified for specific date range |
| `@t[DATE..]` | Medium | Started at date, assumed ongoing |
| `@t[..DATE]` | Medium | Historical, ended at date |
| `@t[~DATE]` | Medium | Last known/verified at date |
| `@t[?]` | Low | Unknown or unverified |
| No tag | Unknown | Treat as low confidence |

Use `--exclude-unknown` to filter out `@t[?]` and untagged facts for higher confidence results.

## `factbase grep <pattern> [-t <type>] [-T <type>] [-r <repo>] [-l <n>] [-j] [-f <format>] [-H] [-C <n>] [--stats] [--count] [--since <date>] [--dry-run] [-w]`

Search document content for exact text matches (like grep).

- `-t, --doc-type` - Filter by document type
- `-T, --exclude-type` - Exclude documents of this type (can be repeated)
- `-r, --repo` - Filter by repository
- `-l, --limit` - Max results (default: 10)
- `-j, --json` - Output as JSON (shorthand for --format json)
- `-f, --format` - Output format: table (default), json, yaml
- `-q, --quiet` - Suppress non-essential output
- `-H, --highlight` - Highlight matched text (default: auto-detect terminal)
- `-C, --context` - Show N lines of context before and after each match (default: 0)
- `--stats` - Show match statistics instead of full results
- `--count` - Output only the count of matching results
- `--since` - Only search files modified since date (ISO 8601 or relative: 1h, 1d, 1w)
- `--dry-run` - Validate pattern and show search scope without searching
- `-w, --watch` - Watch for file changes and re-run search (live updating results)

```bash
factbase grep "TODO"
# First Doc [abc123] (people/first.md)
#   Line 3: TODO fix this bug

factbase grep "API" --json
# [{"id": "abc123", "title": "API Guide", "matches": [...]}]

# Show 2 lines of context around each match
factbase grep -C 2 "pattern"

# Exclude draft and archived documents
factbase grep --exclude-type draft --exclude-type archived "TODO"

# Quick count of matches
factbase grep --count "FIXME"

# Search only recently modified files
factbase grep --since 1d "TODO"

# Watch for changes and re-run search
factbase grep -w "TODO"
```

Highlighting is automatically enabled when output is to a terminal and disabled when piped to other tools. Respects the `NO_COLOR` environment variable.

## `factbase stats [-s] [-j]`

Show quick aggregate statistics across all repositories.

- `-s, --short` - Single-line output for scripting
- `-j, --json` - Output as JSON

```bash
factbase stats
# Factbase Stats
# ==============
# Repositories: 2
# Documents:    45
# Database:     128 KB
# Last scan:    2024-01-25 12:00:00
```

## `factbase status [--repo <repo>] [-d] [-j] [-f <format>] [--offline] [--since <date>]`

Show detailed repository statistics.

- `-d, --detailed` - Show extended stats (most linked docs, orphans, avg size, pool utilization)
- `-j, --json` - Output as JSON (shorthand for --format json)
- `-f, --format <table|json|yaml>` - Output format (default: table)
- `--offline` - Explicit offline mode (no-op: status never contacts inference backend)
- `--since` - Only include documents modified since date (ISO 8601 or relative: 1h, 1d, 1w)

This command reads only from the local database and works offline.

## `factbase serve`

Start MCP server on localhost:3000 and watch for file changes.

## `factbase export <repo> <output> [--with-metadata] [--format <md|json>] [--compress] [--stdout]`

Export documents from a repository.

- `--with-metadata` - Include `_metadata.json` file with links and types (md format only)
- `--format <md|json>` - Output format: md (markdown files, default) or json (single JSON file)
- `--compress` - Compress output with zstd (creates .zst file or .tar.zst archive)
- `--stdout` - Write output to stdout instead of file (only for json/md formats, cannot be used with --compress)

```bash
# Export markdown files to directory
factbase export myrepo ./backup

# Export as compressed tar archive
factbase export myrepo ./backup.tar.zst --compress

# Export JSON to stdout for piping
factbase export myrepo - --format json --stdout | jq '.[] | .title'
```

## `factbase import <repo> <input> [--overwrite] [--include <pattern>]`

Import documents into a repository. Auto-detects compressed files.

- `--overwrite` - Overwrite existing files (default: skip)
- `--include <pattern>` - Import only files matching glob pattern

Supported formats: directory of markdown files, `.tar.zst`, `.json.zst`, `.json`, `.md.zst`.

```bash
factbase import myrepo ./backup
factbase import myrepo ./backup.tar.zst
factbase import myrepo ./backup --overwrite
```

## `factbase completions <shell> [--with-repos]`

Generate shell completions. Supports: bash, zsh, fish, powershell, elvish.

- `--with-repos` - Include current repository IDs in completions (regenerate after adding/removing repos)

```bash
# Bash
factbase completions bash > ~/.local/share/bash-completion/completions/factbase

# Zsh
factbase completions zsh > ~/.zfunc/_factbase

# Fish
factbase completions fish > ~/.config/fish/completions/factbase.fish
```

## `factbase doctor`

Check inference backend connectivity and model availability.

- `--fix` - Auto-fix common issues (create config, pull missing Ollama models)
- `--dry-run` - Show what would be fixed without making changes
- `-q, --quiet` - Suppress output on success (exit 0 if healthy, 1 if not)
- `-j, --json` - Output as JSON
- `--timeout <SECONDS>` - HTTP timeout in seconds (default: from config, typically 30)

```bash
factbase doctor
# ✓ Embedding provider: bedrock (amazon.titan-embed-text-v2:0)
# ✓ LLM provider: bedrock (us.anthropic.claude-3-5-haiku-20241022-v1:0)
# All checks passed. Ready to scan.
```

## `factbase check [--repo <repo>] [--min-length <n>] [--max-age <days>] [--check-duplicates] [--min-similarity <n>] [--fix]`

Check knowledge base quality for common issues.

- `-r, --repo` - Check specific repository (all repos if omitted)
- `--min-length` - Minimum document length in characters (default: 100)
- `--max-age` - Warn about documents not modified in N days
- `--check-duplicates` - Check for duplicate or near-duplicate documents
- `--min-similarity` - Minimum similarity threshold for duplicates (default: 0.95, range: 0.0-1.0)
- `--fix` - Auto-fix broken links by removing them (prompts for confirmation)
- `--incremental` - Only check documents modified since last check (tracks timestamp per repository)
- `--since` - Only check files modified since date (ISO 8601 or relative: 1h, 1d, 1w)
- `-a, --check-all` - Run all validation checks (equivalent to --check-temporal --check-sources --check-duplicates)
- `--deep-check` - Cross-validate facts across documents using fact-level embeddings (slower, requires LLM). Uses pre-computed fact embeddings to find semantically similar fact pairs and classify conflicts.
- `-p, --parallel` - Process documents in parallel for faster checking
- `--batch-size` - Process documents in batches of N to limit memory usage (default: 0 = no batching)

Checks for: orphan documents, broken `[[id]]` links, stub documents, unknown types, stale documents, duplicates.

```bash
factbase check
factbase check --max-age 365
factbase check --incremental
```

## `factbase check [--repo <repo>]`

Generate review questions for documents using LLM analysis.

- `--review` - Enable question generation mode
- `--check-temporal` - Validate temporal tag format and consistency
- `--check-sources` - Validate source footnotes for orphans

Generates questions for: `@q[temporal]`, `@q[conflict]`, `@q[missing]`, `@q[ambiguous]`, `@q[stale]`, `@q[duplicate]`, `@q[precision]`.

## `factbase review --apply [--repo <repo>] [--dry-run]`

Process answered review questions and update documents.

- `--apply` - Process answered questions
- `--repo` - Limit to specific repository
- `--dry-run` - Show proposed changes without applying

## `factbase review --status [--repo <repo>] [-j]`

Show summary of pending review questions.

- `--status` - Show queue summary
- `--repo` - Limit to specific repository
- `-j, --json` - Output as JSON

## `factbase review --import-questions <path> [--repo <repo>] [--dry-run]`

Import review questions from JSON/YAML file (complement to `check --export-questions`).

## `factbase db vacuum`

Optimize database by running VACUUM and ANALYZE. Reclaims space from deleted documents.

## `factbase db backfill-word-counts`

Backfill word counts for documents created before the word_count column was added. One-time operation after upgrading.

## `factbase organize`

Self-organizing knowledge base commands for restructuring documents.

- `factbase organize analyze` - Detect merge/split/misplaced candidates
- `factbase organize merge <id1> <id2>` - Merge two documents
- `factbase organize split <id>` - Split document by sections
- `factbase organize move <id> --to <folder>` - Move document to new folder
- `factbase organize retype <id> --type <type>` - Override document type
- `factbase organize apply` - Process answered orphan markers

## Review Workflow

The review system enables human-in-the-loop quality improvement for fact documents.

### Workflow Overview

```
1. Generate questions    →  factbase check
2. Answer questions      →  Edit markdown files
3. Apply answers         →  factbase review --apply
4. Repeat as needed
```

### Answering Questions

Run `check` to append a Review Queue section to documents with issues:

```markdown
<!-- factbase:review -->
## Review Queue

- [ ] `@q[temporal]` Line 5: "VP Engineering at BigCo" - when was this true?
  > 
```

Edit the markdown file: check the checkbox `[x]` and add your answer in the blockquote:

```markdown
- [x] `@q[temporal]` Line 5: "VP Engineering at BigCo" - when was this true?
  > Started March 2022, left December 2024
```

Run `review --apply` to process answers. The LLM interprets your answers and updates the document (e.g., adding `@t[2022-03..2024-12]`).

### Special Answers

- `dismiss` or `ignore` - Remove question without changes
- `delete` - Remove the referenced fact entirely
- `split: <instruction>` - Split fact into multiple lines

### Question Types

| Type | Meaning |
|------|---------|
| `@q[temporal]` | Missing time info |
| `@q[conflict]` | Contradictory facts |
| `@q[missing]` | No source |
| `@q[ambiguous]` | Unclear meaning |
| `@q[stale]` | Outdated info |
| `@q[duplicate]` | Similar document |
| `@q[corruption]` | Data corruption (malformed temporal tags) |
| `@q[precision]` | Imprecise language (vague qualifiers) |

See [docs/review-system.md](review-system.md) for the complete specification.
