# Module Interactions & Dependencies

## Module Dependency Graph

```
                    ┌─────────┐
                    │  main   │
                    │  (CLI)  │
                    └────┬────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
         ▼               ▼               ▼
    ┌─────────┐    ┌──────────┐    ┌─────────┐
    │ scanner │    │   mcp    │    │ watcher │
    └────┬────┘    │  server  │    └────┬────┘
         │         └────┬─────┘         │
         │              │               │
         ▼              │               │
    ┌───────────┐       │               │
    │ processor │◄──────┴───────────────┘
    └─────┬─────┘              triggers rescan
          │
    ┌─────┴─────┬─────────────┐
    │           │             │
    ▼           ▼             ▼
┌─────────┐ ┌───────┐   ┌──────────┐
│embedding│ │  llm  │   │ database │
└─────────┘ └───────┘   └──────────┘
    │           │             │
    └─────────┬─┴─────────────┘
              │
              ▼
         ┌─────────┐
         │ models  │
         └─────────┘
```

## File Structure

```
src/
├── main.rs              # CLI entry point, clap setup
├── lib.rs               # Module declarations, re-exports
├── error.rs             # Error types (FactbaseError)
├── ollama.rs            # Shared Ollama HTTP client
├── bedrock.rs           # Amazon Bedrock provider (feature-gated)
├── embedding.rs         # EmbeddingProvider trait + Ollama impl
├── watcher.rs           # File system monitoring
├── cache.rs             # LRU caching utilities
├── output.rs            # Output formatting helpers
├── shutdown.rs          # Graceful shutdown coordination
├── patterns.rs          # Regex patterns for parsing
├── progress.rs          # ProgressReporter enum (Cli/Mcp/Silent), ProgressSender type alias
│
├── config/              # Configuration (modular)
│   ├── mod.rs           # Config struct, loading, defaults
│   ├── database.rs      # DatabaseConfig
│   ├── embedding.rs     # EmbeddingConfig
│   ├── processor.rs     # ProcessorConfig
│   ├── server.rs        # ServerConfig
│   ├── web.rs           # WebConfig
│   └── validation.rs    # Config validation
│
├── models/              # Data structures (modular)
│   ├── mod.rs           # Re-exports
│   ├── document.rs      # Document struct
│   ├── repository.rs    # Repository struct
│   ├── link.rs          # Link, DetectedLink
│   ├── search.rs        # SearchResult
│   ├── scan.rs          # ScanResult, ScanStats
│   ├── stats.rs         # RepoStats, DetailedStats
│   ├── temporal.rs      # TemporalTag types
│   └── question.rs      # QuestionType, ReviewQuestion
│
├── database/            # SQLite operations (modular)
│   ├── mod.rs           # Database struct, constructors, transactions
│   ├── compression.rs   # Compression/decompression helpers (zstd, base64)
│   ├── schema.rs        # Schema init, migrations
│   ├── documents/       # Document operations
│   │   ├── mod.rs       # Shared: DOCUMENT_COLUMNS, row_to_document
│   │   ├── crud.rs      # Upsert, get, update, delete
│   │   ├── list.rs      # List, filter, get_documents_for_repo
│   │   └── batch.rs     # Cross-check hashes, backfill word counts
│   ├── repositories.rs  # Repository CRUD
│   ├── links.rs         # Link operations
│   ├── embeddings.rs    # Embedding storage/retrieval
│   ├── search/          # Search operations
│   │   ├── mod.rs
│   │   ├── semantic.rs  # Vector similarity search
│   │   ├── title.rs     # Title search
│   │   └── content.rs   # Content/grep search
│   └── stats/           # Statistics
│       ├── mod.rs
│       ├── basic.rs     # Basic stats
│       ├── detailed.rs  # Detailed stats
│       ├── temporal.rs  # Temporal stats
│       ├── sources.rs   # Source stats
│       ├── compression.rs # Compression stats
│       └── cache.rs     # Stats caching
│
├── llm/                 # LLM operations (modular)
│   ├── mod.rs           # LlmProvider trait
│   ├── ollama.rs        # OllamaLlm impl
│   ├── link_detector.rs # Entity link detection
│   └── review.rs        # Review LLM operations
│
├── scanner/             # File scanning (modular)
│   ├── mod.rs           # Scanner struct, file discovery
│   ├── options.rs       # ScanOptions
│   ├── progress.rs      # Progress bar handling
│   └── orchestration/   # Scan orchestration
│       ├── mod.rs       # full_scan, run_scan
│       ├── types.rs     # Internal types
│       ├── preread.rs   # File pre-reading
│       ├── embedding.rs # Embedding pass
│       ├── links.rs     # Link detection pass
│       ├── duplicates.rs # Duplicate detection
│       └── results.rs   # Result aggregation
│
├── processor/           # Document processing (modular)
│   ├── mod.rs           # Re-exports
│   ├── core.rs          # DocumentProcessor, ID/title/type
│   ├── chunks.rs        # Document chunking
│   ├── stats.rs         # Fact statistics
│   ├── sources.rs       # Source footnote parsing
│   ├── review.rs        # Review queue parsing
│   └── temporal/        # Temporal tag parsing
│       ├── mod.rs
│       ├── date.rs      # Date parsing
│       ├── parser.rs    # Tag parser
│       ├── range.rs     # Date range handling
│       └── validation.rs # Tag validation
│
├── question_generator/  # Review question generation
│   ├── mod.rs           # Main generator
│   ├── temporal.rs      # Temporal questions
│   ├── conflict.rs      # Conflict questions
│   ├── missing.rs       # Missing source questions
│   ├── ambiguous.rs     # Ambiguous fact questions
│   ├── stale.rs         # Stale fact questions
│   ├── duplicate.rs     # Duplicate questions
│   ├── corruption.rs    # Corruption questions
│   ├── precision.rs     # Precision questions
│   ├── placement.rs     # Placement questions
│   ├── fields.rs        # Field extraction
│   ├── facts.rs         # Fact extraction for cross-validation
│   ├── cross_validate.rs # Cross-document fact validation
│   └── check.rs          # Shared check-all-documents loop (MCP + CLI)
│
├── answer_processor/    # Review answer processing
│   ├── mod.rs           # Main processor
│   ├── interpret.rs     # Answer interpretation
│   ├── apply.rs         # Apply changes
│   ├── validate.rs      # Output validation before writing
│   └── temporal.rs      # Temporal answer handling
│
├── organize/            # Knowledge base reorganization (Phase 10)
│   ├── mod.rs           # Re-exports
│   ├── types.rs         # TrackedFact, FactLedger, FactDestination
│   ├── extract.rs       # Fact extraction from markdown
│   ├── links.rs         # Link redirection utilities
│   ├── orphans.rs       # Orphan document creation
│   ├── review.rs        # Orphan review integration
│   ├── audit.rs         # Audit logging for reorganization operations
│   ├── fs_helpers.rs    # write_file/remove_file with descriptive errors
│   ├── detect/          # Detection algorithms
│   │   ├── mod.rs
│   │   ├── merge.rs     # Merge candidate detection
│   │   ├── split.rs     # Split candidate detection
│   │   ├── misplaced.rs # Misplaced document detection
│   │   ├── entity_entries.rs  # Entity entry extraction from documents
│   │   └── duplicate_entries.rs # Cross-document duplicate entry detection
│   ├── plan/            # Planning operations
│   │   ├── mod.rs
│   │   ├── merge.rs     # Merge planning with LLM
│   │   └── split.rs     # Split planning with LLM
│   └── execute/         # Execution operations
│       ├── mod.rs
│       ├── merge.rs     # Merge execution with verification
│       ├── split.rs     # Split execution with verification
│       ├── move.rs      # Move document execution
│       └── retype.rs    # Retype document execution
│
├── commands/            # CLI commands (modular)
│   ├── mod.rs           # Command routing
│   ├── init.rs          # factbase init
│   ├── serve.rs         # factbase serve
│   ├── repo.rs          # factbase repo
│   ├── db.rs            # factbase db
│   ├── doctor/          # factbase doctor
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── checks.rs
│   │   └── fix.rs
│   ├── completions.rs   # factbase completions
│   ├── version.rs       # factbase version
│   ├── links.rs         # Link utilities
│   ├── show.rs          # Display helpers
│   ├── filters.rs       # Search filter parsing
│   ├── watch_helper.rs  # Watch mode helper
│   ├── utils.rs         # Command utilities
│   ├── setup.rs         # Setup helpers
│   ├── paths.rs         # Path utilities
│   ├── errors.rs        # Command error handling
│   ├── scan/            # factbase scan
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── verify.rs
│   │   ├── prune.rs
│   │   └── stats.rs
│   ├── search/          # factbase search
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── output.rs
│   │   ├── filters.rs
│   │   └── watch.rs
│   ├── grep/            # factbase grep
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── execute.rs
│   │   └── output.rs
│   ├── status/          # factbase status
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── display.rs
│   │   └── detailed.rs
│   ├── stats.rs         # factbase stats
│   ├── check/            # factbase check
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── checks.rs       # Unified content checks (basics, temporal, sources)
│   │   ├── output.rs
│   │   ├── review.rs
│   │   ├── execute/         # Link checks, review generation, aggregation
│   │   │   ├── mod.rs
│   │   │   ├── links.rs
│   │   │   ├── review.rs
│   │   │   └── aggregate.rs
│   │   ├── incremental.rs
│   │   └── watch.rs
│   ├── review/          # factbase review
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── apply.rs
│   │   ├── status.rs
│   │   └── import.rs
│   ├── export/          # factbase export
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── markdown.rs
│   │   ├── json.rs
│   │   └── archive.rs
│   ├── import/          # factbase import
│   │   ├── mod.rs
│   │   ├── args.rs
│   │   ├── formats.rs
│   │   └── validate.rs
│   └── organize/        # factbase organize
│       ├── mod.rs
│       └── args.rs
│
└── mcp/                 # MCP server
    ├── mod.rs
    ├── protocol.rs      # Shared MCP protocol types (initialize_result)
    ├── stdio.rs         # Stdio transport (stdin/stdout JSON-RPC)
    ├── server.rs        # HTTP server (axum)
    └── tools/
        ├── mod.rs       # Tool routing (21 tools)
        ├── schema.rs    # Tool schemas
        ├── helpers.rs   # Tool helpers
        ├── search.rs    # search_knowledge, search_content, search_temporal
        ├── entity.rs    # get_entity, list_entities, etc.
        ├── document.rs  # CRUD operations
        ├── workflow.rs  # Guided workflow tools
        └── review/      # Review tools

# Feature-gated modules (web feature)
└── web/                 # Web UI server (Phase 11)
    ├── mod.rs           # Module entry point, re-exports
    ├── server.rs        # Axum server on configurable port (default 3001)
    ├── assets.rs        # Static asset serving via rust-embed
    └── api/             # JSON API endpoints (17 endpoints total)
        ├── mod.rs       # API router, re-exports
        ├── errors.rs    # ApiError, handle_error shared error types
        ├── review.rs    # Review queue endpoints (5 endpoints)
        ├── organize.rs  # Organize suggestion endpoints (6 endpoints)
        ├── documents.rs # Document context endpoints (3 endpoints)
        └── stats.rs     # Stats endpoints (3 endpoints)

# Frontend (web/src/, compiled to web/dist/)
web/
├── package.json         # Vite 5.x, TypeScript 5.x, Tailwind 3.x
├── tsconfig.json        # Strict TypeScript config
├── tailwind.config.js   # Dark mode via media query
├── vite.config.ts       # Output to web/dist/
├── index.html           # Vite entry point
└── src/
    ├── main.ts          # App shell, routing, lifecycle
    ├── style.css        # Tailwind imports, animations, a11y
    ├── router.ts        # Hash-based navigation
    ├── api.ts           # Typed API client (17 endpoints)
    ├── keyboard.ts      # Keyboard navigation (j/k, Enter, Escape, ?)
    ├── pages/
    │   ├── Dashboard.ts # Stats cards, auto-refresh
    │   ├── ReviewQueue.ts # Question list with filters, bulk mode
    │   ├── OrganizeSuggestions.ts # Merge/misplaced suggestions, filters
    │   └── Orphans.ts   # Orphan assignment, repo selector, bulk mode
    └── components/
        ├── QuestionCard.ts # Question type badges, card rendering
        ├── AnswerForm.ts   # Inline answer form, quick actions
        ├── BulkActions.ts  # Checkbox selection, bulk dismiss/answer
        ├── DocumentPreview.ts # Slide-in panel, line highlighting
        ├── SuggestionCard.ts # Merge/misplaced cards, approve/dismiss
        ├── MergePreview.ts # Side-by-side comparison, fact counts
        ├── SplitPreview.ts # Section preview, fact counts
        ├── OrphanCard.ts   # Orphan display, assignment form
        ├── Loading.ts      # Spinner, skeleton loaders
        ├── Error.ts        # Error display, retry button
        └── Toast.ts        # Toast notifications, auto-dismiss
```

## Module Responsibilities

### `config.rs`
- Loads `~/.config/factbase/config.yaml`
- Provides defaults when config missing
- Used by: main, scanner, processor, embedding, llm, mcp, watcher

### `error.rs`
- Defines `FactbaseError` enum
- Implements `From` traits for error conversion
- Used by: all modules

### `models.rs`
- Data structures: `Document`, `Repository`, `ScanResult`, `Link`, `DetectedLink`, `SearchResult`
- Serde derives for serialization
- Used by: all modules

### `database/` (modular)
- SQLite connection management with `r2d2` pool
- Schema initialization (documents, embeddings, links, repositories, FTS5 content index)
- CRUD operations for all tables
- Vector search via sqlite-vec
- Full-text search via FTS5 (`document_content_fts` virtual table)
- Split into 8 submodules: mod, schema, documents, repositories, links, embeddings, search, stats
- **Performance optimizations:**
  - Batch link fetching via `get_links_for_documents()` (eliminates N+1 queries)
  - Prepared statement caching via `prepare_cached()` on hot paths
  - Index on `file_modified_at` for `--since` filter queries
  - Pre-computed `word_count` column to avoid content decompression in stats
- Used by: scanner, processor, mcp

### `ollama.rs`
- Shared Ollama HTTP client
- Connection pooling and retry logic
- Used by: embedding, llm (when using Ollama provider)

### `bedrock.rs` (feature-gated)
- Amazon Bedrock provider implementations
- `BedrockEmbedding`: Titan and Nova embedding models via InvokeModel
- `BedrockLlm`: Any chat model via Converse API
- Used by: setup.rs (when provider = "bedrock")

### `embedding.rs`
- `EmbeddingProvider` trait (async, Send + Sync)
- `OllamaEmbedding` implementation
- Used by: processor, mcp (for search queries)

### `llm/`
- `LlmProvider` trait (async, Send + Sync)
- `OllamaLlm` implementation
- `LinkDetector` service for entity detection
- Used by: scanner (link pass)

### `scanner.rs`
- Finds `.md` files in repository
- Applies ignore patterns (glob matching)
- Orchestrates two-phase scan
- Used by: main (scan command), watcher

### `processor/`
- `core.rs`: Extracts/injects factbase ID headers, parses title, derives type
- `temporal.rs`: Parses `@t[...]` temporal tags
- `sources.rs`: Parses source footnotes `[^n]`
- `review.rs`: Parses review queue sections
- `chunks.rs`: Splits long documents for embedding
- `stats.rs`: Collects processing statistics
- Used by: scanner

### `watcher.rs`
- File system monitoring via `notify` crate
- Debouncing (500ms window)
- Triggers scanner on file changes
- Used by: main (serve command)

### `cache.rs`
- LRU cache implementations for embeddings and metadata
- Used by: embedding, database

### `shutdown.rs`
- Graceful shutdown coordination
- Signal handling (Ctrl+C)
- Used by: main, serve, watcher

### `progress.rs`
- `ProgressReporter` enum: `Cli { quiet }`, `Mcp { sender }`, `Silent`
- Three methods: `report(current, total, message)`, `phase(name)`, `log(message)`
- CLI variant writes to stderr; MCP variant sends JSON via unbounded channel + eprintln; Silent is no-op
- `ProgressSender` type alias (`UnboundedSender<Value>`) for MCP channel
- Used by: scanner, check, review apply, search_content, organize, export/import, bulk MCP operations

### `question_generator.rs`
- Generates review questions for documents
- Analyzes temporal coverage, source citations
- Used by: check, MCP generate_questions

### `answer_processor.rs`
- Processes answered review questions
- Updates documents with temporal tags, sources
- Used by: review --apply, MCP answer_question

### `mcp/server.rs`
- HTTP server via `axum`
- Binds to localhost:3000
- Routes MCP tool calls
- Used by: main (serve command)

### `mcp/tools/`
- `search.rs`: search_knowledge, search_content, search_temporal
- `entity.rs`: get_entity, list_entities, get_perspective, list_repositories, get_document_stats
- `document.rs`: create_document, update_document, delete_document, bulk_create_documents
- `review.rs`: get_review_queue, answer_question, bulk_answer_questions, generate_questions
- Used by: mcp/server

## Key Interfaces Between Modules

### Scanner ↔ Processor
```rust
// Scanner calls processor for each file
processor.process_file(repo_id, path) -> Result<Document>

// Processor needs embedding provider
processor.new(embedding: Box<dyn EmbeddingProvider>, db: Database)
```

### Scanner ↔ LinkDetector
```rust
// Scanner calls link detector in second pass
link_detector.detect_links(content, source_id) -> Result<Vec<DetectedLink>>

// LinkDetector needs LLM provider and database
link_detector.new(llm: Box<dyn LlmProvider>, db: Database)
```

### MCP Server ↔ Database
```rust
// Search tool generates embedding then queries
let embedding = embedding_provider.generate(query).await?;
let results = db.search_semantic(&embedding, limit)?;

// Entity tool queries by ID
let doc = db.get_document(id)?;
let links_to = db.get_links_from(id)?;
let linked_from = db.get_links_to(id)?;
```

### Watcher ↔ Scanner
```rust
// Watcher triggers rescan on file changes
// Callback pattern or channel-based communication
watcher.on_change(|paths| {
    let repo = find_repo_for_path(paths)?;
    scanner.full_scan(repo, db).await?;
});
```

## Async Boundaries

- **Embedding generation**: async (HTTP to inference backend)
- **LLM completion**: async (HTTP to inference backend)
- **Database operations**: sync (rusqlite is sync)
- **File I/O**: sync (std::fs)
- **MCP server**: async (axum)
- **File watcher**: runs in background thread, sends events via channel

## Shared State

The `Database` struct wraps an `r2d2::Pool<SqliteConnectionManager>` to enable:
- Watcher thread triggering scans
- MCP server handling concurrent requests
- Safe sharing without data races
- Configurable pool size (1-32 connections)

## Error Propagation

- Most functions return `Result<T, FactbaseError>`
- Use `?` operator for propagation
- Inference errors trigger `process::exit(1)` with clear message
- Database errors propagate up to caller
- File I/O errors logged and skipped (don't fail entire scan)
