# Factbase Architecture Overview

## What is Factbase?

Factbase is a filesystem-based knowledge management system that indexes markdown files and provides semantic search via MCP (Model Context Protocol) for AI agents. Think of it as a personal/team knowledge base where the filesystem is the source of truth.

## Core Design Principles

### 1. Filesystem is Truth
- Markdown files on disk are the authoritative source
- No approval workflows - if a file exists, it's trusted
- Users can edit files with any tool (VS Code, vim, Obsidian, etc.)
- Git provides version control and audit trail

### 2. Document Identity via File Headers
- Each document gets a unique 6-character hex ID: `<!-- factbase:a1cb2b -->`
- ID is injected into the file itself on first scan
- ID travels with the file through moves/renames
- Enables tracking documents across filesystem changes

### 3. Freeform Markdown
- No required structure beyond our injected header
- Title extracted from first H1 (`# Title`) or filename
- Type derived from parent folder (`people/` → "person")
- Users write whatever markdown they want

### 4. Two-Phase Scanning
- **Pass 1**: Index documents, generate embeddings
- **Pass 2**: Detect links across ALL documents using LLM
- Second pass scans everything because new documents may be referenced by existing ones

### 5. Modular Provider Architecture
- `EmbeddingProvider` trait allows swapping embedding backends
- `LlmProvider` trait allows swapping LLM backends
- Currently supports Amazon Bedrock (default) and Ollama

## System Components

```
┌─────────────────────────────────────────────────────────────┐
│                    Markdown Files                            │
│  The source of truth - human-edited knowledge               │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              Scanner + Processor                             │
│  - Finds .md files, respects ignore patterns                │
│  - Extracts/injects document IDs                            │
│  - Parses title, derives type from folder                   │
│  - Generates embeddings via inference backend (batched, 10 at a time)  │
│  - Detects entity links via LLM (batched, 5 at a time)      │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              SQLite Database                                 │
│  - documents: metadata, content, file_path                  │
│  - document_embeddings: 1024-dim vectors (sqlite-vec)       │
│  - document_links: cross-references between docs            │
│  - repositories: multi-repo support                         │
│  - Connection pool via r2d2 (configurable size 1-32)        │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              File Watcher                                    │
│  - Monitors directories for changes                         │
│  - 500ms debounce to batch rapid edits                      │
│  - Triggers full rescan on any change                       │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              MCP Server                                      │
│  - HTTP server on localhost:3000 (configurable)             │
│  - 25 tools for AI agents (search, entity, CRUD, review, workflow, organize, embeddings)  │
│  - search_knowledge, search_content                         │
│  - get_entity, list_entities, get_perspective               │
│  - create/update/delete/bulk_create documents               │
│  - Review queue + workflow tools for human-in-the-loop QA   │
└─────────────────────────────────────────────────────────────┘
```

## Key Data Flows

### Document Indexing Flow
1. Scanner finds `.md` files in repository
2. For each file:
   - Read content, compute SHA256 hash
   - Extract or inject factbase ID header
   - Parse title from first H1 (or use filename)
   - Derive type from parent folder name
   - Generate embedding via inference backend (chunked for long docs)
   - Store in SQLite
3. After all documents indexed:
   - Run link detection pass on ALL documents
   - LLM identifies entity mentions
   - Store links in document_links table

### Search Flow
1. User/agent provides natural language query
2. Generate embedding for query via inference backend
3. Vector similarity search in sqlite-vec
4. Return ranked results with snippets

### Link Detection Flow
1. Get list of all document titles from database
2. Build prompt with known entities + document content
3. LLM returns JSON array of detected mentions
4. Also extract manual `[[id]]` links via regex
5. Merge and deduplicate
6. Store in document_links table

## Configuration

Global config at `~/.config/factbase/config.yaml`:
- Database location and pool size
- Repository paths
- Inference provider settings (embedding model, LLM model, region)
- Watcher settings (debounce, ignore patterns)
- Rate limiting settings

## Error Handling Philosophy

- **Inference errors are fatal**: If embedding/LLM fails, exit immediately
- User must fix the underlying issue (check credentials, model access, etc.)
- Partial indexing is worse than failing completely
- Clear error messages tell user what to do

## Thread Safety

- Database uses `r2d2` connection pool for thread-safe access
- Enables safe sharing between watcher thread and MCP server
- File watcher runs in background, triggers scans
- MCP server handles concurrent requests
- Graceful shutdown via `shutdown.rs` module

## Multi-Repository Support

- Single factbase instance can manage multiple repos
- Each repo has its own `perspective.yaml`
- Documents namespaced by `repo_id` in database
- Search can filter by repo or search across all
