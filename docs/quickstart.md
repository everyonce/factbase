# Quickstart Guide

Get a searchable knowledge base running in under 2 minutes.

## 1. Install

```bash
git clone https://gitea.home.everyonce.com/daniel/factbase.git
cd factbase
cargo install --path .
```

This installs `factbase` to `~/.cargo/bin/`.

## 2. Index your knowledge base

Point factbase at a folder of markdown files:

```bash
cd ~/notes          # or wherever your markdown lives
factbase scan
```

Factbase auto-initializes on first scan — no separate setup step needed. It reads every `.md` file, generates embeddings for semantic search and cross-document validation, and detects cross-references between documents.

By default, factbase uses local CPU embeddings (BGE-small-en-v1.5, 384-dim) — no cloud credentials or external services needed. First scan downloads the model (~33MB, one-time) and takes a few seconds per document. Subsequent scans only process changed files.

If you don't have markdown files yet, create a few:

```bash
mkdir -p people projects
echo "# Alice\n\n- Engineer at Acme Corp\n- Based in Seattle" > people/alice.md
echo "# Project Alpha\n\n- Started Q1 2025\n- Lead: Alice" > projects/alpha.md
factbase scan
```

## 3. Search via MCP

Factbase is designed for AI agent access via MCP. Add it to your agent's config:

```json
{
  "mcpServers": {
    "factbase": {
      "command": "factbase",
      "args": ["mcp"],
      "cwd": "/path/to/your/notes"
    }
  }
}
```

Then talk to your agent:

- **"Search factbase for info on Project Alpha"** — semantic search
- **"Research Jane Smith and add her to factbase"** — the agent creates structured documents
- **"Fix the factbase review queue"** — the agent resolves stale or conflicting facts

Factbase's workflow tools guide the agent step by step.

## 4. Keep it updated

Edit your markdown files with any tool. Then either:

- Run `factbase scan` again (only processes changes), or
- Run `factbase serve` to start a file watcher that auto-indexes on save

## Plain markdown is all you need

Factbase works with any markdown files — no special syntax required. Just write normal markdown and factbase handles indexing and search.

Temporal tags (`@t[...]`), source footnotes (`[^n]`), review questions (`@q[...]`), and inbox blocks (`<!-- factbase:inbox -->`) are optional power features for users who want structured fact tracking. You can adopt them later, or never.

## What just happened?

Factbase injected a small tracking comment into each file:

```markdown
<!-- factbase:a1cb2b -->
# Alice
```

This 6-character ID lets factbase track documents through renames and moves. It's the only thing factbase writes to your files.

Your documents are stored in a SQLite database at `~/.local/share/factbase/factbase.db` alongside their embeddings and detected links.

## Inference backend

Factbase uses local CPU embeddings by default — no cloud credentials needed. The BGE-small-en-v1.5 model (384 dimensions, ~33MB) downloads automatically on first use.

For higher quality embeddings on large knowledge bases, configure Amazon Bedrock:

```yaml
# ~/.config/factbase/config.yaml
embedding:
  provider: bedrock
  model: amazon.titan-embed-text-v2:0
  dimension: 1024
  region: us-east-1
```

Bedrock requires AWS credentials. Verify with:

```bash
aws sts get-caller-identity
```

If you switch providers after scanning, run `factbase scan --reindex` to rebuild embeddings with the new dimension.

For self-hosted inference with Ollama, see [inference-providers.md](inference-providers.md).

## Set your perspective

Edit `perspective.yaml` in your repository root to tell agents what this knowledge base is about:

```yaml
organization: AWS
focus: Customer tracking for solutions architects

review:
  stale_days: 180
  required_fields:
    person: [current_role, company, location]
    company: [industry, aws_usage]
```

This context flows into the workflow instructions automatically — the agent will know your org, use your staleness threshold, and check for your required fields.

## Next steps

- `factbase status` — see what's indexed
- `factbase doctor` — verify your embedding provider is working
- See the full [README](../README.md) for all commands and configuration options
- See the [agent integration guide](agent-integration.md) for MCP setup
