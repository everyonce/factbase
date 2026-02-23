# Current State & Known Limitations

## Project Status

**Phases 1-45 complete (650+ tasks)**. Phase 46 pending. Releases: v0.1.0, v0.2.0, v0.3.0, v0.4.0, v0.4.1, v0.4.2, v0.4.3.

### Active Work
Phase 45: Cross-Document Fact Validation (tasks/phase45.md)
- Task 1 complete: Fact extraction expansion (`question_generator/facts.rs` with `extract_all_facts`)
- Task 2 complete: Per-fact semantic search (`question_generator/cross_validate.rs` with `cross_validate_document`)
- Task 3 complete: LLM conflict detection (`cross_validate.rs` with `build_prompt`, `parse_llm_response`, `result_to_question`)
- Task 4 complete: Integration into lint (`--cross-check` flag in `commands/lint/mod.rs`)
- Task 5 complete: Re-check tracking (5.1 schema migration v5, 5.2 hash update after validation, 5.3 linked doc invalidation on change)
- Task 6 pending: MCP and workflow integration

Phase 46: Cross-Document Entity Deduplication (tasks/phase46.md)
- Depends on Phase 45
- All tasks pending

## Current Configuration

### Embedding Model
- **Model**: amazon.titan-embed-text-v2:0 (1024 dimensions) via Bedrock
- **Alternative**: amazon.nova-2-multimodal-embeddings-v1:0

### LLM Model
- **Model**: us.anthropic.claude-3-5-haiku-20241022-v1:0 via Bedrock
- **Usage**: Link detection, review question generation

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
- `factbase lint [--repo <id>]` - Quality checks
- `factbase grep <pattern>` - Content search (like grep)
- `factbase export <repo> <output>` - Backup documents
- `factbase import <repo> <input>` - Restore documents
- `factbase db vacuum` - Optimize database
- `factbase db backfill-word-counts` - Populate word counts for existing docs
- `factbase completions <shell>` - Generate shell completions
- `factbase review --apply` - Process answered review questions
- `factbase review --status` - Show review queue summary

### Organize Commands (Phase 10)
- `factbase organize analyze` - Detect merge/split/misplaced candidates
- `factbase organize merge <id1> <id2>` - Merge two documents
- `factbase organize split <id>` - Split document by sections
- `factbase organize move <id> --to <folder>` - Move document to new folder
- `factbase organize retype <id> --type <type>` - Override document type
- `factbase organize apply` - Process answered orphan markers

## MCP Tools (18 total)

### Search Operations
| Tool | Description |
|------|-------------|
| `search_knowledge` | Semantic search with filters |
| `search_content` | Text/regex search (like grep) |
| `search_temporal` | Temporal-aware semantic search |

### Entity Operations
| Tool | Description |
|------|-------------|
| `get_entity` | Get document by ID with links |
| `list_entities` | List documents with filters |
| `get_perspective` | Get repository context |
| `list_repositories` | List all repositories |
| `get_document_stats` | Get document statistics |

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
| `answer_question` | Answer a single review question |
| `bulk_answer_questions` | Answer multiple questions |
| `generate_questions` | Generate review questions for document |

### Workflow Operations
| Tool | Description |
|------|-------------|
| `workflow_start` | Start a guided workflow (resolve, ingest, enrich) |
| `workflow_next` | Get next step in an active workflow |

## Web API Endpoints (17 total, feature-gated)

Requires `web` feature and `web.enabled = true` in config.

### Stats
- `GET /api/stats` - Aggregate stats
- `GET /api/stats/review` - Review queue counts
- `GET /api/stats/organize` - Organize suggestion counts

### Review
- `GET /api/review/queue` - List pending questions
- `GET /api/review/queue/{doc_id}` - Questions for document
- `POST /api/review/answer/{doc_id}` - Submit answer
- `POST /api/review/bulk-answer` - Submit multiple answers
- `GET /api/review/status` - Queue summary

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
- Currently: 973 lib tests (with all features including web), 912 without web

### Binary Tests
- Run with: `cargo test --bin factbase`
- No external dependencies required
- Currently: 354 bin tests (with all features including web), 347 without web

### Integration Tests (Require inference backend)
- Run with: `cargo test -- --ignored`
- Requires: Bedrock access or Ollama running with qwen3-embedding:0.6b and rnj-1-extended
- Currently: 73+ tests

### Frontend Tests (web feature)
- Run with: `cd web && npm test`
- Uses Vitest with jsdom environment
- Currently: 56 tests

### Total: 1327 unit/binary tests (with all features) + 73+ integration tests + 56 frontend tests

## Codebase Structure

The codebase has been modularized into focused submodules. See `.kiro/steering/module-interactions.md` for the complete file structure.

### Key Modules
| Module | Submodules |
|--------|------------|
| `config/` | database, embedding, processor, server, web, validation |
| `models/` | document, repository, link, search, scan, stats, temporal, question |
| `database/` | schema, documents, repositories, links, embeddings, search/, stats/ |
| `processor/` | core, temporal/, sources, review, chunks, stats |
| `llm/` | ollama, link_detector, review |
| `scanner/` | options, progress, orchestration/ |
| `organize/` | types, extract, links, orphans, review, audit, snapshot, verify, detect/, plan/, execute/ |
| `question_generator/` | temporal, conflict, missing, ambiguous, stale, duplicate, fields, facts, cross_validate |
| `commands/` | scan/, search/, grep/, status/, lint/, review/, export/, import/, doctor/, organize/, mcp |
| `mcp/` | protocol, stdio, server, tools/ |
| `mcp/tools/` | schema, helpers, search, entity, document, review/ |
| `web/` (feature-gated) | server, assets, api/ |
