# Agent Integration Guide

Add factbase as an MCP server to your agent, then talk to it naturally.

## Setup

1. Download the factbase binary for your platform from the [releases page](https://gitea.home.everyonce.com/daniel/factbase/releases), or build from source:
   ```bash
   git clone https://gitea.home.everyonce.com/daniel/factbase.git
   cd factbase && cargo install --path .
   ```

2. Add factbase to your agent's MCP config (choose one transport):

### Stdio transport (recommended for local use)

The agent launches factbase as a subprocess — no server to start or manage:

   ```json
   {
     "mcpServers": {
       "factbase": {
         "command": "npx",
         "args": ["-y", "@everyonce/factbase", "mcp"],
         "cwd": "/path/to/your/knowledge-base"
       }
     }
   }
   ```

Set `cwd` to the directory containing your knowledge base. The factbase auto-initializes on first launch — no `init` or `scan` needed beforehand. Just tell your agent "scan the factbase" to index.

### HTTP transport (for shared or remote access)

Start the server first, then point your agent at it:

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

Use HTTP when multiple agents need to share the same factbase instance, or when the server runs on a different machine. The HTTP transport uses Streamable HTTP per MCP spec 2025-03-26 and supports the full MCP lifecycle (initialize → tools/list → tools/call).

4. Start talking to your agent:

   > "Research Jane Smith and add her to factbase"

   > "Search factbase for info on Project Alpha"

   > "Fix the factbase review queue"

That's it. The agent handles the rest.

## What You Can Say

Factbase's workflow tools pick up natural requests automatically:

| You say | What happens |
|---------|-------------|
| "Research Jane Smith and add her to factbase" | Ingest workflow: searches existing data, researches externally, creates/updates documents |
| "Search factbase for info on Project Alpha" | Semantic search across all documents |
| "Fix the factbase review queue" | Resolve workflow: finds stale/conflicting/missing data, researches fixes, applies answers |
| "Improve the person documents in factbase" | Enrich workflow: scans for gaps, researches missing info, updates documents |
| "Lint factbase" or "check factbase for quality issues" | Runs check_repository to generate review questions across all documents |

## Repository Perspective

Edit `perspective.yaml` in your repository root to tell the agent what this factbase is about:

```yaml
organization: AWS
focus: Customer tracking for solutions architects

review:
  stale_days: 180
  required_fields:
    person: [current_role, company, location]
    company: [industry, aws_usage]
```

This flows into workflow instructions automatically — the agent knows your org context, uses your staleness threshold, and checks for your required fields.

## The Quality Loop

Factbase improves iteratively:

1. **Ingest** — agent adds documents from your data sources
2. **Lint** — tell your agent "lint factbase" or "check factbase for quality issues" — it runs `check_repository` to generate review questions across all documents
3. **Resolve** — tell your agent "fix the factbase review queue" — it researches and resolves each question
4. **Repeat** — each cycle produces fewer questions until documents stabilize

## MCP Tools

The agent has access to 21 tools. You don't need to know them — the workflows handle tool selection. For reference:

| Category | Tools |
|----------|-------|
| Indexing | `scan_repository` |
| Search | `search_knowledge`, `search_content`, `search_temporal` |
| Read | `get_entity`, `get_document_stats`, `list_entities`, `list_repositories`, `get_perspective` |
| Write | `create_document`, `update_document`, `delete_document`, `bulk_create_documents` |
| Quality | `get_review_queue`, `answer_question`, `bulk_answer_questions`, `generate_questions`, `check_repository` |
| Workflows | `workflow_start`, `workflow_next` |
| Organize | `get_duplicate_entries` |

## Tips

- **Start small.** Add a few documents, resolve the review queue, then scale up.
- **The review loop converges.** Each cycle produces fewer questions. After 2-3 cycles, most documents stabilize.
- **Connect your data sources as MCP servers too.** The more tools your agent has (Slack, Outlook, web search), the better it can research and resolve.
