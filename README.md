# Factbase

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-1675_passing-brightgreen.svg)]()

Filesystem-based knowledge management with semantic search for AI agents.

Factbase indexes markdown files and provides semantic search via MCP (Model Context Protocol). The filesystem is the source of truth—edit files with any tool, and factbase keeps the index updated.

## Using Factbase with Your Agent

**1. Point your agent at a directory** of markdown files:

```json
{
  "mcpServers": {
    "factbase": {
      "command": "npx",
      "args": ["-y", "@everyonce/factbase", "mcp"],
      "cwd": "/home/you/my-notes"
    }
  }
}
```

The factbase auto-initializes on first launch. Then tell your agent:

> "Scan the factbase"

**2. Search it:**

> "What do we know about Project Atlas?"

**3. After editing files or adding new ones:**

> "Rescan the factbase"

That's it. Everything below is optional depth.

## Features

- **Semantic search** - Find documents by meaning, not just keywords
- **Automatic link detection** - LLM discovers entity references across documents
- **Live updates** - File watcher keeps index in sync
- **MCP server** - AI agents can search and explore your knowledge base
- **Multi-repository** - Manage multiple knowledge bases from one instance

Optional power features (plain markdown works without these):
- **Temporal tags** - Track when facts were valid with `@t[...]` annotations
- **Source attribution** - Footnotes for fact provenance
- **Review system** - Human-in-the-loop quality improvement via `@q[...]` questions
- **Self-organizing** - Merge, split, and reorganize documents

## Prerequisites

- Rust 1.70+
- AWS credentials configured (for Amazon Bedrock — the default inference backend)
- Or [Ollama](https://ollama.ai) for self-hosted inference (see [docs/inference-providers.md](docs/inference-providers.md))

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
| `full` (default) | All features enabled (includes Bedrock) | 16 MB |
| `bedrock` | Amazon Bedrock inference backend | +7 MB |
| `progress` | Progress bars during scan | +0.1 MB |
| `compression` | zstd compression for export/import and database storage | +0.6 MB |
| `mcp` | MCP server for AI agent integration | +1 MB |
| `web` | Web UI for human-in-the-loop review | +1 MB |
| (no features) | CLI-only with Ollama backend | 6.7 MB |

```bash
# Minimal build - CLI only with Ollama backend
cargo install --path . --no-default-features

# MCP server only (no progress bars or compression)
cargo install --path . --no-default-features --features mcp
```

## Quick Start

```bash
cd ~/notes                        # your markdown files
factbase init .                   # initialize
factbase scan                     # index everything
factbase search "project status"  # semantic search
```

**→ [Full quickstart guide](docs/quickstart.md)** — from zero to searching in 2 minutes, including Bedrock setup and MCP integration.

**→ [Agent integration guide](docs/agent-integration.md)** — add factbase to your agent's MCP config and say "research Jane Smith for factbase" to start.

## CLI Commands

See [docs/cli-reference.md](docs/cli-reference.md) for the full command reference with all flags and examples.

| Command | Description |
|---------|-------------|
| `factbase init <path>` | Initialize a new repository |
| `factbase scan` | Index documents (embeddings + link detection) |
| `factbase search <query>` | Semantic search across documents |
| `factbase grep <pattern>` | Exact text search (like grep) |
| `factbase serve` | Start MCP server + file watcher |
| `factbase status` | Show repository statistics |
| `factbase stats` | Quick aggregate statistics |
| `factbase doctor` | Check inference backend connectivity |
| `factbase check` | Check knowledge base quality |
| `factbase review --apply` | Process answered review questions |
| `factbase review --status` | Show review queue summary |
| `factbase export <repo> <out>` | Export documents (markdown, JSON, compressed) |
| `factbase import <repo> <in>` | Import documents |
| `factbase organize analyze` | Detect merge/split/misplaced candidates |
| `factbase repo list` | List registered repositories |
| `factbase db vacuum` | Optimize database |
| `factbase completions <shell>` | Generate shell completions |

## Configuration

Config file: `~/.config/factbase/config.yaml`

```yaml
database:
  path: ~/.local/share/factbase/factbase.db
  pool_size: 4

embedding:
  provider: bedrock
  model: amazon.nova-2-multimodal-embeddings-v1:0
  dimension: 1024
  region: us-east-1    # AWS region (for bedrock) or base_url for ollama

llm:
  provider: bedrock
  model: us.anthropic.claude-haiku-4-5-20251001-v1:0
  region: us-east-1

server:
  host: 127.0.0.1
  port: 3000

web:
  enabled: false
  port: 3001
```

See [examples/config.yaml](examples/config.yaml) for all options including watcher, rate limiting, and compression settings.

## MCP Integration

Factbase exposes 21 MCP tools:

| Tool | Description |
|------|-------------|
| `search_knowledge` | Semantic search with optional type/repo/temporal filters |
| `search_content` | Exact text search (like grep) |
| `get_entity` | Get document by ID with incoming/outgoing links |
| `list_entities` | List documents with optional type/repo filters |
| `list_repositories` | List all registered repositories |
| `get_perspective` | Get repository context from perspective.yaml |
| `create_document` | Create a new document in a repository |
| `update_document` | Update an existing document's title or content |
| `delete_document` | Delete a document by ID |
| `bulk_create_documents` | Create multiple documents atomically (max 100) |
| `scan_repository` | Index (or re-index) all documents — run after adding/editing files |
| `init_repository` | Initialize a new factbase repository at a directory path |
| `get_review_queue` | Get pending review questions |
| `answer_questions` | Answer or defer one or more review questions |
| `check_repository` | Run quality checks and generate review questions |
| `apply_review_answers` | Apply answered review questions to document content |
| `get_deferred_items` | Get deferred review items needing human attention |
| `workflow` | Run a guided workflow (update, resolve, ingest, enrich, improve) |
| `organize_analyze` | Detect reorganization opportunities (merge, split, misplaced, duplicates) |
| `organize` | Execute reorganization actions (merge, split, move, retype, apply) |
| `get_authoring_guide` | Get document formatting rules and templates |

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

## Troubleshooting

**Bedrock Access Denied** — Enable model access in the [Bedrock console](https://console.aws.amazon.com/bedrock/home#/modelaccess), or check IAM permissions for `bedrock:InvokeModel` and `bedrock:Converse`.

**Embedding Dimension Mismatch** — Occurs after switching models. Fix with `factbase scan --reindex`.

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
