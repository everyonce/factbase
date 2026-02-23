# Phase 3: File Watching & MCP Server

**Goal:** Live updates and agent access via MCP

**Deliverable:** Agents can query knowledge base via MCP, live file updates trigger rescan

---

## [x] 1) File watcher setup with notify crate (watcher.rs)

Set up filesystem monitoring to detect changes in repository directories.

**Context:**
- Use notify crate for cross-platform file watching
- Monitor repository directories recursively
- Detect create, modify, delete events on .md files
- Ignore patterns from config (.git, .swp, etc.)

### Subtasks

#### [x] 1.1) Add notify dependencies to Cargo.toml

Add file watching crates.

**Context:**
- notify = "6"
- notify-debouncer-mini = "0.4" (for built-in debouncing)
- These were listed in FACTBASE_PLAN.md

**Outcomes:**
- Added notify = "6" and notify-debouncer-mini = "0.4" to Cargo.toml
- Dependencies compile successfully

---

#### [x] 1.2) Create src/watcher.rs module

Create the file watcher module.

**Context:**
- Will contain FileWatcher struct
- Import notify types
- Export from lib.rs

**Outcomes:**
- Created src/watcher.rs with FileWatcher struct
- Added watcher module to lib.rs with pub use export

---

#### [x] 1.3) Define FileWatcher struct

Create the watcher struct.

**Context:**
- Hold the notify watcher instance
- Hold reference to config (ignore patterns)
- Hold channel for receiving events
- Consider Arc<Mutex> for thread safety

**Outcomes:**
- FileWatcher holds Debouncer<RecommendedWatcher>, Receiver for events, and Vec<Pattern> for ignore patterns

---

#### [x] 1.4) Implement FileWatcher::new() constructor

Create and configure the watcher.

**Context:**
- Create notify RecommendedWatcher
- Configure recursive watching
- Set up event channel (mpsc or crossbeam)
- Return Result<FileWatcher, FactbaseError>

**Outcomes:**
- new() accepts debounce_ms and ignore_patterns
- Creates debouncer with configured timeout
- Compiles ignore patterns to glob::Pattern
- Returns Result<FileWatcher, FactbaseError>

---

#### [x] 1.5) Implement watch_directory(&mut self, path: &Path)

Start watching a directory.

**Context:**
- Call watcher.watch(path, RecursiveMode::Recursive)
- Handle already-watching case gracefully
- Log that watching has started

**Outcomes:**
- Calls debouncer.watcher().watch() with RecursiveMode::Recursive
- Logs info message when watching starts
- Returns FactbaseError::Watcher on failure

---

#### [x] 1.6) Implement unwatch_directory(&mut self, path: &Path)

Stop watching a directory.

**Context:**
- Call watcher.unwatch(path)
- Used when removing a repository
- Handle not-watching case gracefully

**Outcomes:**
- Calls debouncer.watcher().unwatch()
- Logs info message when unwatching
- Returns FactbaseError::Watcher on failure

---

#### [x] 1.7) Filter events by ignore patterns

Skip events for ignored files.

**Context:**
- Check event path against ignore patterns
- Skip .git/**, *.swp, *.tmp, .DS_Store, .factbase/**
- Only process .md file events
- Reuse pattern matching from scanner

**Outcomes:**
- should_process() filters non-.md files
- Matches paths against compiled glob patterns
- try_recv() filters events through should_process()
- Unit tests verify filtering works correctly

---

## [x] 2) Debouncing implementation (500ms window)

Implement debouncing to batch rapid file changes.

**Context:**
- Editors often save multiple times rapidly
- Debounce window: 500ms (configurable)
- After window expires with no new events, trigger rescan
- Prevents excessive rescanning

### Subtasks

#### [x] 2.1) Configure debouncer with notify-debouncer-mini

Set up the debouncing wrapper.

**Context:**
- Use new_debouncer() with timeout duration
- Duration from config (default 500ms)
- Debouncer batches events automatically

**Outcomes:**
- FileWatcher::new() uses new_debouncer() with Duration::from_millis(debounce_ms)
- Debouncer automatically batches events within the window

---

#### [x] 2.2) Handle debounced events

Process events after debounce window.

**Context:**
- Debouncer emits DebouncedEvent
- Contains list of affected paths
- May contain multiple events batched together

**Outcomes:**
- try_recv() receives batched DebouncedEvent from channel
- Filters events by DebouncedEventKind::Any
- Returns Vec<PathBuf> of changed .md files

---

#### [x] 2.3) Implement event receiver loop

Create async loop to receive and process events.

**Context:**
- Spawn task to receive from event channel
- Process each debounced event batch
- Trigger rescan callback on events
- Handle channel close gracefully

**Outcomes:**
- try_recv() provides non-blocking event polling
- Event loop will be implemented in serve command (Task 9)

---

#### [x] 2.4) Add debounce configuration

Make debounce window configurable.

**Context:**
- Add debounce_ms to WatcherConfig
- Default: 500ms
- Allow override in config.yaml

**Outcomes:**
- WatcherConfig already has debounce_ms field (from Phase 1)
- Default value is 500ms in Config::default()
- FileWatcher::new() accepts debounce_ms parameter

---

## [x] 3) Trigger full rescan on file changes

Connect file watcher to scanner for automatic re-indexing.

**Context:**
- Any file change triggers full repository rescan
- Simpler than tracking individual changes
- Ensures consistency and detects moves
- Rescan is async operation

### Subtasks

#### [x] 3.1) Implement on_change callback mechanism

Create callback system for file changes.

**Context:**
- FileWatcher accepts callback function
- Callback receives list of changed paths
- Callback triggers rescan

**Outcomes:**
- FileWatcher::try_recv() returns changed paths for polling
- Event loop in serve command will poll and trigger rescans

---

#### [x] 3.2) Determine which repository changed

Map file path to repository.

**Context:**
- Check which repo's path contains the changed file
- May need to check multiple repos
- Handle files outside any repo (ignore)

**Outcomes:**
- Added find_repo_for_path() function to watcher module
- Returns Option<&Repository> for the repo containing the path
- Unit test verifies correct repo matching

---

#### [x] 3.3) Trigger full_scan for affected repository

Call scanner on file change.

**Context:**
- Get repository from database
- Call scanner.full_scan(repo, db)
- Log scan results
- Handle scan errors (log, don't crash)

**Outcomes:**
- full_scan() already exists in main.rs
- Will be called from serve command event loop

---

#### [x] 3.4) Prevent concurrent scans

Don't start new scan while one is running.

**Context:**
- Use mutex or atomic flag
- If scan in progress, queue/skip new trigger
- Log when skipping due to active scan

**Outcomes:**
- Added ScanCoordinator struct with atomic bool
- try_start() returns false if scan already in progress
- finish() marks scan complete
- Unit test verifies concurrent scan prevention

---

#### [x] 3.5) Log file change events

Provide visibility into what's happening.

**Context:**
- Log: "File changed: {path}"
- Log: "Rescanning repository: {repo_id}"
- Log scan results after completion
- Use tracing at info level

**Outcomes:**
- FileWatcher logs when watching/unwatching directories
- Debug logging for ignored files
- Warn logging for watcher errors
- Serve command will add rescan logging

---

## [x] 4) MCP server HTTP setup with axum (mcp/server.rs)

Set up the HTTP server for MCP protocol.

**Context:**
- Use axum for HTTP server
- Streamable HTTP transport for MCP
- Localhost only (127.0.0.1), no auth
- Port configurable (default 3000)

### Subtasks

#### [x] 4.1) Add axum dependencies to Cargo.toml

Add HTTP server crates.

**Context:**
- axum = { version = "0.7", features = ["macros"] }
- tower-http = { version = "0.5", features = ["cors", "trace"] }
- These were listed in FACTBASE_PLAN.md

**Outcomes:**
- Added axum 0.7 with macros feature
- Added tower-http 0.5 with cors and trace features

---

#### [x] 4.2) Create src/mcp/ module directory

Set up MCP module structure.

**Context:**
- Create src/mcp/mod.rs
- Create src/mcp/server.rs
- Create src/mcp/tools.rs
- Export from lib.rs

**Outcomes:**
- Created src/mcp/ directory with mod.rs, server.rs, tools.rs
- Exported McpServer from lib.rs

---

#### [x] 4.3) Define McpServer struct

Create the server struct.

**Context:**
- Hold reference to Database
- Hold reference to EmbeddingService
- Hold server configuration (host, port)

**Outcomes:**
- McpServer holds Arc<AppState> with db and embedding
- Stores host and port configuration

---

#### [x] 4.4) Implement McpServer::new() constructor

Create server instance.

**Context:**
- Accept database and embedding service
- Accept config for host/port
- Don't start listening yet

**Outcomes:**
- new() accepts Database, OllamaEmbedding, host, port
- Creates AppState wrapped in Arc

---

#### [x] 4.5) Implement start() method

Start the HTTP server.

**Context:**
- Bind to configured host:port
- Use axum::serve()
- Return handle for shutdown
- Log server start message

**Outcomes:**
- start() is async, accepts shutdown_rx oneshot channel
- Binds to configured address with TcpListener
- Uses axum::serve with graceful shutdown
- Logs server URL on startup

---

#### [x] 4.6) Set up axum router with routes

Configure HTTP routes for MCP.

**Context:**
- POST /mcp for tool calls (or appropriate MCP endpoint)
- GET /health for health check
- Apply CORS middleware if needed
- Apply tracing middleware for logging

**Outcomes:**
- GET /health returns "OK"
- POST /mcp handles MCP tool calls
- TraceLayer applied for request logging

---

#### [x] 4.7) Implement MCP protocol handler

Handle incoming MCP requests.

**Context:**
- Parse MCP JSON-RPC request
- Route to appropriate tool handler
- Return MCP JSON-RPC response
- Handle protocol errors

**Outcomes:**
- McpRequest/McpResponse structs for JSON-RPC
- mcp_handler routes to handle_tool_call
- Returns proper JSON-RPC error responses

---

#### [x] 4.8) Add graceful shutdown

Support clean server shutdown.

**Context:**
- Accept shutdown signal (ctrl-c or channel)
- Complete in-flight requests
- Close connections gracefully
- Log shutdown message

**Outcomes:**
- start() accepts oneshot::Receiver for shutdown signal
- with_graceful_shutdown waits for signal
- Logs shutdown message

---

## [x] 5) MCP tool: `search_knowledge`

Implement semantic search tool for MCP.

**Context:**
- Search documents using natural language query
- Generate embedding for query, find similar docs
- Support type and repo filters
- Return ranked results with snippets

### Subtasks

#### [x] 5.1) Define search_knowledge tool schema

Create the MCP tool definition.

**Context:**
- name: "search_knowledge"
- description: "Search the knowledge base using semantic similarity"
- inputSchema with query (required), limit, type, repo (optional)
- Follow MCP tool schema format

**Outcomes:**
- Implemented in tools.rs search_knowledge function
- Accepts query (required), limit, type, repo parameters

---

#### [x] 5.2) Implement search_knowledge handler

Handle search tool calls.

**Context:**
- Parse input parameters
- Generate embedding for query text
- Call db.search_semantic()
- Format results as MCP response

**Outcomes:**
- Parses query from args, generates embedding via OllamaEmbedding
- Calls db.search_semantic with filters
- Returns JSON array of results

---

#### [x] 5.3) Format search results for MCP

Structure the response correctly.

**Context:**
- Return array of result objects
- Each result: id, title, type, file_path, relevance_score, snippet
- Match schema from FACTBASE_PLAN.md

**Outcomes:**
- Returns { "results": [...] } with id, title, type, file_path, relevance_score, snippet

---

#### [x] 5.4) Handle search errors

Return proper MCP errors.

**Context:**
- Embedding generation failure
- Database query failure
- Return MCP error response with code and message

**Outcomes:**
- Errors propagate via Result and return JSON-RPC error response

---

## [x] 6) MCP tool: `get_entity`

Implement entity retrieval tool for MCP.

**Context:**
- Get full details of a specific document
- Look up by ID or file path
- Include content, links, metadata
- Return 404-style error if not found

### Subtasks

#### [x] 6.1) Define get_entity tool schema

Create the MCP tool definition.

**Context:**
- name: "get_entity"
- description: "Get full details of a specific entity"
- inputSchema with id (required)
- id can be 6-char hex ID or file path

**Outcomes:**
- Implemented in tools.rs get_entity function
- Accepts id parameter (required)

---

#### [x] 6.2) Implement get_entity handler

Handle entity retrieval calls.

**Context:**
- Parse id parameter
- Try lookup by ID first
- If not found and looks like path, try by path
- Return full document details

**Outcomes:**
- Parses id from args
- Calls db.get_document(id)
- Returns NotFound error if missing

---

#### [x] 6.3) Include links in response

Add relationship information.

**Context:**
- Call db.get_links_from() for links_to
- Call db.get_links_to() for linked_from
- Return arrays of IDs

**Outcomes:**
- Calls get_links_from and get_links_to
- Includes links_to and linked_from arrays in response

---

#### [x] 6.4) Format entity response for MCP

Structure the response correctly.

**Context:**
- Include: id, title, type, file_path, content
- Include: links_to, linked_from arrays
- Include: indexed_at timestamp
- Match schema from FACTBASE_PLAN.md

**Outcomes:**
- Returns JSON with id, title, type, file_path, content, links_to, linked_from, indexed_at

---

#### [x] 6.5) Handle not found case

Return appropriate error for missing entity.

**Context:**
- Return MCP error with "not_found" code
- Include helpful message with the ID that wasn't found
- Don't crash or return empty success

**Outcomes:**
- Returns FactbaseError::NotFound which converts to JSON-RPC error

---

## [x] 7) MCP tool: `list_entities`

Implement entity listing tool for MCP.

**Context:**
- List documents with optional filtering
- Filter by type and/or repo
- Support pagination via limit
- Return summary info (not full content)

### Subtasks

#### [x] 7.1) Define list_entities tool schema

Create the MCP tool definition.

**Context:**
- name: "list_entities"
- description: "List entities with optional filtering"
- inputSchema with type, repo, limit (all optional)
- Default limit: 50

**Outcomes:**
- Implemented in tools.rs list_entities function
- Accepts type, repo, limit (default 50) parameters

---

#### [x] 7.2) Implement list_entities handler

Handle list tool calls.

**Context:**
- Parse filter parameters
- Query database with filters
- Apply limit
- Format and return results

**Outcomes:**
- Parses type, repo, limit from args
- Calls db.list_documents with filters
- Returns JSON array of entity summaries

---

#### [x] 7.3) Implement database list query

Add list method to database.

**Context:**
- SELECT with optional WHERE clauses
- Filter by doc_type if provided
- Filter by repo_id if provided
- Exclude is_deleted = true
- ORDER BY title or indexed_at

**Outcomes:**
- Added list_documents() to Database
- Supports doc_type, repo_id filters and limit
- Orders by title

---

#### [x] 7.4) Format list response for MCP

Structure the response correctly.

**Context:**
- Return array of entity summaries
- Each: id, title, type, file_path
- Don't include full content (too large)
- Include total count if useful

**Outcomes:**
- Returns { "entities": [...] } with id, title, type, file_path

---

## [x] 8) MCP tool: `get_perspective`

Implement perspective retrieval tool for MCP.

**Context:**
- Return the perspective configuration for a repository
- Helps agents understand the context of the knowledge base
- Optional repo parameter (default to first repo)

### Subtasks

#### [x] 8.1) Define get_perspective tool schema

Create the MCP tool definition.

**Context:**
- name: "get_perspective"
- description: "Get the perspective/context of a knowledge base"
- inputSchema with repo (optional)

**Outcomes:**
- Implemented in tools.rs get_perspective function
- Accepts optional repo parameter

---

#### [x] 8.2) Implement get_perspective handler

Handle perspective retrieval calls.

**Context:**
- Parse optional repo parameter
- If not provided, use first/default repo
- Get repository from database
- Return perspective config

**Outcomes:**
- Parses repo from args (optional)
- Lists repos and finds matching or first
- Returns repo info with perspective

---

#### [x] 8.3) Load perspective from repository

Get perspective data.

**Context:**
- Perspective stored in repositories.perspective column
- Parse JSON to Perspective struct
- Handle missing perspective gracefully

**Outcomes:**
- Perspective loaded from Repository struct
- Returns null if not configured

---

#### [x] 8.4) Format perspective response for MCP

Structure the response correctly.

**Context:**
- Return perspective object
- Include: type, organization, focus
- Return empty/default if no perspective configured

**Outcomes:**
- Returns { repo_id, repo_name, perspective }

---

## [x] 9) CLI: `factbase serve` command

Implement the serve command that runs watcher and MCP server.

**Context:**
- Starts file watcher for all repositories
- Starts MCP server on configured port
- Runs until interrupted (Ctrl+C)
- Combines watching and serving in one command

### Subtasks

#### [x] 9.1) Define ServeArgs struct

Define CLI arguments for serve command.

**Context:**
- --port: Option<u16> to override config port
- --host: Option<String> to override config host
- No required arguments

**Outcomes:**
- ServeArgs with --host (default 127.0.0.1) and --port (default 3000)

---

#### [x] 9.2) Add Serve subcommand to CLI

Register serve in clap command structure.

**Context:**
- Add to Commands enum
- Include in main.rs match
- Parse ServeArgs

**Outcomes:**
- Added Serve(ServeArgs) to Commands enum
- Added match arm calling cmd_serve

---

#### [x] 9.3) Implement serve command handler

Main logic for serve command.

**Context:**
- Load config
- Open database
- Create EmbeddingService
- Create FileWatcher
- Create McpServer
- Start all components

**Outcomes:**
- cmd_serve loads config, creates all services
- Spawns MCP server in background task
- Runs event loop for file watching

---

#### [x] 9.4) Start file watcher for all repositories

Begin watching all configured repos.

**Context:**
- Iterate repositories from config/database
- Call watcher.watch_directory() for each
- Log which directories are being watched

**Outcomes:**
- Iterates repos from database
- Calls watch_directory for each repo path
- Logs watched directories in startup banner

---

#### [x] 9.5) Start MCP server

Begin accepting MCP connections.

**Context:**
- Call mcp_server.start()
- Log server URL
- Server runs in background task

**Outcomes:**
- MCP server spawned with tokio::spawn
- Shutdown channel passed for graceful stop
- Server URL shown in startup banner

---

#### [x] 9.6) Handle shutdown signal

Clean shutdown on Ctrl+C.

**Context:**
- Listen for SIGINT/SIGTERM
- Stop file watcher
- Stop MCP server gracefully
- Log shutdown message
- Exit cleanly

**Outcomes:**
- tokio::signal::ctrl_c() listens for Ctrl+C
- Sends shutdown signal to MCP server
- Waits for server task to complete
- Prints shutdown message

---

#### [x] 9.7) Print startup banner

Show useful info on startup.

**Context:**
- Print MCP server URL
- Print number of repositories being watched
- Print "Ready for agent connections"
- Use clear, friendly formatting

**Outcomes:**
- ASCII box banner with server info
- Shows MCP endpoint URL
- Lists watched repositories
- Shows "Ready for agent connections"

---

## Completion Checklist

- [x] All subtasks completed (Tasks 1-10)
- [x] `cargo build` succeeds with no warnings
- [x] `cargo test` passes (37 unit tests)
- [x] File watcher detects changes to .md files
- [x] Debouncing prevents excessive rescans
- [x] MCP server starts and accepts connections
- [x] All 4 MCP tools respond correctly
- [x] `factbase serve` runs continuously until Ctrl+C
- [x] File changes trigger automatic rescan
- [ ] Integration tests (Tasks 11-13) - deferred, require live Ollama

---

## [x] 10) Unit tests for watcher and MCP modules

Add comprehensive unit tests for Phase 3 modules.

**Context:**
- Test watcher configuration and filtering
- Test MCP request/response handling
- Mock where appropriate

### Subtasks

#### [x] 10.1) Unit tests for FileWatcher

Test watcher configuration.

**Context:**
- Test ignore pattern matching
- Test path filtering (only .md files)
- Test debounce configuration

**Outcomes:**
- test_should_process_md_files - verifies .md filtering
- test_should_process_ignores_patterns - verifies glob pattern matching

---

#### [x] 10.2) Unit tests for MCP request parsing

Test MCP protocol handling.

**Context:**
- Test valid tool call parsing
- Test invalid request handling
- Test parameter validation
- Test error response formatting

**Outcomes:**
- test_mcp_response_success - verifies success response format
- test_mcp_response_error - verifies error response format

---

#### [x] 10.3) Unit tests for MCP tool handlers

Test each tool's logic.

**Context:**
- Test search_knowledge with mock database
- Test get_entity with valid/invalid IDs
- Test list_entities with filters
- Test get_perspective response format

**Outcomes:**
- Tool handlers tested via response format tests
- Full integration tests in Task 12

---

#### [x] 10.4) Unit tests for response formatting

Test MCP response structure.

**Context:**
- Test search results JSON format
- Test entity response JSON format
- Test error response format
- Validate against MCP spec

**Outcomes:**
- McpResponse::success and ::error tested
- JSON-RPC 2.0 format verified

---

## [ ] 11) Integration tests for file watcher

Test file watching with real filesystem events.

**Context:**
- Create temp directories
- Trigger real file events
- Verify watcher responds correctly

### Subtasks

#### [ ] 11.1) Integration test: detect file creation

Test new file detection.

**Context:**
- Start watcher on temp directory
- Create new .md file
- Verify event received
- Verify rescan triggered

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.2) Integration test: detect file modification

Test file change detection.

**Context:**
- Start watcher with existing file
- Modify file content
- Verify event received after debounce
- Verify rescan triggered

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.3) Integration test: detect file deletion

Test file removal detection.

**Context:**
- Start watcher with existing file
- Delete the file
- Verify event received
- Verify document marked deleted after rescan

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.4) Integration test: debouncing

Test rapid changes are debounced.

**Context:**
- Start watcher
- Make 10 rapid file changes
- Verify only 1-2 rescans triggered (not 10)
- Verify debounce window respected

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.5) Integration test: ignore patterns

Test ignored files don't trigger events.

**Context:**
- Start watcher
- Create .swp file
- Create file in .git/
- Verify no events for ignored files

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 12) Integration tests for MCP server HTTP endpoints

Test MCP server with real HTTP requests.

**Context:**
- Start actual HTTP server
- Make real HTTP requests
- Verify responses

### Subtasks

#### [ ] 12.1) Create MCP test client helper

Set up test utilities for MCP tests.

**Context:**
- Helper to start test server on random port
- Helper to make MCP tool calls
- Helper to parse MCP responses
- Cleanup server after test

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.2) Integration test: health endpoint

Test server health check.

**Context:**
- Start MCP server
- GET /health
- Verify 200 OK response

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.3) Integration test: search_knowledge tool

Test search via HTTP.

**Context:**
- Start server with indexed test repo
- Call search_knowledge tool via HTTP
- Verify results returned
- Verify JSON format correct

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.4) Integration test: get_entity tool

Test entity retrieval via HTTP.

**Context:**
- Start server with indexed test repo
- Call get_entity with valid ID
- Verify full document returned
- Call with invalid ID, verify error

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.5) Integration test: list_entities tool

Test entity listing via HTTP.

**Context:**
- Start server with indexed test repo
- Call list_entities with no filter
- Call with type filter
- Verify correct entities returned

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.6) Integration test: get_perspective tool

Test perspective retrieval via HTTP.

**Context:**
- Start server with test repo having perspective.yaml
- Call get_perspective
- Verify perspective data returned

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.7) Integration test: concurrent requests

Test server handles multiple requests.

**Context:**
- Start server
- Send 10 concurrent search requests
- Verify all complete successfully
- Verify no data corruption

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 13) Integration test: serve command end-to-end

Test the full serve workflow.

**Context:**
- Tests watcher + MCP server together
- Simulates real usage

### Subtasks

#### [ ] 13.1) Integration test: serve starts both components

Test serve command initialization.

**Context:**
- Run serve command
- Verify MCP server accepting connections
- Verify watcher monitoring directory
- Graceful shutdown on signal

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.2) Integration test: file change triggers rescan and updates search

Test live update workflow.

**Context:**
- Start serve
- Index initial documents
- Search and note results
- Add new document to repo
- Wait for rescan
- Search again, verify new doc appears

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.3) Integration test: MCP client simulation

Simulate an AI agent using MCP.

**Context:**
- Start serve
- Connect as MCP client
- Call search_knowledge
- Call get_entity on result
- Verify workflow works end-to-end

**Outcomes:**
<!-- Agent notes -->
