# Factbase Project Plan

**Version:** 2.0  
**Date:** 2026-01-24  
**Status:** Design Complete

---

## Executive Summary

Factbase is a filesystem-based knowledge management system that treats markdown files as the source of truth for organizational knowledge. Unlike fact-graph (which extracts facts from messages with human approval), factbase monitors a directory of human-curated markdown files, maintains a real-time vector search index, and provides read-only MCP access for AI agents.

**Core Principle:** The filesystem is the source of truth. Whatever writes to these files is trusted.

---

## System Architecture

### High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                    Markdown File Repository                  │
│  /people/john-doe.md, /projects/platform-mod.md, etc.       │
└────────────────┬────────────────────────────────────────────┘
                 │ (File System Events)
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              File Watcher (notify-rs)                        │
│  Monitors directory tree for create/modify/delete            │
│  Triggers full rescan on any change                          │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│           Document Processor Pipeline                        │
│  1. Extract/inject factbase ID header                        │
│  2. Parse title (first H1 or filename)                       │
│  3. Derive type from folder                                  │
│  4. Detect cross-reference links [[id]]                      │
│  5. Generate embedding (AWS Bedrock)                         │
│  6. Update SQLite index                                      │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│         SQLite Database (with sqlite-vec)                    │
│  - repositories (multi-repo support)                         │
│  - documents (id, title, type, content)                      │
│  - document_embeddings (vector search)                       │
│  - document_links (cross-references)                         │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│              MCP Server (Read-Only)                          │
│  Tools: search_knowledge, get_entity, list_entities          │
│  Transport: Streamable HTTP (localhost:3000)                 │
└─────────────────────────────────────────────────────────────┘
```

---

## Data Model

### Markdown File Structure

Each file represents a single entity (person, project, company, concept, etc.). Files are **freeform markdown** - no required structure beyond the factbase header (injected automatically on first scan).

**Required header (injected by factbase):**
```markdown
<!-- factbase:a1cb2b -->
```

**Example: `/people/john-doe.md`**

```markdown
<!-- factbase:a1cb2b -->
# John Doe

John is a Senior Backend Engineer at Acme Corp, specializing in Python microservices.

## Background
- Joined Acme Corp in 2024
- Previously worked at TechStart Inc for 3 years
- Expert in distributed systems and API design

## Current Projects
- Leading the Platform Modernization [[c7f3e2]] initiative
- Mentoring 2 junior engineers

## Contact
- Email: <email>
- Slack: @johndoe
```

**Example: `/projects/platform-modernization.md`**

```markdown
<!-- factbase:c7f3e2 -->
# Platform Modernization Project

Migration of legacy monolith to microservices architecture.

## Objectives
- Reduce deployment time from 2 hours to 15 minutes
- Improve system reliability to 99.9% uptime
- Enable independent team deployments

## Timeline
- Phase 1 (Q4 2025): Service decomposition - COMPLETE
- Phase 2 (Q1 2026): Data migration - IN PROGRESS
- Phase 3 (Q2 2026): Legacy system decommission

## Team
- John Doe [[a1cb2b]] (Lead)
- Sarah Chen [[d4e5f6]] (DevOps)
- Mike Johnson [[b2c3d4]] (Backend)
```

### Parsing Rules

| Field | Source | Fallback |
|-------|--------|----------|
| ID | `<!-- factbase:XXXXXX -->` header | Generated on first scan |
| Title | First H1 (`# Title`) | Filename without extension |
| Type | Parent folder name (`people/` → person) | "document" |
| Content | Full file content | - |
| Links | `[[id]]` patterns in content | - |

### Cross-Reference Links

When factbase detects a known entity name in content, it can annotate it with the entity's ID:

```markdown
Working with John Doe [[a1cb2b]] on the migration.
```

This creates explicit, parseable links between documents. The `document_links` table tracks these relationships.

### Perspective Configuration

Each repository has a `perspective.yaml` at the root defining its context:

```yaml
perspective:
  type: salesperson
  organization: Acme Corp Sales Team
  focus: customer relationships, deals, opportunities
```

Alternative example:

```yaml
perspective:
  type: homeowner
  focus: home maintenance, projects, contractors
```

The perspective helps agents understand the context of the knowledge base.

---

## Database Schema

### SQLite Tables

```sql
-- Repositories table for multi-repo support
CREATE TABLE repositories (
    id TEXT PRIMARY KEY,              -- Short identifier (e.g., 'main', 'sales')
    name TEXT NOT NULL,               -- Human-readable name
    path TEXT UNIQUE NOT NULL,        -- Filesystem path
    perspective TEXT,                 -- JSON: perspective config from perspective.yaml
    created_at TIMESTAMP NOT NULL,
    last_indexed_at TIMESTAMP
);

-- Core documents table
CREATE TABLE documents (
    id TEXT PRIMARY KEY,              -- 6-char hex (e.g., 'a1cb2b')
    repo_id TEXT NOT NULL,            -- Repository identifier
    file_path TEXT NOT NULL,          -- Relative path from repo root
    file_hash TEXT NOT NULL,          -- SHA256 of file content
    
    -- Parsed from content
    title TEXT NOT NULL,              -- From first H1 or filename
    doc_type TEXT,                    -- From parent folder (people/ → person)
    content TEXT NOT NULL,            -- Full markdown content
    
    -- Timestamps
    file_modified_at TIMESTAMP,       -- File mtime
    indexed_at TIMESTAMP NOT NULL,    -- When we indexed it
    
    -- Status
    is_deleted BOOLEAN DEFAULT FALSE,
    
    UNIQUE(repo_id, file_path),
    FOREIGN KEY (repo_id) REFERENCES repositories(id)
);

-- Vector embeddings (sqlite-vec)
CREATE VIRTUAL TABLE document_embeddings USING vec0(
    document_id TEXT PRIMARY KEY,
    embedding FLOAT[768]              -- nomic-embed-text dimension
);

-- Cross-references between documents
CREATE TABLE document_links (
    source_id TEXT NOT NULL,          -- Document containing the reference
    target_id TEXT NOT NULL,          -- Document being referenced  
    context TEXT,                     -- Surrounding text snippet
    created_at TIMESTAMP NOT NULL,
    PRIMARY KEY (source_id, target_id),
    FOREIGN KEY (source_id) REFERENCES documents(id),
    FOREIGN KEY (target_id) REFERENCES documents(id)
);

-- Indexes
CREATE INDEX idx_documents_repo ON documents(repo_id);
CREATE INDEX idx_documents_type ON documents(doc_type);
CREATE INDEX idx_documents_title ON documents(title);
CREATE INDEX idx_documents_modified ON documents(file_modified_at DESC);
CREATE INDEX idx_documents_deleted ON documents(is_deleted);
CREATE INDEX idx_links_source ON document_links(source_id);
CREATE INDEX idx_links_target ON document_links(target_id);
```

### Schema Notes

- **No tags/metadata columns**: Freeform markdown means no structured frontmatter to parse
- **Type from folder**: `doc_type` derived from parent folder, not file content
- **Links table**: Populated during full rescan when `[[id]]` patterns detected
- **Soft deletes**: `is_deleted` flag preserves history; cleanup deferred

---

## Core Components

### 1. File Watcher (`src/watcher.rs`)

**Responsibilities:**
- Monitor repository directory for filesystem events
- Debounce rapid changes (e.g., editor auto-save)
- Trigger full rescan after debounce window

**Technology:** `notify` crate (cross-platform filesystem notifications)

**Key Features:**
- Recursive directory monitoring
- Event debouncing (500ms window)
- Ignore patterns (`.swp`, `.tmp`, `.git/`, etc.)
- Path normalization for cross-platform compatibility

**Behavior:**
- Any file change triggers a full rescan after debounce
- Full rescan detects: new files, modified files, deleted files, moved files (by ID)
- Simpler than tracking individual events; ensures consistency

### 2. Document Processor (`src/processor.rs`)

**Responsibilities:**
- Parse markdown files (extract ID, title)
- Inject factbase header on new files
- Detect cross-reference links
- Generate embeddings via AWS Bedrock
- Update SQLite database

**Pipeline Stages:**

```rust
pub struct DocumentProcessor {
    embedding_service: EmbeddingService,
    db: Database,
}

impl DocumentProcessor {
    /// Process a single file
    pub async fn process_file(&self, repo_id: &str, path: &Path) -> Result<Document> {
        // 1. Read file content
        let content = fs::read_to_string(path)?;
        
        // 2. Extract or generate ID
        let (id, content) = self.ensure_factbase_header(path, content)?;
        
        // 3. Parse title (first H1 or filename)
        let title = self.extract_title(&content, path);
        
        // 4. Derive type from parent folder
        let doc_type = self.derive_type(path);
        
        // 5. Detect [[id]] links
        let links = self.extract_links(&content);
        
        // 6. Generate embedding
        let embedding = self.embedding_service.generate(&content).await?;
        
        // 7. Store in database
        self.db.upsert_document(Document { id, repo_id, title, doc_type, content, ... })?;
        self.db.upsert_embedding(&id, &embedding)?;
        self.db.update_links(&id, &links)?;
        
        Ok(document)
    }
}
```

**Header Injection:**
```rust
fn ensure_factbase_header(&self, path: &Path, content: String) -> Result<(String, String)> {
    // Check for existing header: <!-- factbase:XXXXXX -->
    if let Some(id) = self.extract_existing_id(&content) {
        return Ok((id, content));
    }
    
    // Generate new 6-char hex ID
    let id = self.generate_id(); // e.g., "a1cb2b"
    
    // Prepend header to file
    let new_content = format!("<!-- factbase:{} -->\n{}", id, content);
    fs::write(path, &new_content)?;
    
    Ok((id, new_content))
}
```

**Title Extraction:**
```rust
fn extract_title(&self, content: &str, path: &Path) -> String {
    // Look for first H1: # Title
    for line in content.lines() {
        if line.starts_with("# ") {
            return line[2..].trim().to_string();
        }
    }
    // Fallback to filename
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}
```

**Link Detection:**
```rust
fn extract_links(&self, content: &str) -> Vec<String> {
    // Match [[id]] patterns
    let re = Regex::new(r"\[\[([a-f0-9]{6})\]\]").unwrap();
    re.captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect()
}
```

### 3. Scanner (`src/scanner.rs`)

**Responsibilities:**
- Full directory scan on startup and after changes
- Detect new, modified, deleted, and moved files
- Coordinate document processing

```rust
pub struct Scanner {
    processor: DocumentProcessor,
    db: Database,
}

impl Scanner {
    pub async fn full_scan(&self, repo: &Repository) -> Result<ScanResult> {
        let mut result = ScanResult::default();
        
        // ===== PASS 1: Document Processing =====
        // Index documents, generate embeddings
        
        // 1. Get all .md files in repo
        let files = self.find_markdown_files(&repo.path)?;
        
        // 2. Get all known documents from DB
        let known_docs = self.db.get_documents_for_repo(&repo.id)?;
        
        // 3. Process each file (ID, title, type, embedding)
        for file_path in &files {
            let content = fs::read_to_string(file_path)?;
            let hash = sha256(&content);
            
            // Check if file has factbase ID
            if let Some(id) = extract_id(&content) {
                // Known document - check if modified
                if let Some(doc) = known_docs.get(&id) {
                    if doc.file_hash != hash {
                        self.processor.process_file(&repo.id, file_path).await?;
                        result.updated += 1;
                    }
                    // Track that we've seen this ID
                    seen_ids.insert(id);
                } else {
                    // ID exists in file but not in DB (restored from backup?)
                    self.processor.process_file(&repo.id, file_path).await?;
                    result.added += 1;
                }
            } else {
                // New file - will get ID injected
                self.processor.process_file(&repo.id, file_path).await?;
                result.added += 1;
            }
        }
        
        // 4. Mark missing documents as deleted
        for (id, doc) in known_docs {
            if !seen_ids.contains(&id) {
                self.db.mark_deleted(&id)?;
                result.deleted += 1;
            }
        }
        
        // ===== PASS 2: Link Processing =====
        // Scan ALL documents for [[id]] links (not just changed ones)
        // This catches links TO newly created documents FROM existing docs
        
        let all_docs = self.db.get_documents_for_repo(&repo.id)?;
        for (id, doc) in all_docs {
            if doc.is_deleted { continue; }
            let links = self.processor.extract_links(&doc.content);
            let contexts = self.processor.extract_link_contexts(&doc.content, &links);
            self.db.update_links(&id, &links, &contexts)?;
        }
        
        Ok(result)
    }
}
```

### 4. Database Layer (`src/database.rs`)

**Responsibilities:**
- SQLite connection management
- CRUD operations for documents
- Vector search via sqlite-vec
- Transaction management

**Key Operations:**

```rust
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    // Document operations
    pub fn upsert_document(&self, doc: &Document) -> Result<()>;
    pub fn get_document(&self, id: &str) -> Result<Option<Document>>;
    pub fn get_documents_for_repo(&self, repo_id: &str) -> Result<HashMap<String, Document>>;
    pub fn mark_deleted(&self, id: &str) -> Result<()>;
    
    // Embedding operations
    pub fn upsert_embedding(&self, doc_id: &str, embedding: &[f32]) -> Result<()>;
    
    // Link operations
    pub fn update_links(&self, source_id: &str, target_ids: &[String], contexts: &[String]) -> Result<()>;
    pub fn get_links_from(&self, source_id: &str) -> Result<Vec<Link>>;
    pub fn get_links_to(&self, target_id: &str) -> Result<Vec<Link>>;
    
    // Search operations
    pub fn search_semantic(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;
    pub fn search_by_type(&self, doc_type: &str, limit: usize) -> Result<Vec<Document>>;
    
    // Repository operations
    pub fn get_repository(&self, id: &str) -> Result<Option<Repository>>;
    pub fn list_repositories(&self) -> Result<Vec<Repository>>;
}
```

**Vector Search Query:**

```sql
SELECT 
    d.id, d.title, d.doc_type, d.content, d.file_path,
    vec_distance_cosine(e.embedding, ?) as distance
FROM documents d
JOIN document_embeddings e ON d.id = e.document_id
WHERE d.is_deleted = FALSE
  AND d.repo_id = ?
ORDER BY distance ASC
LIMIT ?;
```

### 5. MCP Server (`src/mcp.rs`)

**Responsibilities:**
- Expose read-only MCP tools for agents
- Handle Streamable HTTP transport
- Provide semantic search and entity lookup

**Server Configuration:**
- Host: `127.0.0.1` (localhost only)
- Port: `3000` (configurable)
- No authentication required

**MCP Tools:**

#### `search_knowledge`
Search across all documents using semantic similarity.

```json
{
  "name": "search_knowledge",
  "description": "Search the knowledge base using semantic similarity",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Natural language search query"
      },
      "limit": {
        "type": "integer",
        "default": 10,
        "description": "Maximum number of results"
      },
      "type": {
        "type": "string",
        "description": "Optional: filter by document type (person, project, etc.)"
      },
      "repo": {
        "type": "string",
        "description": "Optional: filter by repository ID"
      }
    },
    "required": ["query"]
  }
}
```

**Response:**
```json
{
  "results": [
    {
      "id": "a1cb2b",
      "title": "John Doe",
      "type": "person",
      "file_path": "people/john-doe.md",
      "relevance_score": 0.92,
      "snippet": "John is a Senior Backend Engineer at Acme Corp..."
    }
  ]
}
```

#### `get_entity`
Retrieve a specific entity by ID or file path.

```json
{
  "name": "get_entity",
  "description": "Get full details of a specific entity",
  "inputSchema": {
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "description": "Document ID (6-char hex) or file path"
      }
    },
    "required": ["id"]
  }
}
```

**Response:**
```json
{
  "id": "a1cb2b",
  "title": "John Doe",
  "type": "person",
  "file_path": "people/john-doe.md",
  "content": "<!-- factbase:a1cb2b -->\n# John Doe\n\nJohn is a Senior...",
  "links_to": ["c7f3e2", "d4e5f6"],
  "linked_from": ["b2c3d4"],
  "indexed_at": "2026-01-23T15:30:00Z"
}
```

#### `list_entities`
List entities with optional filtering.

```json
{
  "name": "list_entities",
  "description": "List entities with optional filtering",
  "inputSchema": {
    "type": "object",
    "properties": {
      "type": {
        "type": "string",
        "description": "Filter by document type"
      },
      "repo": {
        "type": "string", 
        "description": "Filter by repository ID"
      },
      "limit": {
        "type": "integer",
        "default": 50
      }
    }
  }
}
```

#### `get_perspective`
Get the perspective configuration for a repository.

```json
{
  "name": "get_perspective",
  "description": "Get the perspective/context of a knowledge base",
  "inputSchema": {
    "type": "object",
    "properties": {
      "repo": {
        "type": "string",
        "description": "Repository ID (optional, defaults to first repo)"
      }
    }
  }
}
```

### 6. Embedding Service (`src/embedding.rs`)

**Responsibilities:**
- Generate embeddings via Ollama (nomic-embed-text)
- Modular provider trait for future swapping
- Fatal exit on persistent failures

```rust
/// Provider-agnostic embedding trait
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn generate(&self, text: &str) -> Result<Vec<f32>>;
    fn dimension(&self) -> usize;
}

/// Ollama embedding provider
pub struct OllamaEmbedding {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaEmbedding {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let response = self.client
            .post(format!("{}/api/embeddings", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": text
            }))
            .send()
            .await?;
            
        if !response.status().is_success() {
            eprintln!("FATAL: Ollama embedding error: {}", response.status());
            eprintln!("Ensure Ollama is running: ollama serve");
            std::process::exit(1);
        }
        
        let body: Value = response.json().await?;
        let embedding = body["embedding"]
            .as_array()
            .ok_or_else(|| anyhow!("No embedding in response"))?
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        Ok(embedding)
    }
    
    fn dimension(&self) -> usize { 768 }
}
```

**Configuration:**
- Provider: Ollama
- Model: nomic-embed-text
- Dimension: 768
- Base URL: http://localhost:11434

### 7. LLM Service (`src/llm.rs`)

**Responsibilities:**
- Detect entity mentions in document content
- Match mentions to existing documents
- Modular provider trait for future swapping

```rust
/// Provider-agnostic LLM trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

/// Ollama LLM provider
pub struct OllamaLlm {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[async_trait]
impl LlmProvider for OllamaLlm {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let response = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false
            }))
            .send()
            .await?;
            
        if !response.status().is_success() {
            eprintln!("FATAL: Ollama LLM error: {}", response.status());
            eprintln!("Ensure Ollama is running with model: ollama run {}", self.model);
            std::process::exit(1);
        }
        
        let body: Value = response.json().await?;
        Ok(body["response"].as_str().unwrap_or("").to_string())
    }
}

/// Link detection service using LLM
pub struct LinkDetector {
    llm: Box<dyn LlmProvider>,
    db: Database,
}

impl LinkDetector {
    /// Detect entity mentions and match to existing documents
    pub async fn detect_links(&self, content: &str, source_id: &str) -> Result<Vec<DetectedLink>> {
        // Get all document titles for matching
        let docs = self.db.get_all_document_titles()?;
        
        let prompt = format!(
            "Given this document content, identify any mentions of these known entities.\n\n\
            Known entities:\n{}\n\n\
            Document content:\n{}\n\n\
            Return JSON array of matches: [{{\"entity\": \"name\", \"context\": \"surrounding text\"}}]\n\
            Only return exact or very close matches. Return [] if no matches.",
            docs.iter().map(|(id, title)| format!("- {} ({})", title, id)).collect::<Vec<_>>().join("\n"),
            content
        );
        
        let response = self.llm.complete(&prompt).await?;
        // Parse JSON response and map to document IDs
        // ...
    }
}
```

**Configuration:**
- Provider: Ollama  
- Model: rnj-1
- Base URL: http://localhost:11434

---

## Configuration

### Global Configuration

Factbase uses a global configuration file at `~/.config/factbase/config.yaml`.

### `config.yaml`

```yaml
# Repositories (multi-repo support)
repositories:
  - id: "main"
    name: "Main Knowledge Base"
    path: "./knowledge-base"
  - id: "sales"
    name: "Sales Team KB"
    path: "/shared/sales-kb"

# File watcher settings
watcher:
  debounce_ms: 500
  ignore_patterns:
    - "*.swp"
    - "*.tmp"
    - "*~"
    - ".git/**"
    - ".DS_Store"
    - ".factbase/**"

# Database settings
database:
  path: "./.factbase/factbase.db"
  
# Embedding settings
embedding:
  provider: "ollama"
  base_url: "http://localhost:11434"
  model: "nomic-embed-text"
  dimension: 768

# LLM settings (for link detection)
llm:
  provider: "ollama"
  base_url: "http://localhost:11434"
  model: "rnj-1"

# MCP server settings
mcp:
  host: "127.0.0.1"
  port: 3000

# Processing settings
processor:
  max_file_size: 100000         # bytes, skip files larger than this
  snippet_length: 200           # characters for search result snippets
```

### Environment Variables

Configuration can be overridden via environment:

```bash
FACTBASE_DB_PATH=./custom.db
FACTBASE_EMBEDDING_REGION=us-west-2
FACTBASE_EMBEDDING_PROFILE=prod
FACTBASE_MCP_PORT=8080
```

---

## Project Structure

```
factbase/
├── Cargo.toml
├── config.yaml
├── README.md
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Configuration loading
│   ├── watcher.rs           # File system monitoring
│   ├── scanner.rs           # Full directory scanning
│   ├── processor.rs         # Document processing (ID, title, links)
│   ├── database.rs          # SQLite operations
│   ├── embedding.rs         # Embedding provider trait + Ollama impl
│   ├── llm.rs               # LLM provider trait + Ollama impl + LinkDetector
│   ├── mcp/
│   │   ├── mod.rs           # MCP module
│   │   ├── server.rs        # HTTP server setup
│   │   └── tools.rs         # Tool implementations
│   ├── models.rs            # Data structures
│   └── error.rs             # Error types
├── tests/
│   ├── scanner_test.rs
│   ├── processor_test.rs
│   └── search_test.rs
└── examples/
    └── sample-knowledge-base/
        ├── perspective.yaml
        ├── people/
        │   └── john-doe.md
        └── projects/
            └── platform-modernization.md
```

---

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)
**Goal:** Basic file scanning and database storage

- [ ] Project setup (Cargo.toml, dependencies)
- [ ] Configuration loading
- [ ] Database schema and initialization
- [ ] Scanner: find all .md files
- [ ] Processor: extract/inject ID header
- [ ] Processor: parse title (H1 or filename)
- [ ] Processor: derive type from folder
- [ ] Document storage in SQLite
- [ ] CLI: `factbase init`, `factbase scan`

**Deliverable:** Can scan a directory, inject IDs, and store documents in SQLite

### Phase 2: Embedding & Search (Week 2)
**Goal:** Semantic search capability

- [ ] AWS Bedrock integration
- [ ] Embedding generation (fatal on error)
- [ ] sqlite-vec integration
- [ ] Vector search implementation
- [ ] Link detection (`[[id]]` patterns)
- [ ] Link storage in document_links table
- [ ] CLI: `factbase search <query>`

**Deliverable:** Can search documents semantically, links tracked

### Phase 3: File Watching & MCP Server (Week 3)
**Goal:** Live updates and agent access

- [ ] File watcher with debouncing
- [ ] Trigger full rescan on changes
- [ ] MCP server (Streamable HTTP)
- [ ] Tool: `search_knowledge`
- [ ] Tool: `get_entity`
- [ ] Tool: `list_entities`
- [ ] Tool: `get_perspective`
- [ ] CLI: `factbase serve`

**Deliverable:** Agents can query knowledge base via MCP, live file updates

### Phase 4: Multi-Repo & Polish (Week 4)
**Goal:** Production-ready system

- [ ] Multi-repository support
- [ ] Repository management CLI
- [ ] Error handling and logging
- [ ] Performance optimization
- [ ] Documentation and examples
- [ ] CLI: `factbase status`, `factbase repo add/remove`

**Deliverable:** Stable, documented, multi-repo system

---

## CLI Commands

### `factbase init [path]`
Initialize a new knowledge base repository.

```bash
factbase init ./my-knowledge-base
# Creates:
# - ./my-knowledge-base/perspective.yaml (template)
# - ./my-knowledge-base/.factbase/ (metadata directory)
# - Initializes SQLite database
# - Adds to config.yaml repositories list
```

### `factbase scan [repo]`
Scan a repository and index all documents.

```bash
factbase scan main
# Output:
# Scanning repository: main (./knowledge-base)
# Found 47 markdown files
# Processing: people/john-doe.md (new, assigned ID: a1cb2b)
# Processing: projects/platform-mod.md (unchanged)
# ...
# Scan complete: 5 new, 2 updated, 1 deleted, 39 unchanged
```

### `factbase serve [--port PORT]`
Start MCP server (includes watching all repositories).

```bash
factbase serve --port 3000
# Output:
# [2026-01-23 23:30] MCP server listening on http://127.0.0.1:3000
# [2026-01-23 23:30] Watching 2 repositories
# [2026-01-23 23:30] Ready for agent connections
# [2026-01-23 23:31] File changed: main/people/john-doe.md
# [2026-01-23 23:31] Rescanning repository: main
```

### `factbase search <query> [--repo REPO] [--type TYPE]`
Search the knowledge base from CLI.

```bash
factbase search "backend engineers" --type person
# Results:
# 1. John Doe (person) - 0.92 relevance
#    John is a Senior Backend Engineer at Acme Corp...
#    File: people/john-doe.md [a1cb2b]
#
# 2. Sarah Chen (person) - 0.78 relevance
#    DevOps engineer with backend experience...
#    File: people/sarah-chen.md [d4e5f6]
```

### `factbase status`
Show knowledge base statistics.

```bash
factbase status
# Factbase Status
# ===============
# Repositories: 2
#
# [main] Main Knowledge Base
#   Path: ./knowledge-base
#   Documents: 47 (3 deleted)
#   By type: person (23), project (12), company (8), other (4)
#   Links: 156 cross-references
#   Last scan: 2026-01-23 23:31:45
#
# [sales] Sales Team KB
#   Path: /shared/sales-kb
#   Documents: 89
#   ...
#
# Database: .factbase/factbase.db (15.2 MB)
# MCP server: not running
```

### `factbase repo add <id> <path>`
Add a repository to track.

```bash
factbase repo add sales /shared/sales-kb
# Added repository: sales
# Run 'factbase scan sales' to index documents
```

### `factbase repo remove <id>`
Remove a repository from tracking.

```bash
factbase repo remove sales
# Removed repository: sales
# Documents retained in database (marked inactive)
```

---

## Design Decisions

### 1. Filesystem as Source of Truth
Files are trusted. No internal approval workflow.
- Humans can use any editor/tool to manage files
- Git provides version control and audit trail
- LLM-assisted extraction happens outside this system

### 2. One File Per Entity
Each markdown file represents exactly one entity.
- Clear ownership and organization
- Easy to move/rename files (ID travels with file)
- Natural fit for version control

### 3. Freeform Markdown
No required structure beyond our injected header.
- **Required:** `<!-- factbase:XXXXXX -->` (injected automatically)
- **Title:** First H1, or filename if no H1
- **Type:** Derived from parent folder
- No YAML frontmatter parsing, no validation errors
- Template provided as guidance, not requirement

### 4. Document Identity via File Headers
6-character hex ID injected into files on first scan.
```markdown
<!-- factbase:a1cb2b -->
```
- ID persists with the file itself
- Enables tracking across file moves/renames
- Human-readable and non-intrusive

### 5. Cross-Reference Links
Detected entity references annotated inline with IDs.
```markdown
Working with John Doe [[a1cb2b]] on the migration.
```
- Explicit, parseable links
- Builds relationship graph
- Full rescan detects new mentions

### 6. Full Rescan on Changes
Any file change triggers a complete directory rescan.
- Simpler than tracking individual events
- Detects moves by matching IDs
- Ensures consistency
- Two-phase scan: documents first, then links for ALL documents
- Second pass catches links TO new documents FROM existing docs

### 7. Full Document Embedding
Index entire document as single unit (no chunking).
- Documents are entity-focused, should stay reasonably sized
- Simpler implementation
- Can add chunking later if precision issues arise

### 8. Multi-Repository Support
Support multiple repositories per factbase instance.
- Namespace isolation via repo_id
- Single database, multiple watched directories
- Each repo has its own perspective.yaml

### 9. Soft Deletes
Mark documents as deleted rather than removing from DB.
- Preserve history for audit trail
- Avoid broken references during file moves
- Cleanup policy deferred

### 10. Fatal Embedding Errors
Exit process on embedding service failures.
- User needs to fix underlying issue (Bedrock creds, Ollama running)
- Partial indexing would leave system in broken state
- Clear error message tells user what to do

### 11. Read-Only MCP Interface
Agents can only read, not write.
- Prevents accidental corruption
- Humans maintain control
- Simpler security model

### 12. Localhost-Only MCP Server
No authentication, bound to 127.0.0.1.
- Local tool for personal/team use
- Streamable HTTP transport
- Can add auth later if needed

---

## Success Criteria

### Phase 1 Success
- [ ] Can scan directory and find all .md files
- [ ] Injects factbase ID header into new files
- [ ] Extracts title from H1 or filename
- [ ] Derives type from parent folder
- [ ] Stores documents in SQLite
- [ ] CLI commands work: `init`, `scan`, `status`

### Phase 2 Success
- [ ] Generates embeddings for all documents
- [ ] Fatal exit on embedding errors with clear message
- [ ] Vector search returns relevant results
- [ ] Detects and stores `[[id]]` cross-references
- [ ] Search latency < 100ms for typical queries
- [ ] CLI: `search` works with filters

### Phase 3 Success
- [ ] File watcher detects changes
- [ ] Debouncing prevents excessive rescans
- [ ] Full rescan triggered on any change
- [ ] MCP server responds to tool calls
- [ ] All 4 MCP tools functional
- [ ] CLI: `serve` runs continuously

### Phase 4 Success
- [ ] Multiple repositories supported
- [ ] Repository add/remove works
- [ ] System runs continuously without crashes
- [ ] Comprehensive logging
- [ ] Documentation complete
- [ ] Example knowledge base provided

---

## Dependencies (Cargo.toml)

```toml
[package]
name = "factbase"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# HTTP client (for Ollama)
reqwest = { version = "0.11", features = ["json"] }

# File watching
notify = "6"
notify-debouncer-mini = "0.4"

# Database
rusqlite = { version = "0.32", features = ["bundled"] }
sqlite-vec = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# HTTP server (MCP)
axum = { version = "0.7", features = ["macros"] }
tower-http = { version = "0.5", features = ["cors", "trace"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Utilities
regex = "1"                      # For [[id]] link detection
rand = "0.8"                     # For ID generation
sha2 = "0.10"                    # File hashing
hex = "0.4"                      # ID generation
walkdir = "2"                    # Directory traversal
glob = "0.3"                     # Ignore pattern matching
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dirs = "5"                       # For ~/.config path

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
```

---

## Comparison: Factbase vs. Fact-Graph

| Aspect | Fact-Graph | Factbase |
|--------|-----------|----------|
| **Source of Truth** | Message batches (Slack, email) | Markdown files |
| **Fact Granularity** | Individual extracted facts | Entire document |
| **Human Approval** | Required (CLI workflow) | Not required (files are trusted) |
| **Extraction** | LLM batch processing | External (human + LLM) |
| **Document Format** | N/A | Freeform markdown |
| **Identity** | UUID in database | 6-char hex in file header |
| **Storage** | SQLite (facts + candidates) | SQLite (documents + embeddings) |
| **Agent Access** | MCP (submit batches, query facts) | MCP (read-only search) |
| **Update Model** | Append-only with supersession | File modification + rescan |
| **Cross-References** | Implicit via entity matching | Explicit `[[id]]` links |
| **Complexity** | High (prioritization, conflicts) | Low (scan + index) |
| **Use Case** | Curate facts from unstructured comms | Organize structured knowledge |

---

## Next Steps

1. **Review this design** - Confirm approach and architecture
2. **Initialize project** - `cargo init`, set up Cargo.toml with dependencies
3. **Start Phase 1** - Implement scanner, processor, and basic storage
4. **Iterate** - Build incrementally, test with real markdown files

---

## Notes

- **Freeform flexibility:** Any markdown works; template is guidance only
- **ID in file:** Document identity travels with the file, survives moves/renames
- **Git-friendly:** Markdown files + injected IDs work naturally with version control
- **Human-editable:** Any text editor works, no special tools required
- **LLM-assisted curation:** Humans can use LLMs to generate these markdown files, then review/edit
- **Incremental adoption:** Can start with a few files and grow organically
- **Multi-repo:** Different knowledge bases for different contexts (work, personal, team)

---

**End of Plan**
