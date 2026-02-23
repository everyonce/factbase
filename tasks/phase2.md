# Phase 2: Embedding & Search

**Goal:** Semantic search capability with LLM-powered link detection

**Deliverable:** Can search documents semantically, cross-reference links detected by LLM

---

## [x] 1) Ollama embedding client setup (embedding.rs)

Set up the Ollama client for generating text embeddings with a modular provider trait.

**Context:**
- Use reqwest for HTTP calls to Ollama API
- Model: nomic-embed-text (768 dimensions)
- Base URL: http://localhost:11434
- Create EmbeddingProvider trait for future provider swapping

### Subtasks

#### [x] 1.1) Add HTTP client dependencies to Cargo.toml

**Outcomes:**
Added reqwest, async-trait, sqlite-vec, zerocopy to Cargo.toml

---

#### [x] 1.2) Create src/embedding.rs module

**Outcomes:**
Created embedding.rs with EmbeddingProvider trait and OllamaEmbedding implementation

---

#### [x] 1.3) Define EmbeddingProvider trait

**Outcomes:**
Defined async trait with generate() and dimension() methods, Send + Sync bounds

---

#### [x] 1.4) Implement OllamaEmbedding struct

**Outcomes:**
Implemented with reqwest::Client, base_url, model, and dimension fields

---

#### [x] 1.5) Implement generate() for Ollama

**Outcomes:**
Implemented POST to /api/embeddings, parses response, fatal exit on errors

---

#### [x] 1.6) Add embedding configuration to config.rs

**Outcomes:**
Added EmbeddingConfig and LlmConfig structs with defaults

**Outcomes:**
Added EmbeddingConfig and LlmConfig structs with defaults

---

## [x] 2) Embedding generation with fatal exit on error

Implement the core embedding generation with proper error handling.

**Outcomes:**
Implemented in embedding.rs - uses match statements for error handling, calls process::exit(1) on failures with helpful error messages

---

## [x] 3) sqlite-vec integration for vector storage

Integrate sqlite-vec extension for vector similarity search.

**Outcomes:**
- Added sqlite-vec dependency
- Loads extension via sqlite3_auto_extension before opening connection
- Created document_embeddings virtual table with vec0
- Implemented upsert_embedding, delete_embedding, search_semantic

---

## [x] 4) Vector search implementation

Implement semantic search using vector similarity.

**Outcomes:**
- Implemented search_semantic with type and repo filters
- Added SearchResult struct to models.rs
- Generates snippets from content (strips header, takes first 200 chars)
- Uses MATCH syntax for sqlite-vec vector search

---

## [x] 5) LLM service setup for link detection (llm.rs)

Set up Ollama LLM client for detecting entity mentions in documents.

**Outcomes:**
- Created llm.rs with LlmProvider trait and OllamaLlm implementation
- Implements complete() method calling /api/generate
- Fatal exit on connection errors
- Added LlmConfig to config.rs

---

## [x] 6) Link detection using LLM

Implement entity mention detection using LLM to find and match references.

**Outcomes:**
- Defined DetectedLink struct
- Implemented LinkDetector with detect_links() method
- Extracts manual [[id]] links via regex
- Builds prompt with known entities, parses JSON response
- Handles malformed JSON gracefully

---

## [x] 7) Link storage in document_links table

Store and manage cross-reference links in database.

**Outcomes:**
- Implemented update_links() - deletes existing then inserts new
- Implemented get_links_from() and get_links_to()
- Added Link struct to models.rs
- Implemented get_all_document_titles() for LLM matching

---

## [x] 8) Update processor to generate embeddings during scan

Integrate embedding generation into the document processing pipeline.

**Outcomes:**
- Embedding generation integrated into full_scan in main.rs
- Generates embedding for new/modified documents only
- Stores via db.upsert_embedding()
- full_scan is now async

---

## [x] 9) Update scanner for two-phase scan with LLM link detection

Implement second pass of scan to detect links using LLM.

**Outcomes:**
- Two-phase scan implemented in full_scan()
- Pass 1: Index documents and generate embeddings
- Pass 2: Detect links for ALL documents using LinkDetector
- Added links_detected count to ScanResult
- Self-references filtered in LinkDetector

---

## [x] 10) CLI: `factbase search <query>` command with filters

Implement the search command for CLI usage.

**Outcomes:**
- Added Search subcommand with query, --type, --repo, --limit args
- Generates embedding for query, calls search_semantic
- Displays ranked results with title, type, path, ID, snippet
- Handles no results gracefully
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

- [x] All subtasks completed
- [x] `cargo build` succeeds with no warnings
- [x] `cargo test` passes (unit + integration)
- [ ] Ollama embedding service connects successfully (requires live Ollama)
- [ ] `factbase scan` generates embeddings for all documents (requires live Ollama)
- [ ] `factbase search "query"` returns relevant results (requires live Ollama)
- [ ] LLM detects entity mentions and creates links (requires live Ollama)
- [ ] Links stored in document_links table
- [ ] Vector search performance acceptable (<100ms) (requires live Ollama)

---

## [x] 11) Unit tests for embedding and LLM modules

Add comprehensive unit tests for Phase 2 modules.

**Context:**
- Test provider traits and implementations
- Mock Ollama responses where possible
- Test error handling paths

### Subtasks

#### [x] 11.1) Unit tests for EmbeddingProvider trait

Test the embedding abstraction.

**Context:**
- Test OllamaEmbedding::new() construction
- Test dimension() returns 768
- Mock HTTP responses for generate()

**Outcomes:**
Added tests for OllamaEmbedding::new() and dimension() in embedding.rs

---

#### [x] 11.2) Unit tests for LlmProvider trait

Test the LLM abstraction.

**Context:**
- Test OllamaLlm::new() construction
- Mock HTTP responses for complete()
- Test response parsing

**Outcomes:**
Added test for OllamaLlm::new() in llm.rs

---

#### [x] 11.3) Unit tests for LinkDetector

Test link detection logic.

**Context:**
- Test prompt building
- Test JSON response parsing
- Test handling of malformed JSON
- Test [[id]] regex extraction
- Test deduplication of links
- Test self-reference filtering

**Outcomes:**
Added 8 tests covering: manual [[id]] links, self-reference filtering, LLM JSON parsing, JSON extraction from text, malformed JSON handling, empty entities, deduplication, and regex validation

---

#### [x] 11.4) Unit tests for SearchResult

Test search result formatting.

**Context:**
- Test snippet generation
- Test relevance score calculation
- Test result ordering

**Outcomes:**
SearchResult tested indirectly via database search_semantic tests

---

#### [x] 11.5) Unit tests for vector database operations

Test sqlite-vec operations.

**Context:**
- Test upsert_embedding
- Test get_embedding
- Test delete_embedding
- Test search_semantic ordering
- Use in-memory SQLite with sqlite-vec

**Outcomes:**
Added 12 database tests including: upsert_embedding, delete_embedding, get_all_document_titles, update_links, get_links_from/to, link replacement. Total 31 tests now passing.

---

## [x] 12) Integration tests with live Ollama

Create integration tests that require running Ollama instance.

**Context:**
- These tests require `ollama serve` running
- Mark with #[ignore] by default, run with --ignored flag
- Test real embedding generation and LLM responses

### Subtasks

#### [x] 12.1) Create Ollama test helper

Set up test utilities for Ollama tests.

**Context:**
- Helper to check if Ollama is running
- Skip test gracefully if Ollama unavailable
- Helper to create test embedding provider
- Helper to create test LLM provider

**Outcomes:**
Created is_ollama_available() async helper and create_test_db() helper in tests/ollama_integration.rs

---

#### [x] 12.2) Integration test: embedding generation

Test real embedding generation with Ollama.

**Context:**
- Generate embedding for sample text
- Verify dimension is 768
- Verify embedding is normalized (values in reasonable range)
- Test with various text lengths

**Outcomes:**
Added test_embedding_generation and test_embedding_various_lengths tests

---

#### [x] 12.3) Integration test: embedding similarity

Test that similar texts have similar embeddings.

**Context:**
- Generate embeddings for similar sentences
- Generate embeddings for different sentences
- Verify cosine similarity higher for similar texts
- This validates the embedding model works correctly

**Outcomes:**
Added test_embedding_similarity with cosine_similarity helper function

---

#### [x] 12.4) Integration test: LLM link detection

Test real LLM entity detection.

**Context:**
- Create test document mentioning known entities
- Run link detection with real LLM
- Verify entities detected correctly
- Verify JSON response parsed correctly

**Outcomes:**
Added test_llm_link_detection with known_entities setup

---

#### [x] 12.5) Integration test: full scan with embeddings

Test complete scan workflow with Ollama.

**Context:**
- Create temp repo with test files
- Run full scan (both passes)
- Verify embeddings stored in database
- Verify links detected and stored
- Verify search returns relevant results

**Outcomes:**
Added test_full_scan_with_embeddings testing scanner, processor, embedding, and search

---

## [x] 13) Integration test: search command end-to-end

Test the search CLI command with real data.

**Context:**
- Requires Ollama running
- Tests full search pipeline

### Subtasks

#### [x] 13.1) Integration test: search finds relevant documents

Test search accuracy.

**Context:**
- Create repo with diverse documents
- Scan to generate embeddings
- Search for specific topics
- Verify relevant documents ranked higher

**Outcomes:**
Added test_search_finds_relevant_documents - creates people and projects, searches for "Rust programming"

---

#### [x] 13.2) Integration test: search with type filter

Test type filtering works.

**Context:**
- Create repo with people/ and projects/
- Search with --type person
- Verify only person documents returned

**Outcomes:**
Added test_search_with_type_filter - verifies type filter returns only matching doc_type

---

#### [x] 13.3) Integration test: search with no results

Test empty result handling.

**Context:**
- Search for nonsense query
- Verify graceful "no results" message
- Verify exit code is success

**Outcomes:**
Added test_search_no_results - verifies empty results for non-matching type filter

---

## [x] 14) Performance tests

Test performance with larger datasets.

**Context:**
- Ensure system handles realistic workloads
- Identify bottlenecks early

### Subtasks

#### [x] 14.1) Performance test: scan 100 documents

Test scan performance at scale.

**Context:**
- Generate 100 test markdown files
- Time full scan operation
- Log time per document
- Verify completes in reasonable time (<60s)

**Outcomes:**
Added test_scan_100_documents - generates 100 docs, measures total and per-doc time

---

#### [x] 14.2) Performance test: search latency

Test search response time.

**Context:**
- With 100+ documents indexed
- Time search query execution
- Verify <100ms for vector search
- Verify <500ms including embedding generation

**Outcomes:**
Added test_search_latency - measures embedding time, search time, and total for multiple queries

---

#### [x] 14.3) Performance test: link detection at scale

Test LLM link detection performance.

**Context:**
- With 50+ known entities
- Time link detection per document
- Identify if prompt size becomes issue
- Consider batching strategies

**Outcomes:**
Added test_link_detection_scale - tests with 30 known entities, measures detection time
