# Phase 2: Embedding & Search

**Goal:** Semantic search capability with LLM-powered link detection

**Deliverable:** Can search documents semantically, cross-reference links detected by LLM

---

## [ ] 1) Ollama embedding client setup (embedding.rs)

Set up the Ollama client for generating text embeddings with a modular provider trait.

**Context:**
- Use reqwest for HTTP calls to Ollama API
- Model: nomic-embed-text (768 dimensions)
- Base URL: http://localhost:11434
- Create EmbeddingProvider trait for future provider swapping

### Subtasks

#### [ ] 1.1) Add HTTP client dependencies to Cargo.toml

Add the required crates for Ollama communication.

**Context:**
- reqwest = { version = "0.11", features = ["json"] }
- async-trait = "0.1"
- These replace the AWS SDK dependencies

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.2) Create src/embedding.rs module

Create the embedding service module file.

**Context:**
- Will contain EmbeddingProvider trait and OllamaEmbedding impl
- Import reqwest and async_trait
- Export from lib.rs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.3) Define EmbeddingProvider trait

Create provider-agnostic trait for embeddings.

**Context:**
- async fn generate(&self, text: &str) -> Result<Vec<f32>>
- fn dimension(&self) -> usize
- Use #[async_trait] attribute
- Trait must be Send + Sync for async usage

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.4) Implement OllamaEmbedding struct

Create the Ollama-specific embedding provider.

**Context:**
- Fields: client (reqwest::Client), base_url (String), model (String)
- Constructor: new(base_url: &str, model: &str)
- Implement EmbeddingProvider trait

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.5) Implement generate() for Ollama

Call Ollama embeddings API.

**Context:**
- POST to {base_url}/api/embeddings
- Body: {"model": "nomic-embed-text", "prompt": text}
- Parse response["embedding"] as Vec<f32>
- Return 768-dimension vector

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.6) Add embedding configuration to config.rs

Extend config to include embedding settings.

**Context:**
- Add EmbeddingConfig struct: provider, base_url, model, dimension
- Add to main Config struct
- Defaults: ollama, localhost:11434, nomic-embed-text, 768

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 2) Embedding generation with fatal exit on error

Implement the core embedding generation with proper error handling.

**Context:**
- Call Ollama /api/embeddings endpoint
- Parse response to extract embedding vector
- Fatal exit (process::exit) on connection failures
- User must ensure Ollama is running before continuing

### Subtasks

#### [ ] 2.1) Implement generate() method

Core embedding generation method.

**Context:**
- Build request body with model and prompt
- POST to Ollama API
- This is an async method

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.2) Build Ollama request body

Construct the JSON payload for embedding request.

**Context:**
- Format: {"model": "nomic-embed-text", "prompt": "your text"}
- Use serde_json for serialization
- reqwest handles JSON automatically with .json()

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.3) Parse embedding from response

Extract the embedding vector from Ollama response.

**Context:**
- Response body contains JSON with "embedding" array
- Parse as Vec<f64> then convert to Vec<f32>
- Validate dimension matches expected (768)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.4) Implement fatal exit on error

Exit process when embedding service fails.

**Context:**
- Print clear error message to stderr
- Include suggestion: "Ensure Ollama is running: ollama serve"
- Call std::process::exit(1)
- This is intentional - partial indexing is worse than failing

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.5) Add unit tests for embedding service

Test embedding generation (may need mocking).

**Context:**
- Test successful embedding returns correct dimension (768)
- Test error handling triggers fatal exit
- Consider integration test with real Ollama (optional)

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 3) sqlite-vec integration for vector storage

Integrate sqlite-vec extension for vector similarity search.

**Context:**
- sqlite-vec provides vector operations in SQLite
- Create document_embeddings virtual table
- Store 768-dimension float vectors (nomic-embed-text)
- Enable cosine similarity search

### Subtasks

#### [ ] 3.1) Add sqlite-vec dependency to Cargo.toml

Add the sqlite-vec crate.

**Context:**
- sqlite-vec = "0.1" (check for latest version)
- May need to enable specific features
- Ensure compatibility with rusqlite version

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.2) Load sqlite-vec extension in Database::new()

Initialize the vector extension when opening database.

**Context:**
- Call sqlite_vec::sqlite3_vec_init or equivalent
- Must be done before creating virtual tables
- Handle extension loading errors

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.3) Create document_embeddings virtual table

Add the vector table to schema initialization.

**Context:**
- CREATE VIRTUAL TABLE document_embeddings USING vec0(...)
- document_id TEXT PRIMARY KEY
- embedding FLOAT[768] (nomic-embed-text dimension)
- Add to init_schema() in database.rs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.4) Implement upsert_embedding(&self, doc_id: &str, embedding: &[f32])

Store or update an embedding for a document.

**Context:**
- vec0 doesn't support INSERT OR REPLACE, so DELETE then INSERT
- Convert Vec<f32> to blob format sqlite-vec expects
- Link to document by ID

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.5) Implement get_embedding(&self, doc_id: &str) -> Option<Vec<f32>>

Retrieve an embedding by document ID.

**Context:**
- SELECT embedding FROM document_embeddings WHERE document_id = ?
- Convert from sqlite-vec blob format to Vec<f32>
- Return None if not found

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.6) Implement delete_embedding(&self, doc_id: &str)

Remove an embedding when document is deleted.

**Context:**
- DELETE FROM document_embeddings WHERE document_id = ?
- Called when document is hard-deleted (future)
- For soft delete, embedding can remain

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 4) Vector search implementation

Implement semantic search using vector similarity.

**Context:**
- Generate embedding for search query
- Find documents with similar embeddings using cosine distance
- Return ranked results with relevance scores
- Support filtering by type and repo

### Subtasks

#### [ ] 4.1) Implement search_semantic(&self, embedding: &[f32], limit: usize) -> Vec<SearchResult>

Core vector search method in database.

**Context:**
- Use vec_distance_cosine() function from sqlite-vec
- JOIN documents with document_embeddings
- Filter out is_deleted = true
- ORDER BY distance ASC, LIMIT

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.2) Define SearchResult struct

Create struct for search results.

**Context:**
- id: String
- title: String
- doc_type: Option<String>
- file_path: String
- relevance_score: f32 (1.0 - distance for cosine)
- snippet: String (first N chars of content)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.3) Add type filter to search

Support filtering results by document type.

**Context:**
- Add optional doc_type parameter
- Add WHERE clause: AND doc_type = ? if provided
- Allow searching across all types when None

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.4) Add repo filter to search

Support filtering results by repository.

**Context:**
- Add optional repo_id parameter
- Add WHERE clause: AND repo_id = ? if provided
- Default to searching all repos when None

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.5) Generate snippet from content

Create preview snippet for search results.

**Context:**
- Take first N characters (configurable, default 200)
- Strip the factbase header line
- Truncate at word boundary if possible
- Add "..." if truncated

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.6) Add search tests

Test vector search functionality.

**Context:**
- Test: search returns results ordered by relevance
- Test: type filter works correctly
- Test: repo filter works correctly
- Test: deleted documents excluded

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 5) LLM service setup for link detection (llm.rs)

Set up Ollama LLM client for detecting entity mentions in documents.

**Context:**
- Use reqwest for HTTP calls to Ollama API
- Model: rnj-1 (configurable)
- Create LlmProvider trait for future provider swapping
- LLM detects entity names and matches to existing documents

### Subtasks

#### [ ] 5.1) Create src/llm.rs module

Create the LLM service module file.

**Context:**
- Will contain LlmProvider trait, OllamaLlm impl, and LinkDetector
- Import reqwest and async_trait
- Export from lib.rs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.2) Define LlmProvider trait

Create provider-agnostic trait for LLM completions.

**Context:**
- async fn complete(&self, prompt: &str) -> Result<String>
- Use #[async_trait] attribute
- Trait must be Send + Sync for async usage

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.3) Implement OllamaLlm struct

Create the Ollama-specific LLM provider.

**Context:**
- Fields: client (reqwest::Client), base_url (String), model (String)
- Constructor: new(base_url: &str, model: &str)
- Implement LlmProvider trait

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.4) Implement complete() for Ollama

Call Ollama generate API.

**Context:**
- POST to {base_url}/api/generate
- Body: {"model": "rnj-1", "prompt": text, "stream": false}
- Parse response["response"] as String
- Handle errors with fatal exit

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.5) Add LLM configuration to config.rs

Extend config to include LLM settings.

**Context:**
- Add LlmConfig struct: provider, base_url, model
- Add to main Config struct
- Defaults: ollama, localhost:11434, rnj-1

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 6) Link detection using LLM

Implement entity mention detection using LLM to find and match references.

**Context:**
- LLM analyzes document content to find entity mentions
- Matches mentions against known document titles
- Returns list of detected links with context
- Also preserves manually added [[id]] links

### Subtasks

#### [ ] 6.1) Define DetectedLink struct

Create struct for LLM-detected links.

**Context:**
- target_id: String (matched document ID)
- target_title: String (matched document title)
- mention_text: String (how entity was mentioned)
- context: String (surrounding text)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.2) Implement LinkDetector struct

Create the link detection service.

**Context:**
- Fields: llm (Box<dyn LlmProvider>), db reference
- Constructor accepts LLM provider and database
- Main method: detect_links()

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.3) Implement get_all_document_titles() in database

Get all document titles for LLM matching.

**Context:**
- SELECT id, title FROM documents WHERE is_deleted = false
- Return Vec<(String, String)> of (id, title) pairs
- Used to build prompt for LLM

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.4) Build link detection prompt

Create prompt for LLM to find entity mentions.

**Context:**
- Include list of known entities (title + ID)
- Include document content to analyze
- Request JSON array output format
- Ask for exact or close matches only

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.5) Implement detect_links() method

Main link detection logic.

**Context:**
- Get all document titles from database
- Build prompt with entities and content
- Call LLM for analysis
- Parse JSON response
- Map entity names to document IDs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.6) Parse LLM JSON response

Extract detected links from LLM output.

**Context:**
- LLM returns JSON array: [{"entity": "name", "context": "text"}]
- Parse with serde_json
- Handle malformed JSON gracefully (log warning, return empty)
- Match entity names to document IDs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.7) Handle existing [[id]] links

Preserve manually added links.

**Context:**
- Also extract [[id]] patterns with regex
- Merge with LLM-detected links
- Deduplicate by target_id
- Manual links take precedence

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 7) Link storage in document_links table

Store and manage cross-reference links in database.

**Context:**
- document_links table tracks source → target relationships
- Links are processed in SECOND PASS after all documents indexed
- Support querying links in both directions

### Subtasks

#### [ ] 7.1) Implement update_links(&self, source_id: &str, links: &[DetectedLink])

Update all links for a document.

**Context:**
- Delete existing links for source_id first
- Insert new links for each detected link
- Use transaction for atomicity
- Store context from DetectedLink

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.2) Implement get_links_from(&self, source_id: &str) -> Vec<Link>

Get all documents this document links to.

**Context:**
- SELECT * FROM document_links WHERE source_id = ?
- Return list of Link structs
- Used by get_entity to show "links_to"

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.3) Implement get_links_to(&self, target_id: &str) -> Vec<Link>

Get all documents that link to this document.

**Context:**
- SELECT * FROM document_links WHERE target_id = ?
- Return list of Link structs
- Used by get_entity to show "linked_from"

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.4) Define Link struct

Create struct for link records.

**Context:**
- source_id: String
- target_id: String
- context: Option<String>
- created_at: DateTime<Utc>

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 8) Update processor to generate embeddings during scan

Integrate embedding generation into the document processing pipeline.

**Context:**
- Generate embedding for each document during FIRST PASS of scan
- Store embedding alongside document
- Handle embedding errors (fatal exit)
- Links are NOT processed in this pass (done in second pass)

### Subtasks

#### [ ] 8.1) Add EmbeddingProvider to DocumentProcessor

Include embedding provider in processor struct.

**Context:**
- Add embedding: Box<dyn EmbeddingProvider> field
- Update constructor to accept provider
- Provider created once, reused for all documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.2) Generate embedding in process_file()

Call embedding provider during document processing.

**Context:**
- After extracting content, call embedding.generate()
- Pass full document content
- Await the async result

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.3) Store embedding in database

Save the generated embedding.

**Context:**
- Call db.upsert_embedding() with doc ID and vector
- Do this after upserting the document
- Handle storage errors

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.4) Skip embedding if content unchanged

Optimize by not regenerating unchanged embeddings.

**Context:**
- Check if document hash changed
- If unchanged, skip embedding generation
- Saves API calls and time on large repos

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.5) Update full_scan to use async processor

Make scan async to support embedding generation.

**Context:**
- Change full_scan to async fn
- Await process_file calls
- Consider batching or parallelization (future optimization)

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 9) Update scanner for two-phase scan with LLM link detection

Implement second pass of scan to detect links using LLM.

**Context:**
- Links are processed AFTER all documents are indexed (second pass)
- This ensures all target document IDs exist for matching
- Scan ALL documents for links (not just changed ones)
- New documents may be referenced by existing documents
- Uses LLM to intelligently detect entity mentions

### Subtasks

#### [ ] 9.1) Implement link_pass() in scanner

Create second pass that processes links for all documents.

**Context:**
- Run after document pass completes
- Iterate ALL documents in repository (not just changed)
- Use LinkDetector to find entity mentions
- Store detected links

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.2) Add LinkDetector to Scanner

Include link detector in scanner struct.

**Context:**
- Add link_detector: LinkDetector field
- Initialize with LLM provider and database
- Created once, reused for all documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.3) Call detect_links() for each document

Detect links in document content.

**Context:**
- Get document content from database (already stored)
- Pass content to link_detector.detect_links()
- Collect DetectedLink results

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.4) Store links in database

Save detected links.

**Context:**
- Call db.update_links() with source ID and detected links
- Replace all links for document (handles removed links)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.5) Handle self-references

Don't store links from document to itself.

**Context:**
- Filter out target_id == source_id
- Could happen if document mentions its own title
- Not useful as a relationship

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.6) Log link statistics

Report link counts in scan results.

**Context:**
- Track total links found
- Track LLM-detected vs manual [[id]] links
- Include in ScanResult or log separately

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.7) Update full_scan to call both passes

Orchestrate two-phase scan.

**Context:**
- First pass: process documents (ID, title, type, embedding)
- Second pass: detect links for ALL documents using LLM
- Return combined ScanResult

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 10) CLI: `factbase search <query>` command with filters

Implement the search command for CLI usage.

**Context:**
- Takes natural language query
- Generates embedding and searches
- Displays ranked results
- Supports --type and --repo filters

### Subtasks

#### [ ] 10.1) Define SearchArgs struct

Define CLI arguments for search command.

**Context:**
- query: String (required, positional)
- --type: Option<String> for type filter
- --repo: Option<String> for repo filter
- --limit: Option<usize> (default 10)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.2) Add Search subcommand to CLI

Register search in clap command structure.

**Context:**
- Add to Commands enum
- Include in main.rs match
- Parse SearchArgs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.3) Implement search command handler

Main logic for search command.

**Context:**
- Load config and open database
- Create embedding provider (Ollama)
- Generate embedding for query
- Call db.search_semantic()
- Display results

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.4) Format and display search results

Pretty-print search results.

**Context:**
- Show rank number, title, type, relevance score
- Show snippet preview
- Show file path and ID
- Use colors if terminal supports

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.5) Handle no results case

Graceful handling when search finds nothing.

**Context:**
- Print friendly message: "No results found for: {query}"
- Suggest checking spelling or trying different terms
- Exit with success (not an error)

**Outcomes:**
<!-- Agent notes -->

---

## Completion Checklist

- [ ] All subtasks completed
- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes (unit + integration)
- [ ] Ollama embedding service connects successfully
- [ ] `factbase scan` generates embeddings for all documents
- [ ] `factbase search "query"` returns relevant results
- [ ] LLM detects entity mentions and creates links
- [ ] Links stored in document_links table
- [ ] Vector search performance acceptable (<100ms)

---

## [ ] 11) Unit tests for embedding and LLM modules

Add comprehensive unit tests for Phase 2 modules.

**Context:**
- Test provider traits and implementations
- Mock Ollama responses where possible
- Test error handling paths

### Subtasks

#### [ ] 11.1) Unit tests for EmbeddingProvider trait

Test the embedding abstraction.

**Context:**
- Test OllamaEmbedding::new() construction
- Test dimension() returns 768
- Mock HTTP responses for generate()

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.2) Unit tests for LlmProvider trait

Test the LLM abstraction.

**Context:**
- Test OllamaLlm::new() construction
- Mock HTTP responses for complete()
- Test response parsing

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.3) Unit tests for LinkDetector

Test link detection logic.

**Context:**
- Test prompt building
- Test JSON response parsing
- Test handling of malformed JSON
- Test [[id]] regex extraction
- Test deduplication of links
- Test self-reference filtering

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.4) Unit tests for SearchResult

Test search result formatting.

**Context:**
- Test snippet generation
- Test relevance score calculation
- Test result ordering

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.5) Unit tests for vector database operations

Test sqlite-vec operations.

**Context:**
- Test upsert_embedding
- Test get_embedding
- Test delete_embedding
- Test search_semantic ordering
- Use in-memory SQLite with sqlite-vec

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 12) Integration tests with live Ollama

Create integration tests that require running Ollama instance.

**Context:**
- These tests require `ollama serve` running
- Mark with #[ignore] by default, run with --ignored flag
- Test real embedding generation and LLM responses

### Subtasks

#### [ ] 12.1) Create Ollama test helper

Set up test utilities for Ollama tests.

**Context:**
- Helper to check if Ollama is running
- Skip test gracefully if Ollama unavailable
- Helper to create test embedding provider
- Helper to create test LLM provider

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.2) Integration test: embedding generation

Test real embedding generation with Ollama.

**Context:**
- Generate embedding for sample text
- Verify dimension is 768
- Verify embedding is normalized (values in reasonable range)
- Test with various text lengths

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.3) Integration test: embedding similarity

Test that similar texts have similar embeddings.

**Context:**
- Generate embeddings for similar sentences
- Generate embeddings for different sentences
- Verify cosine similarity higher for similar texts
- This validates the embedding model works correctly

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.4) Integration test: LLM link detection

Test real LLM entity detection.

**Context:**
- Create test document mentioning known entities
- Run link detection with real LLM
- Verify entities detected correctly
- Verify JSON response parsed correctly

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.5) Integration test: full scan with embeddings

Test complete scan workflow with Ollama.

**Context:**
- Create temp repo with test files
- Run full scan (both passes)
- Verify embeddings stored in database
- Verify links detected and stored
- Verify search returns relevant results

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 13) Integration test: search command end-to-end

Test the search CLI command with real data.

**Context:**
- Requires Ollama running
- Tests full search pipeline

### Subtasks

#### [ ] 13.1) Integration test: search finds relevant documents

Test search accuracy.

**Context:**
- Create repo with diverse documents
- Scan to generate embeddings
- Search for specific topics
- Verify relevant documents ranked higher

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.2) Integration test: search with type filter

Test type filtering works.

**Context:**
- Create repo with people/ and projects/
- Search with --type person
- Verify only person documents returned

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.3) Integration test: search with no results

Test empty result handling.

**Context:**
- Search for nonsense query
- Verify graceful "no results" message
- Verify exit code is success

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 14) Performance tests

Test performance with larger datasets.

**Context:**
- Ensure system handles realistic workloads
- Identify bottlenecks early

### Subtasks

#### [ ] 14.1) Performance test: scan 100 documents

Test scan performance at scale.

**Context:**
- Generate 100 test markdown files
- Time full scan operation
- Log time per document
- Verify completes in reasonable time (<60s)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.2) Performance test: search latency

Test search response time.

**Context:**
- With 100+ documents indexed
- Time search query execution
- Verify <100ms for vector search
- Verify <500ms including embedding generation

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.3) Performance test: link detection at scale

Test LLM link detection performance.

**Context:**
- With 50+ known entities
- Time link detection per document
- Identify if prompt size becomes issue
- Consider batching strategies

**Outcomes:**
<!-- Agent notes -->
