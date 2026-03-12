# Factbase

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A knowledge base that lives in your filesystem. Markdown in, semantic search out. Managed by your AI agent via MCP.

## Quick Start

Add factbase to your agent's MCP config:

```json
{
  "mcpServers": {
    "factbase": {
      "command": "npx",
      "args": ["-y", "@everyonce/factbase", "mcp"],
      "cwd": "/path/to/your/knowledge"
    }
  }
}
```

Then talk to your agent. Six workflows cover the full lifecycle:

| What you say | What happens |
|---|---|
| "Build me a knowledge base about Pacific Northwest mushrooms" | `create` — designs structure, creates docs, scans |
| "Research chanterelle harvesting seasons and add it" | `add` — researches topic, creates entities, links |
| "Run maintenance on the KB" | `maintain` — scan, quality check, resolve questions |
| "Check for new species classifications this year" | `refresh` — searches live sources, updates stale facts |
| "Amanita muscaria is toxic but rarely fatal, not deadly" | `correct` — finds and fixes the false claim everywhere |
| "The genus Agaricus was reclassified under the new taxonomy as of 2025" | `transition` — asks how to rename, applies it with history |

That's it. Your agent handles scanning, indexing, and search automatically. Everything below is optional depth.

---

## Features

- **Semantic search** - Find documents by meaning, not just keywords
- **Automatic link detection** - String matching discovers entity references across documents
- **Cross-document validation** - Fact-level embeddings detect conflicts across documents
- **Live updates** - File watcher keeps index in sync
- **MCP server** - AI agents can search and explore your knowledge base

Optional power features (plain markdown works without these):
- **Temporal tags** - Track when facts were valid with `@t[...]` annotations
- **Source attribution** - Footnotes for fact provenance
- **Review system** - Human-in-the-loop quality improvement via `@q[...]` questions
- **Self-organizing** - Merge, split, and reorganize documents
- **Folder placement checks** - Detect misplaced documents via link graph analysis

## Prerequisites

- Rust 1.70+
- No other dependencies needed — local CPU embeddings work out of the box
- Optional: AWS credentials for [Amazon Bedrock](docs/inference-providers.md) (higher quality embeddings)
- Optional: [Ollama](https://ollama.ai) for self-hosted inference (see [docs/inference-providers.md](docs/inference-providers.md))

## Installation

### Via npm (recommended)

```bash
npx @everyonce/factbase mcp
```

No install needed — `npx` downloads the right binary for your platform. Or install globally:

```bash
npm i -g @everyonce/factbase
factbase mcp
```

### From source

```bash
git clone https://gitea.home.everyonce.com/daniel/factbase.git
cd factbase
cargo install --path .
```

### Feature Flags

| Feature | Description | Binary Size |
|---------|-------------|-------------|
| `full` (default) | All features enabled (includes Bedrock + local embeddings) | ~25 MB |
| `bedrock` | Amazon Bedrock inference backend | +7 MB |
| `local-embedding` | Local CPU embeddings via fastembed (BGE-small-en-v1.5) | +8 MB |
| `progress` | Progress bars during scan | +0.1 MB |
| `compression` | zstd compression for database storage | +0.6 MB |
| `mcp` | MCP server for AI agent integration | +1 MB |
| `web` | Web UI for human-in-the-loop review | +1 MB |
| (no features) | CLI-only with Ollama backend | 6.7 MB |

```bash
# Minimal build - CLI only with Ollama backend
cargo install --path . --no-default-features

# MCP server only (no progress bars or compression)
cargo install --path . --no-default-features --features mcp
```

## CLI Commands

See [docs/cli-reference.md](docs/cli-reference.md) for the full command reference with all flags and examples.

| Command | Description |
|---------|-------------|
| `factbase scan` | Index documents (embeddings, link detection) |
| `factbase status` | Show repository statistics |
| `factbase doctor` | Check embedding provider connectivity |
| `factbase repair` | Auto-fix document corruption |
| `factbase embeddings` | Manage vector embeddings (export, import, status) |
| `factbase serve` | Start MCP server + file watcher |
| `factbase mcp` | Run MCP stdio transport (for agent integration) |

Most operations (search, check, review, organize, CRUD) are available via MCP tools rather than CLI commands.

## Configuration

Config file: `~/.config/factbase/config.yaml`

```yaml
database:
  path: ~/.local/share/factbase/factbase.db
  pool_size: 4

embedding:
  provider: local              # default; or 'bedrock', 'ollama'
  # For bedrock/ollama, also set:
  # model: amazon.nova-2-multimodal-embeddings-v1:0
  # dimension: 1024
  # region: us-east-1    # AWS region (for bedrock) or base_url for ollama

server:
  host: 127.0.0.1
  port: 3000
  time_budget_secs: 180  # Time budget for document-scaling MCP operations (5-600)

web:
  enabled: false
  port: 3001

cross_validate:
  fact_similarity_threshold: 0.5  # Minimum similarity for fact pairs (0.0-1.0)
  batch_size: 10                  # Fact pairs per batch (1-50)
```

See [examples/config.yaml](examples/config.yaml) for all options including watcher, rate limiting, and compression settings.

## MCP Integration

Factbase exposes 3 MCP tools:

| Tool | Description |
|------|-------------|
| `search` | Semantic or content search with filters (doc_type, temporal, boost_recent) |
| `workflow` | Guided multi-step workflows (create, add, maintain, refresh, correct, transition) |
| `factbase` | Unified operations: CRUD, scan, check, review, organize, links, embeddings |

The `factbase` tool uses an `op` parameter to select operations:

| Category | Operations |
|----------|-----------:|
| Documents | `get_entity`, `list`, `perspective`, `create`, `update`, `delete`, `bulk_create` |
| Quality | `scan`, `check`, `detect_links` |
| Review | `review_queue`, `answer`, `deferred` |
| Organize | `organize` (action=analyze\|merge\|split\|move\|retype\|delete\|execute_suggestions) |
| Links | `links` (action=suggest\|store), `fact_pairs` |
| Meta | `authoring_guide`, `embeddings` (action=export\|import\|status) |

## Document Format

Documents are freeform markdown. Factbase injects a header for tracking:

```markdown
<!-- factbase:a1cb2b -->
# Document Title

Your content here...
```

- **ID**: 6-character hex, auto-generated on first scan
- **Title**: Extracted from first `# Heading` or filename
- **Type**: Derived from parent folder (`people/` → "person")

See [docs/fact-document-format.md](docs/fact-document-format.md) for the complete specification including temporal tags, source attribution, and review questions.

## Obsidian

Factbase works alongside [Obsidian](https://obsidian.md) — use Obsidian for reading, writing, and graph exploration while your agent handles indexing and search.

Enable Obsidian-compatible output in `perspective.yaml`:

```yaml
format:
  preset: obsidian
```

This switches to wikilinks (`[[Entity Name]]`), YAML frontmatter, and a collapsed callout for the review queue.

**Two-tool workflow:**

1. Edit or rename files in Obsidian as usual — Obsidian auto-updates its own wikilinks
2. Run `factbase(op=scan)` (or `factbase scan`) to sync the database with any path changes
3. Ask your agent to search, check quality, or add new content

**What works out of the box:**
- Graph view — factbase writes `[[wikilinks]]` that Obsidian renders as graph edges
- Tags — auto-generated from folder structure (`species/` → `#species`)
- Review queue — appears as a collapsed callout, expandable in Obsidian
- Editing — any content added in Obsidian is picked up on the next scan

**One friction point:** if you rename a file in Obsidian, the factbase database holds the old path until you run a scan. The document's ID is stable across renames, so no data is lost — just run `factbase(op=scan)` to resync.

## Troubleshooting

**Bedrock Access Denied** — Enable model access in the [Bedrock console](https://console.aws.amazon.com/bedrock/home#/modelaccess), or check IAM permissions for `bedrock:InvokeModel`.

**Embedding Dimension Mismatch** — Occurs after switching providers (e.g., local → Bedrock). Fix with `factbase scan --reindex`.

**Database Locked** — Another process is using the database. Check with `pgrep -a factbase`.

**General Diagnostics** — Run `factbase doctor` to check system health. For Ollama setup, see [docs/inference-providers.md](docs/inference-providers.md).

## Architecture

See [.kiro/steering/architecture.md](.kiro/steering/architecture.md) for details.

```
Markdown Files → Scanner/Processor → SQLite + sqlite-vec
                                           ↓
                      File Watcher ← → MCP Server (localhost:3000)
```

## License

MIT
