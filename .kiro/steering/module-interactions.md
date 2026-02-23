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

### `database.rs`
- SQLite connection management with `Arc<Mutex<Connection>>`
- Schema initialization (documents, embeddings, links, repositories)
- CRUD operations for all tables
- Vector search via sqlite-vec
- Used by: scanner, processor, mcp

### `embedding.rs`
- `EmbeddingProvider` trait (async, Send + Sync)
- `OllamaEmbedding` implementation
- Calls Ollama `/api/embeddings` endpoint
- Returns 768-dimension vectors
- Used by: processor, mcp (for search queries)

### `llm.rs`
- `LlmProvider` trait (async, Send + Sync)
- `OllamaLlm` implementation
- `LinkDetector` service for entity detection
- Calls Ollama `/api/generate` endpoint
- Used by: scanner (link pass)

### `scanner.rs`
- Finds `.md` files in repository
- Applies ignore patterns (glob matching)
- Orchestrates two-phase scan
- Used by: main (scan command), watcher

### `processor.rs`
- Extracts/injects factbase ID headers
- Parses title from H1 or filename
- Derives type from folder name
- Generates embeddings during scan
- Used by: scanner

### `watcher.rs`
- File system monitoring via `notify` crate
- Debouncing (500ms window)
- Triggers scanner on file changes
- Used by: main (serve command)

### `mcp/server.rs`
- HTTP server via `axum`
- Binds to localhost:3000
- Routes MCP tool calls
- Used by: main (serve command)

### `mcp/tools.rs`
- Tool implementations: search_knowledge, get_entity, list_entities, get_perspective
- Calls database and embedding service
- Formats MCP responses
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

- **Embedding generation**: async (HTTP to Ollama)
- **LLM completion**: async (HTTP to Ollama)
- **Database operations**: sync (rusqlite is sync)
- **File I/O**: sync (std::fs)
- **MCP server**: async (axum)
- **File watcher**: runs in background thread, sends events via channel

## Shared State

The `Database` struct wraps `Arc<Mutex<Connection>>` to enable:
- Watcher thread triggering scans
- MCP server handling concurrent requests
- Safe sharing without data races

## Error Propagation

- Most functions return `Result<T, FactbaseError>`
- Use `?` operator for propagation
- Ollama errors trigger `process::exit(1)` with clear message
- Database errors propagate up to caller
- File I/O errors logged and skipped (don't fail entire scan)
