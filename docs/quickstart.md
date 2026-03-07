# Quickstart Guide

Get a searchable knowledge base running in under 2 minutes.

## 1. Install

```bash
git clone https://gitea.home.everyonce.com/daniel/factbase.git
cd factbase
cargo install --path .
```

This installs `factbase` to `~/.cargo/bin/`.

## 2. Set up your knowledge base

Point factbase at a folder of markdown files:

```bash
cd ~/notes          # or wherever your markdown lives
factbase init .
```

This creates a `.factbase` config directory. If you don't have markdown files yet, create a few:

```bash
mkdir -p people projects
echo "# Alice\n\n- Engineer at Acme Corp\n- Based in Seattle" > people/alice.md
echo "# Project Alpha\n\n- Started Q1 2025\n- Lead: Alice" > projects/alpha.md
```

## 3. Index

```bash
factbase scan
```

Factbase reads every `.md` file, generates embeddings for semantic search and cross-document validation, and detects cross-references between documents.

By default, factbase uses local CPU embeddings (BGE-small-en-v1.5, 384-dim) — no cloud credentials or external services needed. First scan downloads the model (~33MB, one-time) and takes a few seconds per document. Subsequent scans only process changed files.

## 4. Search

```bash
factbase search "who works on alpha"
```

Returns documents ranked by semantic similarity — it understands meaning, not just keywords. For exact text matching, use `grep`:

```bash
factbase grep "Acme Corp"
```

## 5. Keep it updated

Edit your markdown files with any tool. Then either:

- Run `factbase scan` again (only processes changes), or
- Run `factbase serve` to start a file watcher that auto-indexes on save

## Plain markdown is all you need

Factbase works with any markdown files — no special syntax required. Just write normal markdown and factbase handles indexing and search.

Temporal tags (`@t[...]`), source footnotes (`[^n]`), review questions (`@q[...]`), and inbox blocks (`<!-- factbase:inbox -->`) are optional power features for users who want structured fact tracking. You can adopt them later, or never — `factbase scan` and `factbase search` work perfectly without them.

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

## MCP integration

Factbase includes an MCP server so AI agents can search and manage your knowledge base. Choose a transport:

**Stdio (recommended for local use)** — the agent launches factbase as a subprocess, no server to manage:

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

Set `cwd` to the directory where you ran `factbase init`. Run `factbase scan` first to index your documents — the stdio transport doesn't auto-scan. It automatically exits when the parent process dies or after 5 minutes of inactivity.

**HTTP (for shared or remote access)** — start the server first, then point your agent at it:

```bash
factbase serve
```

```json
{
  "mcpServers": {
    "factbase": {
      "url": "http://localhost:3000"
    }
  }
}
```

Then just talk to your agent:

- **"Search factbase for info on Project Alpha"** — finds documents by meaning
- **"Research Jane Smith and add her to factbase"** — the agent will search your other tools (Slack, Outlook, web) for information and create a structured document
- **"Fix the factbase review queue"** — the agent will find stale or conflicting facts and resolve them using your data sources
- **"Improve the person documents in factbase"** — the agent will scan for gaps and fill them in

Factbase's workflow tools guide the agent step by step — you don't need to write prompts or know tool names.

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
- `factbase check` — check for quality issues (orphan docs, broken links)
- `factbase doctor` — verify your inference backend is working
- See the full [README](../README.md) for all commands and configuration options
