# Phase 4: Multi-Repo & Polish

**Goal:** Production-ready system with multi-repository support

**Deliverable:** Stable, documented, multi-repository system

---

## [ ] 1) Multi-repository support in database layer

Extend database operations to fully support multiple repositories.

**Context:**
- Database schema already has repo_id on documents
- Need to ensure all queries respect repo boundaries
- Support listing and managing multiple repos
- Handle cross-repo scenarios appropriately

### Subtasks

#### [ ] 1.1) Review and update all document queries for repo_id

Ensure repo isolation in all queries.

**Context:**
- Check every SELECT includes repo_id filter where appropriate
- Some queries (by ID) may not need repo filter
- Search queries should support optional repo filter

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.2) Implement add_repository(&self, repo: &Repository)

Add a new repository to the database.

**Context:**
- INSERT into repositories table
- Set created_at to current time
- Validate repo ID is unique
- Return error if path already registered

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.3) Implement remove_repository(&self, id: &str)

Remove a repository from tracking.

**Context:**
- DELETE from repositories WHERE id = ?
- Decide: also delete documents? Or mark inactive?
- Recommendation: soft-delete documents (mark is_deleted)
- Log warning about orphaned documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.4) Implement get_repository_by_path(&self, path: &Path) -> Option<Repository>

Find repository by filesystem path.

**Context:**
- SELECT * FROM repositories WHERE path = ?
- Useful for mapping file changes to repos
- Handle path normalization

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.5) Update list_repositories to include stats

Enhance repo listing with document counts.

**Context:**
- Include document count per repo
- Include last_indexed_at
- Use JOIN or subquery for counts

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.6) Handle repository path changes

Support updating a repository's path.

**Context:**
- Implement update_repository_path()
- Update all document file_paths? Or require rescan?
- Recommendation: require rescan after path change

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 2) Multi-repository support in scanner/processor

Update scanning to work with multiple repositories.

**Context:**
- Scanner should handle any repository
- Processor should tag documents with correct repo_id
- Support scanning individual repos or all repos

### Subtasks

#### [ ] 2.1) Update full_scan to accept repository parameter

Ensure scan is scoped to one repository.

**Context:**
- Already accepts Repository parameter
- Verify all operations use repo.id
- Documents created with correct repo_id

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.2) Implement scan_all_repositories()

Scan all registered repositories.

**Context:**
- Get list of all repositories from database
- Call full_scan for each
- Aggregate results
- Handle individual repo failures gracefully

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.3) Update file watcher for multiple repos

Watch all repository directories.

**Context:**
- On startup, watch all registered repos
- When repo added, start watching
- When repo removed, stop watching
- Map file events to correct repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.4) Handle file events across repos

Route events to correct repository.

**Context:**
- When file changes, determine which repo
- Check file path against all repo paths
- Trigger scan for correct repo only
- Handle files outside any repo (ignore)

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 3) Multi-repository support in MCP tools

Update MCP tools to work across multiple repositories.

**Context:**
- search_knowledge: search across all or filter by repo
- get_entity: find in any repo
- list_entities: filter by repo
- get_perspective: specify which repo

### Subtasks

#### [ ] 3.1) Update search_knowledge for multi-repo

Support searching across or within repos.

**Context:**
- If repo param provided, filter to that repo
- If not provided, search all repos
- Include repo_id in results

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.2) Update get_entity for multi-repo

Find entity in any repository.

**Context:**
- Search by ID across all repos (IDs are globally unique)
- If searching by path, may need repo context
- Include repo_id in response

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.3) Update list_entities for multi-repo

Support listing across or within repos.

**Context:**
- If repo param provided, filter to that repo
- If not provided, list from all repos
- Include repo_id in each result

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.4) Add list_repositories MCP tool

New tool to list available repositories.

**Context:**
- name: "list_repositories"
- Returns all registered repos with stats
- Helps agents discover available knowledge bases

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 4) CLI: `factbase repo add <id> <path>` command

Implement command to add a new repository.

**Context:**
- Register a new directory as a knowledge base
- Assign ID and optional name
- Initialize perspective.yaml if not exists
- Don't scan automatically (user runs scan separately)

### Subtasks

#### [ ] 4.1) Define RepoAddArgs struct

Define CLI arguments for repo add.

**Context:**
- id: String (required) - short identifier
- path: PathBuf (required) - directory path
- --name: Option<String> - human-readable name

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.2) Add RepoAdd subcommand to CLI

Register in clap command structure.

**Context:**
- Add to Commands enum as Repo(RepoCommand)
- RepoCommand has Add, Remove, List subcommands
- Nested subcommand structure

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.3) Implement repo add handler

Main logic for adding repository.

**Context:**
- Validate path exists and is directory
- Check ID not already used
- Check path not already registered
- Add to database
- Create perspective.yaml if missing

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.4) Validate repository path

Ensure path is valid for a repository.

**Context:**
- Path must exist
- Path must be a directory
- Path should be readable
- Warn if path is inside another repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.5) Print success and next steps

Confirm addition to user.

**Context:**
- Print: "Added repository: {id}"
- Print: "Path: {path}"
- Suggest: "Run 'factbase scan {id}' to index documents"

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 5) CLI: `factbase repo remove <id>` command

Implement command to remove a repository.

**Context:**
- Unregister a repository from tracking
- Keep documents in database (marked inactive) or delete?
- Don't delete actual files on disk

### Subtasks

#### [ ] 5.1) Define RepoRemoveArgs struct

Define CLI arguments for repo remove.

**Context:**
- id: String (required) - repository to remove
- --force: bool - skip confirmation prompt

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.2) Add RepoRemove subcommand to CLI

Register in clap command structure.

**Context:**
- Add to RepoCommand enum
- Parse RepoRemoveArgs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.3) Implement repo remove handler

Main logic for removing repository.

**Context:**
- Verify repository exists
- Prompt for confirmation (unless --force)
- Remove from database
- Stop watching directory
- Print confirmation

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.4) Handle documents on removal

Decide what to do with indexed documents.

**Context:**
- Option 1: Delete documents from database
- Option 2: Mark documents as deleted (soft delete)
- Option 3: Keep documents but mark repo inactive
- Recommendation: soft delete documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.5) Add confirmation prompt

Require user confirmation before removal.

**Context:**
- Print: "Remove repository '{id}' with X documents?"
- Wait for y/n input
- Skip if --force flag provided
- Cancel on 'n' or Ctrl+C

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 6) Update `factbase status` for multi-repo display

Enhance status command to show all repositories.

**Context:**
- Show summary of all repositories
- Show per-repo statistics
- Show overall totals
- Indicate which repos are being watched

### Subtasks

#### [ ] 6.1) Update status to list all repositories

Show all registered repos.

**Context:**
- Query all repositories from database
- Display each with ID, name, path
- Show document counts per repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.2) Show per-repository statistics

Display detailed stats for each repo.

**Context:**
- Document count (total, active, deleted)
- Count by document type
- Last indexed timestamp
- Link count

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.3) Show overall summary

Display aggregate statistics.

**Context:**
- Total repositories
- Total documents across all repos
- Total links
- Database file size

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.4) Indicate watcher status

Show if repos are being watched.

**Context:**
- Only relevant when serve is running
- Could show "watching" vs "not watching"
- Or skip if status is run standalone

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.5) Add repo list subcommand

Dedicated command to list repos.

**Context:**
- `factbase repo list`
- Simpler output than full status
- Just repos with basic info

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 7) Comprehensive error handling and logging

Improve error handling and add structured logging throughout.

**Context:**
- Use tracing for structured logging
- Consistent error messages
- Proper error propagation
- Log levels: error, warn, info, debug, trace

### Subtasks

#### [ ] 7.1) Review all error handling paths

Audit error handling throughout codebase.

**Context:**
- Check all Result returns are handled
- Ensure errors have context
- Use anyhow context() for better messages
- No silent failures

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.2) Add tracing spans for operations

Instrument key operations with spans.

**Context:**
- Span for each scan operation
- Span for each MCP request
- Span for file processing
- Include relevant fields (repo_id, file_path, etc.)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.3) Configure log levels appropriately

Set appropriate levels for different messages.

**Context:**
- error: failures that need attention
- warn: recoverable issues
- info: normal operation milestones
- debug: detailed operation info
- trace: very verbose debugging

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.4) Add --verbose flag to CLI

Allow users to increase log verbosity.

**Context:**
- Default: info level
- -v: debug level
- -vv: trace level
- Apply to tracing subscriber

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.5) Improve error messages for users

Make errors actionable.

**Context:**
- Include what went wrong
- Include what user can do to fix
- Include relevant context (file path, repo, etc.)
- Avoid technical jargon where possible

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 8) Performance optimization for large repositories

Optimize for repositories with many documents.

**Context:**
- Target: handle 1000+ documents efficiently
- Scan should complete in reasonable time
- Search should be fast (<100ms)
- Memory usage should be bounded

### Subtasks

#### [ ] 8.1) Profile scan performance

Measure current scan performance.

**Context:**
- Time full scan with various repo sizes
- Identify bottlenecks
- Measure: file reading, hashing, embedding, DB writes

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.2) Optimize database operations

Improve database performance.

**Context:**
- Use transactions for batch operations
- Consider prepared statements
- Add missing indexes if needed
- Use WAL mode for better concurrency

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.3) Skip unchanged files efficiently

Avoid reprocessing unchanged documents.

**Context:**
- Check file hash before full processing
- Skip embedding generation if unchanged
- Only update DB if something changed

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.4) Consider parallel processing

Evaluate parallelizing scan operations.

**Context:**
- File reading could be parallel
- Embedding generation is I/O bound (API calls)
- DB writes should be serialized
- Use tokio tasks or rayon

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.5) Add progress indication for long operations

Show progress during lengthy scans.

**Context:**
- Print progress: "Processing 45/100 files..."
- Update periodically, not every file
- Show elapsed time
- Consider progress bar library

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 9) Example knowledge base in examples/

Create a sample knowledge base for testing and demonstration.

**Context:**
- Provide realistic example content
- Demonstrate different document types
- Show cross-references between documents
- Include perspective.yaml

### Subtasks

#### [ ] 9.1) Create examples/sample-knowledge-base/ directory

Set up the example directory structure.

**Context:**
- Create examples/ in project root
- Create sample-knowledge-base/ subdirectory
- Add to .gitignore if generated content

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.2) Create perspective.yaml

Define the example knowledge base context.

**Context:**
- type: "developer"
- organization: "Example Corp"
- focus: "team knowledge, projects, documentation"

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.3) Create people/ directory with sample documents

Add example person documents.

**Context:**
- Create 3-5 person documents
- Include varied roles (engineer, manager, designer)
- Add cross-references to projects
- Use realistic but fictional content

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.4) Create projects/ directory with sample documents

Add example project documents.

**Context:**
- Create 2-3 project documents
- Reference team members
- Include status, timeline, objectives
- Show different project states

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.5) Create other document types

Add variety to the example.

**Context:**
- Maybe: companies/, concepts/, notes/
- Show flexibility of the system
- Keep total to ~10 documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.6) Add README for examples

Document how to use the examples.

**Context:**
- Explain what the example contains
- Show commands to init and scan
- Demonstrate search queries

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 10) README documentation

Create comprehensive project documentation.

**Context:**
- Main README.md in project root
- Explain what factbase is and does
- Installation and setup instructions
- Usage examples for all commands
- Configuration reference

### Subtasks

#### [ ] 10.1) Write project overview

Explain what factbase is.

**Context:**
- One-paragraph summary
- Key features list
- Comparison to alternatives (brief)
- Link to detailed docs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.2) Write installation instructions

How to install factbase.

**Context:**
- Prerequisites (Rust, AWS credentials)
- Build from source instructions
- cargo install if published
- Verify installation

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.3) Write quick start guide

Get users running quickly.

**Context:**
- Initialize a knowledge base
- Add some markdown files
- Run scan
- Try search
- Start serve

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.4) Document all CLI commands

Reference for each command.

**Context:**
- factbase init
- factbase scan
- factbase search
- factbase serve
- factbase status
- factbase repo add/remove/list

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.5) Document configuration options

Reference for config.yaml.

**Context:**
- All configuration sections
- All options with defaults
- Environment variable overrides
- Example config file

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.6) Document MCP integration

How to use with AI agents.

**Context:**
- MCP server setup
- Available tools and their schemas
- Example agent configurations
- Troubleshooting tips

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.7) Add architecture overview

Technical documentation.

**Context:**
- High-level architecture diagram
- Component descriptions
- Data flow explanation
- Link to FACTBASE_PLAN.md for details

**Outcomes:**
<!-- Agent notes -->

---

## Completion Checklist

- [ ] All subtasks completed
- [ ] `cargo build --release` succeeds
- [ ] `cargo test` passes (all unit + integration tests)
- [ ] `cargo clippy` has no warnings
- [ ] Multiple repositories can be added and managed
- [ ] `factbase repo add/remove/list` work correctly
- [ ] `factbase status` shows all repos with stats
- [ ] MCP tools work across multiple repos
- [ ] Example knowledge base is complete and works
- [ ] README documentation is comprehensive
- [ ] Performance acceptable with 1000+ documents

---

## [ ] 11) Unit tests for multi-repo functionality

Add unit tests for multi-repository features.

**Context:**
- Test repo isolation
- Test cross-repo queries
- Test repo management operations

### Subtasks

#### [ ] 11.1) Unit tests for repository CRUD

Test repository database operations.

**Context:**
- Test add_repository
- Test remove_repository
- Test get_repository_by_path
- Test list_repositories with stats

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.2) Unit tests for repo-scoped document queries

Test document queries respect repo boundaries.

**Context:**
- Test get_documents_for_repo only returns that repo's docs
- Test search with repo filter
- Test stats are per-repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.3) Unit tests for MCP multi-repo tools

Test MCP tools with repo parameter.

**Context:**
- Test search_knowledge with repo filter
- Test list_entities with repo filter
- Test list_repositories tool

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 12) Integration tests for multi-repo workflows

Test multi-repository scenarios end-to-end.

**Context:**
- Create multiple test repositories
- Test isolation and cross-repo features

### Subtasks

#### [ ] 12.1) Integration test: add multiple repositories

Test adding several repos.

**Context:**
- Create two temp directories with test files
- Run repo add for each
- Verify both in database
- Verify both in status output

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.2) Integration test: scan specific repository

Test scanning one repo doesn't affect others.

**Context:**
- Add two repos
- Scan only first repo
- Verify second repo unchanged
- Scan second repo
- Verify both now indexed

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.3) Integration test: search across repos

Test cross-repo search.

**Context:**
- Add two repos with different content
- Search without repo filter
- Verify results from both repos
- Search with repo filter
- Verify only that repo's results

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.4) Integration test: remove repository

Test repo removal workflow.

**Context:**
- Add repo and scan
- Remove repo
- Verify repo not in list
- Verify documents soft-deleted
- Verify search excludes removed repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.5) Integration test: file watcher multiple repos

Test watching multiple directories.

**Context:**
- Start serve with two repos
- Modify file in first repo
- Verify only first repo rescanned
- Modify file in second repo
- Verify only second repo rescanned

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 13) End-to-end system tests

Comprehensive tests of the complete system.

**Context:**
- Test realistic usage scenarios
- Verify all components work together

### Subtasks

#### [ ] 13.1) E2E test: new user workflow

Test complete new user experience.

**Context:**
- Start with no config
- Run init to create repo
- Add some markdown files
- Run scan
- Run search
- Verify everything works

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.2) E2E test: agent workflow simulation

Simulate AI agent using factbase.

**Context:**
- Start serve
- Connect via MCP
- Search for information
- Get entity details
- Follow links to related entities
- Verify complete workflow

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.3) E2E test: continuous operation

Test long-running stability.

**Context:**
- Start serve
- Make periodic file changes over time
- Make periodic MCP requests
- Run for extended period (5+ minutes)
- Verify no memory leaks or crashes
- Verify all operations still work

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 14) Performance and stress tests

Test system under load.

**Context:**
- Verify performance targets met
- Identify scaling limits

### Subtasks

#### [ ] 14.1) Performance test: 1000 document repository

Test with large repository.

**Context:**
- Generate 1000 test markdown files
- Time full scan
- Time search queries
- Verify acceptable performance

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.2) Performance test: concurrent MCP requests

Test server under load.

**Context:**
- Start serve
- Send 100 concurrent search requests
- Measure response times
- Verify no failures
- Verify p99 latency acceptable

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.3) Performance test: rapid file changes

Test watcher under stress.

**Context:**
- Start serve
- Make 100 file changes in quick succession
- Verify debouncing works
- Verify system remains stable
- Verify final state correct

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.4) Memory usage test

Test memory doesn't grow unbounded.

**Context:**
- Start serve
- Monitor memory usage
- Perform many operations
- Verify memory stable over time
- No significant leaks

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 15) Test documentation and CI setup

Document testing and set up automation.

**Context:**
- Make tests easy to run
- Enable CI/CD

### Subtasks

#### [ ] 15.1) Document test requirements

Write testing documentation.

**Context:**
- Document Ollama requirement for integration tests
- Document how to run unit tests only
- Document how to run integration tests
- Document test fixtures

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.2) Create test runner scripts

Add convenience scripts.

**Context:**
- Script to run unit tests only
- Script to run integration tests (checks Ollama first)
- Script to run all tests
- Script to run performance tests

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.3) Add CI configuration

Set up GitHub Actions or similar.

**Context:**
- Run unit tests on every PR
- Run clippy and fmt checks
- Integration tests as optional/manual
- Build release artifacts

**Outcomes:**
<!-- Agent notes -->
