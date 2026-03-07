# Current State & Known Limitations

## Project Status

**Phases 1-50 complete**. Releases: v0.1.0, v0.2.0, v0.3.0, v0.4.0, v0.4.1, v0.4.2, v0.4.3. Current Cargo.toml version: v50.85.1.

### Active Work
- No active phases. All work through Phase 50 is complete.

## Current Configuration

### Embedding Model
- **Default**: BAAI/bge-small-en-v1.5 (384 dimensions) via local CPU (fastembed)
- **Alternative**: amazon.titan-embed-text-v2:0 (1024 dimensions) via Bedrock
- **Alternative**: amazon.nova-2-multimodal-embeddings-v1:0 via Bedrock

### LLM Model
- **Status**: Removed (Phase 6 complete). All reasoning is now agent-driven via MCP.

## Known Limitations

### Link Detection Truncation  
- **Current limit**: ~40K chars per document in batch mode
- **Impact**: Entity mentions beyond limit may not be detected in very long documents

## CLI Commands Reference

### Core Commands
- `factbase init` - Create config file
- `factbase scan [--repo <id>]` - Index documents
- `factbase search <query>` - Semantic search
- `factbase serve` - Start MCP server + file watcher (+ web server if enabled)
- `factbase mcp` - Run MCP stdio transport (for agent integration)

### Repository Management
- `factbase repo add <id> <path>` - Register repository
- `factbase repo remove <id>` - Unregister repository
- `factbase repo list` - List all repositories

### Utilities
- `factbase status [--repo <id>]` - Show statistics
- `factbase stats` - Quick aggregate stats
- `factbase doctor` - Check inference backend connectivity
- `factbase check [--repo <id>]` - Quality checks
- `factbase check --deep-check` - Cross-document fact validation (requires inference backend)
- `factbase grep <pattern>` - Content search (like grep)
- `factbase export <repo> <output>` - Backup documents
- `factbase import <repo> <input>` - Restore documents
- `factbase db vacuum` - Optimize database
- `factbase db backfill-word-counts` - Populate word counts for existing docs
- `factbase completions <shell>` - Generate shell completions
- `factbase review --apply` - Process answered review questions
- `factbase review --status` - Show review queue summary (includes deferred count)

### Organize Commands (Phase 10)
- `factbase organize analyze` - Detect merge/split/misplaced/duplicate candidates
- `factbase organize merge <id1> <id2>` - Merge two documents
- `factbase organize split <id>` - Split document by sections
- `factbase organize move <id> --to <folder>` - Move document to new folder
- `factbase organize retype <id> --type <type>` - Override document type
- `factbase organize apply` - Process answered orphan markers

## MCP Tools (27)

### Search Operations
| Tool | Description |
|------|-------------|
| `search_knowledge` | Semantic search with filters |
| `search_content` | Text/regex search (like grep) |

### Entity Operations
| Tool | Description |
|------|-------------|
| `get_entity` | Get document by ID with links |
| `list_entities` | List documents with filters |
| `get_perspective` | Get repository context |
| `list_repositories` | List all repositories |

### Document CRUD
| Tool | Description |
|------|-------------|
| `create_document` | Create new document |
| `update_document` | Update title or content |
| `delete_document` | Delete document by ID |
| `bulk_create_documents` | Create multiple documents atomically |

### Review Operations
| Tool | Description |
|------|-------------|
| `get_review_queue` | Get pending review questions |
| `answer_questions` | Answer one or more review questions |
| `generate_questions` | Generate review questions for a document |
| `check_repository` | Run quality checks (modes: questions, cross_validate, discover) |
| `apply_review_answers` | Process answered review questions |
| `get_deferred_items` | Get deferred questions needing human attention |

### Workflow & Scan Operations
| Tool | Description |
|------|-------------|
| `workflow` | Guided workflow (bootstrap, setup, resolve, ingest, enrich, improve) |
| `scan_repository` | Index (or re-index) all documents |
| `init_repository` | Initialize a new repository |
| `organize_analyze` | Detect reorganization opportunities (merge, split, misplaced, duplicates) |
| `organize` | Execute reorganization actions (merge, split, move, retype, apply) |
| `get_authoring_guide` | Get document authoring guide |

### Embedding Operations
| Tool | Description |
|------|-------------|
| `embeddings_export` | Export pre-computed embeddings as JSONL |
| `embeddings_import` | Import pre-computed embeddings |
| `embeddings_status` | Check embedding index coverage and stats |

### Link Operations
| Tool | Description |
|------|-------------|
| `get_link_suggestions` | Find documents with few links and suggest similar unlinked candidates |
| `store_links` | Write `[[id]]` references into document files' Links: blocks |

## Web API Endpoints (20 total, feature-gated)

Requires `web` feature and `web.enabled = true` in config.

### Stats
- `GET /api/stats` - Aggregate stats
- `GET /api/stats/review` - Review queue counts (includes deferred)
- `GET /api/stats/organize` - Organize suggestion counts

### Review
- `GET /api/review/queue` - List pending questions
- `GET /api/review/queue/{doc_id}` - Questions for document
- `POST /api/review/answer/{doc_id}` - Submit answer
- `POST /api/review/bulk-answer` - Submit multiple answers
- `GET /api/review/status` - Queue summary (includes deferred)

### Actions
- `POST /api/apply` - Apply answered review questions (agent-driven)
- `POST /api/scan` - Trigger scan (returns CLI instructions)
- `POST /api/check` - Trigger quality checks (returns CLI instructions)

### Organize
- `GET /api/organize/suggestions` - List suggestions
- `GET /api/organize/suggestions/{doc_id}` - Suggestions for document
- `POST /api/organize/approve` - Approve suggestion (redirects to CLI)
- `POST /api/organize/dismiss` - Dismiss suggestion
- `GET /api/organize/orphans` - List orphaned facts
- `POST /api/organize/assign-orphan` - Assign orphan to document

### Documents
- `GET /api/documents/{id}` - Get document with content
- `GET /api/documents/{id}/links` - Get document links
- `GET /api/repos` - List repositories

## Testing

### Unit Tests
- Run with: `cargo test --lib`
- No external dependencies required
- Currently: ~1731 lib tests (default features); ~1796 lib tests (with all features including web)

### Binary Tests
- Run with: `cargo test --bin factbase`
- No external dependencies required
- Currently: ~386 bin tests (default features); ~393 bin tests (with all features including web)

### Integration Tests (Require inference backend)
- Run with: `cargo test -- --ignored`
- Requires: Bedrock access or Ollama running with qwen3-embedding:0.6b and rnj-1-extended
- Currently: 73+ tests

### Frontend Tests (web feature)
- Run with: `cd web && npm test`
- Uses Vitest with jsdom environment
- Currently: 56 tests

### E2E Tests (web feature, requires running server)
- Run with: `cd web && npm run test:e2e`
- Requires: `cargo build --features web`, Ollama with models
- Uses Playwright with Chromium
- Currently: 12 tests

### Total: ~2117 unit/binary tests (default features), ~2189 (with all features) + 73+ integration tests + 56 frontend tests + 12 E2E tests

## Codebase Structure

The codebase has been modularized into focused submodules. See `.kiro/steering/module-interactions.md` for the complete file structure.

### Key Modules
| Module | Submodules |
|--------|------------|
| `config/` | database, embedding, processor, server, web, validation |
| `models/` | document, repository, link, search, scan, stats, temporal, question |
| `database/` | schema, documents/, repositories, links, embeddings, search/, stats/, compression |
| `processor/` | core, temporal/, sources, review, chunks, stats |
| `link_detection.rs` | DetectedLink, LinkDetector (string matching) |
| `scanner/` | options, progress, orchestration/ |
| `organize/` | types, extract, links, orphans, review, audit, snapshot, verify, detect/, plan/, execute/ |
| `question_generator/` | temporal, conflict, missing, ambiguous, stale, duplicate, corruption, precision, placement, fields, facts, cross_validate, check |
| `answer_processor/` | mod, interpret, apply, temporal, inbox, apply_all, validate |
| `commands/` | scan/, search/, grep/, status/, check/, review/, export/, import/, doctor/, organize/, mcp |
| `mcp/` | protocol, stdio, server, tools/ |
| `mcp/tools/` | schema, helpers, search, entity, document, organize, review/ |
| `web/` (feature-gated) | server, assets, api/ |
| `progress.rs` | ProgressReporter enum (Cli/Mcp/Silent), ProgressSender type alias |
| `embedding.rs` | EmbeddingProvider trait, OllamaEmbedding, test_helpers (MockEmbedding, HashEmbedding) |
