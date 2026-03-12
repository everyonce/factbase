# CLI Reference

Complete command reference for factbase. See also the [README](../README.md) for a quick overview.

## `factbase scan`

Index documents. Generates document embeddings for semantic search, fact-level embeddings for cross-document validation, and detects entity links. Scans the current directory (or the registered repository containing it).

- `--detailed` - Show per-file processing details
- `-q, --quiet` - Suppress output except errors
- `-j, --json` - Output as JSON
- `--dry-run` - Preview changes without modifying database or calling embedding provider
- `-w, --watch` - Watch for file changes and rescan automatically
- `--check-duplicates` - Check for duplicate or near-duplicate documents (similarity > 95%)
- `--stats` / `--profile` - Show timing statistics for each scan phase
- `--since <date>` - Only process files modified since date (ISO 8601 or relative: 1h, 1d, 1w)
- `--stats-only` - Show quick statistics without modifying database or calling embedding provider
- `--verify` - Verify document integrity (check file exists, hash matches, ID header present)
- `--fix` - Auto-fix integrity issues found by --verify (re-inject headers, update database)
- `-y, --yes` - Skip confirmation prompts when using --fix
- `--prune` - Remove orphaned database entries for deleted files (soft delete by default)
- `--hard` - Permanently remove orphaned entries instead of soft delete (use with --prune)
- `--reindex` - Force re-generation of all embeddings even if content unchanged (useful after switching embedding providers)
- `--batch-size <n>` - Batch size for embedding generation (default: 10, from config)
- `--timeout <seconds>` - Timeout for API calls (default: from config)
- `--no-links` - Skip link detection phase for faster indexing
- `--no-embed` - Skip embedding generation (index documents without calling embedding provider)
- `--relink` - Force link detection on all documents (useful for migrated/copied KBs)
- `--check` - Validate index integrity for CI (check embeddings exist and dimensions match)
- `--progress` - Force progress bars even without TTY
- `--no-progress` - Disable progress bars but keep other output
- `--assess` - Assess existing files without modifying anything (onboarding report)

```bash
factbase scan                     # index everything
factbase scan --since 1d          # only recent changes
factbase scan --dry-run --stats   # preview with timing
factbase scan --verify --fix -y   # fix integrity issues
factbase scan --reindex           # rebuild all embeddings
factbase scan --no-links          # fast indexing, skip links
```

Repositories auto-initialize on first scan — no separate `init` step needed.

## `factbase status [-d] [-j] [-f <format>] [--since <date>]`

Show repository statistics.

- `-d, --detailed` - Show extended stats (most linked docs, orphans, avg size, pool utilization)
- `-j, --json` - Output as JSON (shorthand for --format json)
- `-q, --quiet` - Suppress non-essential output
- `-f, --format <table|json|yaml>` - Output format (default: table)
- `--since <date>` - Only include documents modified since date

This command reads only from the local database and works offline.

## `factbase doctor`

Check embedding provider connectivity and model availability.

- `--fix` - Auto-fix common issues (create config, pull missing Ollama models)
- `--dry-run` - Show what would be fixed without making changes
- `-q, --quiet` - Suppress output on success (exit 0 if healthy, 1 if not)
- `-j, --json` - Output as JSON
- `--timeout <seconds>` - HTTP timeout in seconds (default: from config)

```bash
factbase doctor
# ✓ Embedding provider: local (bge-small-en-v1.5, 384-dim)
# All checks passed. Ready to scan.
```

## `factbase repair [--doc <id>] [--dry-run] [-q]`

Auto-fix document corruption (malformed headers, broken footnotes, title issues).

- `--doc <id>` - Repair a single document by ID
- `--dry-run` - Preview changes without writing
- `-q, --quiet` - Suppress non-essential output

```bash
factbase repair              # repair all documents
factbase repair --doc a1b2c3 # repair one document
factbase repair --dry-run    # preview changes
```

## `factbase embeddings <subcommand>`

Manage vector embeddings.

### `factbase embeddings export`

Export embeddings to a JSONL file for backup or transfer.

### `factbase embeddings import`

Import embeddings from a JSONL file.

### `factbase embeddings status`

Show embedding coverage and statistics.

## `factbase serve`

Start MCP HTTP server on localhost:3000 and watch for file changes. Use this for shared or remote access.

## `factbase mcp`

Run MCP stdio transport for agent integration. This is the recommended way to connect factbase to AI agents — add it to your agent's MCP server config.

```json
{
  "mcpServers": {
    "factbase": {
      "command": "factbase",
      "args": ["mcp"],
      "cwd": "/path/to/your/knowledge-base"
    }
  }
}
```

## Hidden Commands

These commands are available but hidden from `--help`:

### `factbase db vacuum`

Optimize database by running VACUUM and ANALYZE.

### `factbase db stats`

Show database statistics.

### `factbase db backfill-word-counts`

Backfill word counts for documents created before the word_count column was added.

### `factbase completions <shell>`

Generate shell completions. Supports: bash, zsh, fish, powershell, elvish.

```bash
factbase completions bash > ~/.local/share/bash-completion/completions/factbase
factbase completions zsh > ~/.zfunc/_factbase
factbase completions fish > ~/.config/fish/completions/factbase.fish
```

### `factbase version`

Show version and configuration info.

## MCP Tools

Most operations (search, check, review, organize, export, import, CRUD) are available via MCP tools rather than CLI commands. Factbase exposes 3 MCP tools:

| Tool | Description |
|------|-------------|
| `search` | Semantic or content search with filters |
| `workflow` | Guided multi-step workflows (create, add, maintain, refresh, correct, transition) |
| `factbase` | Unified operations: CRUD, scan, check, review, organize, links, embeddings |

See the [agent integration guide](agent-integration.md) for details.
