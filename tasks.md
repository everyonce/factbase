# Factbase Tasks

## Phases

- [ ] [Phase 1: Core Infrastructure](#phase-1-core-infrastructure)
- [ ] [Phase 2: Embedding & Search](#phase-2-embedding--search)
- [ ] [Phase 3: File Watching & MCP Server](#phase-3-file-watching--mcp-server)
- [ ] [Phase 4: Multi-Repo & Polish](#phase-4-multi-repo--polish)

---

## Phase 1: Core Infrastructure

**Goal:** Basic file scanning and database storage

**Context:**
- Set up Rust project with all dependencies from FACTBASE_PLAN.md
- Documents are freeform markdown with a `<!-- factbase:XXXXXX -->` header (6-char hex ID)
- Title extracted from first H1 (`# Title`) or filename if no H1
- Type derived from parent folder name (e.g., `people/` → "person")
- Single repository support only in this phase (multi-repo comes in Phase 4)
- No embeddings yet - just store documents in SQLite
- Use Arc<Mutex<Connection>> for database from the start (thread safety for Phase 3)
- Global config at ~/.config/factbase/config.yaml
- Comprehensive unit tests for all modules
- Integration tests with test fixtures and temp directories

**Tasks:** [tasks/phase1.md](tasks/phase1.md) (17 tasks)

**Outcomes:**
<!-- Agent notes for future phases go here -->

---

## Phase 2: Embedding & Search

**Goal:** Semantic search capability with LLM-powered link detection

**Context:**
- Use Ollama for embeddings (nomic-embed-text, 768 dimensions)
- Use Ollama for LLM link detection (rnj-1 model)
- Fatal exit on Ollama errors - user must ensure Ollama is running
- Use sqlite-vec for vector storage and cosine similarity search
- TWO-PHASE SCAN: First pass indexes documents + embeddings, second pass uses LLM to detect entity mentions in ALL documents
- LLM matches entity names against known document titles to create links
- Manual [[id]] links are also preserved
- Modular provider traits allow swapping Ollama for other providers later
- Integration tests require live Ollama instance (marked #[ignore] by default)
- Performance tests for search latency and scan throughput

**Tasks:** [tasks/phase2.md](tasks/phase2.md) (14 tasks)

**Outcomes:**
<!-- Agent notes for future phases go here -->

---

## Phase 3: File Watching & MCP Server

**Goal:** Live updates and agent access via MCP

**Context:**
- Use notify crate with 500ms debounce window
- Any file change triggers full rescan (simpler than tracking individual events)
- MCP server: Streamable HTTP on localhost:3000, no auth
- Four MCP tools: search_knowledge, get_entity, list_entities, get_perspective
- `factbase serve` combines file watching + MCP server
- Integration tests for file watcher with real filesystem events
- Integration tests for MCP server with real HTTP requests
- End-to-end tests simulating AI agent workflows

**Tasks:** [tasks/phase3.md](tasks/phase3.md) (13 tasks)

**Outcomes:**
<!-- Agent notes for future phases go here -->

---

## Phase 4: Multi-Repo & Polish

**Goal:** Production-ready system with multi-repository support

**Context:**
- Support multiple repositories per factbase instance
- Each repo has its own perspective.yaml and namespace isolation via repo_id
- Single database stores all repos
- Add repo management CLI commands
- Focus on error handling, logging, documentation
- Comprehensive E2E tests for complete workflows
- Performance tests with 1000+ documents
- Stress tests for concurrent requests and rapid file changes
- CI/CD setup with GitHub Actions

**Tasks:** [tasks/phase4.md](tasks/phase4.md) (15 tasks)

**Outcomes:**
<!-- Agent notes for future phases go here -->
