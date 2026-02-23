# Phase 1: Core Infrastructure

**Goal:** Basic file scanning and database storage

**Deliverable:** Can scan a directory, inject IDs, and store documents in SQLite

---

## [ ] 1) Project setup (Cargo.toml with dependencies)

Create the Rust project structure and configure all dependencies needed for Phase 1.

**Context:**
- Use Rust 2021 edition
- Only include dependencies needed for Phase 1 (no embedding/MCP deps yet)
- Phase 1 deps: tokio, rusqlite, serde, serde_json, serde_yaml, clap, regex, sha2, hex, walkdir, chrono, anyhow, thiserror, tracing, tracing-subscriber

### Subtasks

#### [ ] 1.1) Run `cargo init` in project root

Initialize a new Rust project in the factbase directory.

**Context:**
- Should create Cargo.toml and src/main.rs
- If files already exist, skip or handle gracefully

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.2) Configure Cargo.toml with package metadata and Phase 1 dependencies

Set up the package section and add all required dependencies for Phase 1.

**Context:**
- Package name: factbase, version: 0.1.0, edition: 2021
- Dependencies with versions from FACTBASE_PLAN.md
- tokio needs "full" feature, rusqlite needs "bundled" feature
- clap needs "derive" feature, chrono needs "serde" feature
- tracing-subscriber needs "env-filter" feature
- Include rand for ID generation, glob for ignore patterns

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.3) Create src/main.rs with minimal tokio async main

Set up the entry point with async runtime.

**Context:**
- Use #[tokio::main] attribute
- Initialize tracing subscriber for logging
- Placeholder for CLI parsing (will be expanded in task 13)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.4) Create src/lib.rs with module declarations

Set up the library root with module structure.

**Context:**
- Declare modules: config, database, error, models, processor, scanner
- Re-export key types for convenience
- Modules will be created in subsequent tasks

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 1.5) Verify project compiles with `cargo build`

Ensure the basic project structure compiles without errors.

**Context:**
- May need stub files for declared modules
- Fix any dependency resolution issues
- Warnings are okay at this stage

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 2) Error types (error.rs)

Define custom error types for the application using thiserror.

**Context:**
- Use thiserror for derive macros
- Cover: IO errors, database errors, config errors, parse errors
- Keep it simple - can extend later

### Subtasks

#### [ ] 2.1) Create src/error.rs with FactbaseError enum

Create the error module with the main error type.

**Context:**
- Use thiserror::Error derive macro
- Each variant should have a descriptive message

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.2) Add variants: Io, Database, Config, Parse, NotFound

Define error variants covering all Phase 1 failure modes.

**Context:**
- Io: file system operations
- Database: SQLite errors
- Config: configuration loading/parsing
- Parse: markdown/header parsing
- NotFound: missing files, repos, documents

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.3) Implement From traits for std::io::Error and rusqlite::Error

Enable automatic error conversion with ? operator.

**Context:**
- From<std::io::Error> for FactbaseError
- From<rusqlite::Error> for FactbaseError
- Consider From<serde_yaml::Error> for config parsing

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 2.4) Export from lib.rs

Make error types available from the crate root.

**Context:**
- pub mod error in lib.rs
- Consider pub use error::FactbaseError for convenience

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 3) Data models (models.rs)

Define the core data structures used throughout the application.

**Context:**
- Document: id, repo_id, file_path, file_hash, title, doc_type, content, timestamps, is_deleted
- Repository: id, name, path, perspective (JSON), timestamps
- ScanResult: counts for added, updated, deleted, unchanged
- Use serde for serialization

### Subtasks

#### [ ] 3.1) Create src/models.rs

Create the models module file.

**Context:**
- Will contain all data structures
- Import serde traits at top

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.2) Define Document struct with all fields from schema

Create the Document struct matching the database schema.

**Context:**
- id: String (6-char hex)
- repo_id: String
- file_path: String (relative to repo root)
- file_hash: String (SHA256)
- title: String
- doc_type: Option<String>
- content: String
- file_modified_at: Option<DateTime<Utc>>
- indexed_at: DateTime<Utc>
- is_deleted: bool

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.3) Define Repository struct

Create the Repository struct for knowledge base repositories.

**Context:**
- id: String (short identifier like "main")
- name: String (human-readable)
- path: PathBuf (filesystem path)
- perspective: Option<Perspective>
- created_at: DateTime<Utc>
- last_indexed_at: Option<DateTime<Utc>>

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.4) Define ScanResult struct for scan statistics

Create struct to hold scan operation results.

**Context:**
- added: usize
- updated: usize
- deleted: usize
- unchanged: usize
- Consider implementing Display trait for nice output

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.5) Define Perspective struct for perspective.yaml parsing

Create struct matching the perspective.yaml format.

**Context:**
- type_name: String (called "type" in YAML, rename due to reserved word)
- organization: Option<String>
- focus: Option<String>
- Use serde rename attribute for "type" field

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.6) Add serde Serialize/Deserialize derives

Enable JSON/YAML serialization for all structs.

**Context:**
- Add #[derive(Serialize, Deserialize)] to all structs
- Add #[derive(Debug, Clone)] for convenience
- Use #[serde(rename = "type")] for Perspective.type_name

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 3.7) Export from lib.rs

Make models available from crate root.

**Context:**
- pub mod models in lib.rs
- Consider re-exporting commonly used types

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 4) Configuration loading (config.rs)

Load and parse the config.yaml file with application settings.

**Context:**
- Config file location: ./config.yaml or specified via CLI
- For Phase 1: only need database path, single repository path, watcher ignore patterns
- Use serde_yaml for parsing
- Provide sensible defaults

### Subtasks

#### [ ] 4.1) Create src/config.rs

Create the configuration module file.

**Context:**
- Will contain Config struct and loading logic
- Import serde and std::path types
- Config file location: ~/.config/factbase/config.yaml (global)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.2) Define Config struct with database, repository, watcher sections

Create the top-level configuration structure.

**Context:**
- database: DatabaseConfig
- repositories: Vec<RepositoryConfig> (single item for Phase 1)
- watcher: WatcherConfig
- processor: ProcessorConfig

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.3) Define sub-structs: DatabaseConfig, RepositoryConfig, WatcherConfig

Create nested configuration structures.

**Context:**
- DatabaseConfig: path (String)
- RepositoryConfig: id, name, path
- WatcherConfig: debounce_ms, ignore_patterns (Vec<String>)
- ProcessorConfig: max_file_size, snippet_length

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.4) Implement Config::load(path) to read and parse YAML

Create the main configuration loading function.

**Context:**
- Default location: ~/.config/factbase/config.yaml
- Create config directory if it doesn't exist
- Read file contents with std::fs::read_to_string
- Parse with serde_yaml::from_str
- Return Result<Config, FactbaseError>
- Handle file not found by creating default config

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.5) Implement Default trait with sensible defaults

Provide defaults when config file is missing or incomplete.

**Context:**
- Database path: "~/.config/factbase/factbase.db"
- Ignore patterns: ["*.swp", "*.tmp", "*~", ".git/**", ".DS_Store", ".factbase/**"]
- Debounce: 500ms
- Max file size: 100000 bytes
- Empty repositories list (user adds repos)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.6) Add config validation (paths exist, etc.)

Validate configuration after loading.

**Context:**
- Check repository paths exist (warn if not)
- Ensure database directory can be created
- Validate ignore patterns are valid globs

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 4.7) Export from lib.rs

Make config types available from crate root.

**Context:**
- pub mod config in lib.rs
- Re-export Config for convenience

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 5) Database schema and initialization (database.rs)

Set up SQLite database with schema from FACTBASE_PLAN.md.

**Context:**
- Database location from config (default: ./.factbase/factbase.db)
- Create .factbase directory if needed
- Tables: repositories, documents, document_links (no embeddings table yet - Phase 2)
- Use rusqlite with bundled SQLite

### Subtasks

#### [ ] 5.1) Create src/database.rs

Create the database module file.

**Context:**
- Will contain Database struct and all DB operations
- Import rusqlite types

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.2) Define Database struct with Connection

Create the database wrapper struct.

**Context:**
- Hold rusqlite::Connection wrapped in Arc<Mutex<>>
- Use Arc<Mutex<Connection>> from the start for thread safety
- This avoids refactoring when adding file watcher + MCP server in Phase 3
- Arc allows sharing across threads, Mutex ensures exclusive access

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.3) Implement Database::new(path) - create dir, open connection

Create constructor that initializes the database.

**Context:**
- Create parent directories if they don't exist
- Open SQLite connection with rusqlite::Connection::open
- Call init_schema after opening
- Return Result<Database, FactbaseError>

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.4) Implement init_schema() - create tables if not exist

Create all required tables on first run.

**Context:**
- Use CREATE TABLE IF NOT EXISTS
- Run all table creation in a transaction
- Create indexes after tables

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.5) Write SQL for repositories table

Define the repositories table schema.

**Context:**
- id TEXT PRIMARY KEY
- name TEXT NOT NULL
- path TEXT UNIQUE NOT NULL
- perspective TEXT (JSON)
- created_at TIMESTAMP NOT NULL
- last_indexed_at TIMESTAMP

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.6) Write SQL for documents table (without embedding references)

Define the documents table schema.

**Context:**
- id TEXT PRIMARY KEY (6-char hex)
- repo_id TEXT NOT NULL with foreign key
- file_path TEXT NOT NULL
- file_hash TEXT NOT NULL
- title TEXT NOT NULL
- doc_type TEXT
- content TEXT NOT NULL
- file_modified_at TIMESTAMP
- indexed_at TIMESTAMP NOT NULL
- is_deleted BOOLEAN DEFAULT FALSE
- UNIQUE(repo_id, file_path)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.7) Write SQL for document_links table

Define the cross-reference links table schema.

**Context:**
- source_id TEXT NOT NULL
- target_id TEXT NOT NULL
- context TEXT (surrounding text snippet)
- created_at TIMESTAMP NOT NULL
- PRIMARY KEY (source_id, target_id)
- Foreign keys to documents table

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.8) Create indexes

Add indexes for common query patterns.

**Context:**
- idx_documents_repo ON documents(repo_id)
- idx_documents_type ON documents(doc_type)
- idx_documents_title ON documents(title)
- idx_documents_deleted ON documents(is_deleted)
- idx_links_source ON document_links(source_id)
- idx_links_target ON document_links(target_id)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 5.9) Export from lib.rs

Make database types available from crate root.

**Context:**
- pub mod database in lib.rs
- Re-export Database struct

**Outcomes:**
<!-- Agent notes -->



---

## [ ] 6) Database CRUD operations

Implement basic database operations for documents and repositories.

**Context:**
- All operations are synchronous (rusqlite is sync)
- Use transactions for multi-step operations
- Soft delete: set is_deleted = true, don't remove rows

### Subtasks

#### [ ] 6.1) Implement upsert_repository(&self, repo: &Repository)

Insert or update a repository record.

**Context:**
- Use INSERT OR REPLACE or INSERT ON CONFLICT
- Serialize perspective to JSON if present
- Set created_at only on insert, update last_indexed_at

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.2) Implement get_repository(&self, id: &str) -> Option<Repository>

Retrieve a repository by its ID.

**Context:**
- Query by primary key
- Deserialize perspective JSON back to struct
- Return None if not found

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.3) Implement list_repositories(&self) -> Vec<Repository>

Get all registered repositories.

**Context:**
- Simple SELECT * query
- Order by name or created_at
- Used by status command and multi-repo features

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.4) Implement upsert_document(&self, doc: &Document)

Insert or update a document record.

**Context:**
- Use INSERT OR REPLACE keyed on id
- Update indexed_at to current time
- Preserve is_deleted = false on upsert (document is active)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.5) Implement get_document(&self, id: &str) -> Option<Document>

Retrieve a document by its ID.

**Context:**
- Query by primary key
- Return None if not found or is_deleted = true
- Used by get_entity MCP tool

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.6) Implement get_document_by_path(&self, repo_id: &str, path: &str) -> Option<Document>

Find a document by its file path within a repository.

**Context:**
- Query by repo_id + file_path unique constraint
- Useful for checking if file already indexed
- Return None if not found

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.7) Implement get_documents_for_repo(&self, repo_id: &str) -> HashMap<String, Document>

Get all documents in a repository, keyed by ID.

**Context:**
- Include both active and deleted documents
- HashMap allows O(1) lookup during scan
- Used to detect deleted files (IDs not seen in scan)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.8) Implement mark_deleted(&self, id: &str)

Soft-delete a document by setting is_deleted flag.

**Context:**
- UPDATE documents SET is_deleted = true WHERE id = ?
- Don't actually remove the row
- Called when file no longer exists in filesystem

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 6.9) Implement get_stats(&self, repo_id: &str) -> RepoStats

Get statistics for a repository.

**Context:**
- Total document count
- Count by doc_type
- Active vs deleted counts
- Used by status command

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 7) Scanner: find all .md files in directory (scanner.rs)

Implement directory traversal to find all markdown files.

**Context:**
- Use walkdir crate for recursive traversal
- Filter to only .md files
- Respect ignore patterns from config (*.swp, *.tmp, .git/**, etc.)
- Return list of PathBuf for found files

### Subtasks

#### [ ] 7.1) Create src/scanner.rs

Create the scanner module file.

**Context:**
- Will contain Scanner struct and file discovery logic
- Import walkdir and glob/regex for pattern matching

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.2) Define Scanner struct with config reference

Create the scanner struct.

**Context:**
- Hold reference to ignore patterns
- May hold reference to processor config (max file size)
- Keep it simple - stateless operations

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.3) Implement find_markdown_files(&self, root: &Path) -> Vec<PathBuf>

Find all .md files under a directory.

**Context:**
- Use WalkDir::new(root) for recursive traversal
- Filter entries: is_file() and extension == "md"
- Apply ignore patterns to skip unwanted files
- Return sorted list for deterministic ordering

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.4) Add ignore pattern matching using glob

Implement pattern matching for ignore list.

**Context:**
- Patterns like "*.swp", ".git/**", ".DS_Store"
- Use glob crate for pattern matching
- Match against relative path from repo root
- Compile patterns once and reuse

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.5) Handle permission errors gracefully (log and skip)

Don't fail entire scan on permission denied.

**Context:**
- WalkDir can return errors for unreadable directories
- Log warning with tracing::warn!
- Continue scanning other directories
- Return successfully found files

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 7.6) Export from lib.rs

Make scanner available from crate root.

**Context:**
- pub mod scanner in lib.rs
- Re-export Scanner struct

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 8) Processor: extract existing factbase ID from header (processor.rs)

Parse markdown files to extract the factbase ID if present.

**Context:**
- Header format: `<!-- factbase:XXXXXX -->` where X is hex char
- Must be at the very start of the file (first line)
- Return None if no header found
- Use regex for parsing

### Subtasks

#### [ ] 8.1) Create src/processor.rs

Create the processor module file.

**Context:**
- Will contain DocumentProcessor and all parsing logic
- Import regex crate

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.2) Define DocumentProcessor struct

Create the processor struct.

**Context:**
- May hold compiled regex patterns
- Keep stateless - all state in Database
- Consider lazy_static or once_cell for regex compilation

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.3) Implement extract_id(&self, content: &str) -> Option<String>

Extract factbase ID from file content.

**Context:**
- Check first line only
- Return the 6-char hex string if found
- Return None if no valid header

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.4) Create regex pattern for `<!-- factbase:([a-f0-9]{6}) -->`

Define the header matching pattern.

**Context:**
- Capture group for the 6-char ID
- Anchor to start of string: ^
- Allow optional whitespace variations if desired
- Compile once and reuse

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.5) Add unit tests for ID extraction (with header, without, malformed)

Test the extraction logic thoroughly.

**Context:**
- Test: valid header returns Some(id)
- Test: no header returns None
- Test: malformed header (wrong length, invalid chars) returns None
- Test: header not on first line returns None

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 8.6) Export from lib.rs

Make processor available from crate root.

**Context:**
- pub mod processor in lib.rs
- Re-export DocumentProcessor

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 9) Processor: generate and inject new ID header for new files

Generate 6-char hex IDs and inject header into files without one.

**Context:**
- Generate random 6-char lowercase hex string
- Check for collisions against existing IDs in database
- Prepend header to file content and write back to disk
- Format: `<!-- factbase:XXXXXX -->\n` followed by original content

### Subtasks

#### [ ] 9.1) Implement generate_id(&self) -> String using rand or sha2

Generate a random 6-character hex ID.

**Context:**
- Use rand crate to generate 3 random bytes
- Convert to lowercase hex string
- Alternative: hash timestamp + random for uniqueness

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.2) Implement is_id_unique(&self, id: &str, db: &Database) -> bool

Check if an ID already exists in the database.

**Context:**
- Query documents table for existing ID
- Return true if not found
- Used to prevent collisions

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.3) Implement generate_unique_id(&self, db: &Database) -> String

Generate an ID guaranteed to be unique.

**Context:**
- Loop: generate_id, check is_id_unique
- Retry if collision (extremely rare with 16M possibilities)
- Add max retry limit to prevent infinite loop

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.4) Implement inject_header(&self, path: &Path, id: &str) -> Result<String>

Add factbase header to a file and return new content.

**Context:**
- Read existing file content
- Prepend `<!-- factbase:{id} -->\n`
- Return the new content (don't write yet)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.5) Write updated content back to file

Persist the header injection to disk.

**Context:**
- Use std::fs::write to overwrite file
- Consider atomic write (write to temp, rename)
- Handle write errors gracefully

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 9.6) Add unit tests for ID generation and header injection

Test ID generation and injection logic.

**Context:**
- Test: generated IDs are 6 chars, lowercase hex
- Test: injection prepends header correctly
- Test: existing content preserved after header

**Outcomes:**
<!-- Agent notes -->



---

## [ ] 10) Processor: parse title from first H1 or filename

Extract document title from content or fall back to filename.

**Context:**
- Look for first line starting with `# ` (H1 markdown)
- Title is everything after `# ` trimmed
- If no H1 found, use filename without extension
- Handle edge cases: empty file, only whitespace before H1

### Subtasks

#### [ ] 10.1) Implement extract_title(&self, content: &str, path: &Path) -> String

Extract title from content with filename fallback.

**Context:**
- Primary: find first H1 line
- Fallback: use path.file_stem()
- Always return a non-empty string

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.2) Iterate lines looking for `# ` prefix

Find the first H1 heading in the content.

**Context:**
- Skip the factbase header line if present
- Look for line starting with "# " (hash space)
- Stop at first match
- Handle lines with only whitespace before H1

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.3) Implement filename fallback using path.file_stem()

Use filename when no H1 found.

**Context:**
- path.file_stem() returns filename without extension
- Convert OsStr to String
- Handle edge case: file_stem returns None

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 10.4) Add unit tests for title extraction (H1 present, no H1, empty file)

Test title extraction thoroughly.

**Context:**
- Test: H1 on first line (after header) extracts correctly
- Test: H1 after other content still found
- Test: no H1 falls back to filename
- Test: empty file uses filename
- Test: whitespace-only file uses filename

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 11) Processor: derive type from parent folder

Determine document type from the parent directory name.

**Context:**
- `people/john.md` → type "person" (singular form)
- `projects/foo.md` → type "project"
- Files in root → type "document"
- Simple singularization: remove trailing 's' if present
- Store as-is, don't validate against any list

### Subtasks

#### [ ] 11.1) Implement derive_type(&self, path: &Path) -> String

Derive document type from file path.

**Context:**
- Get parent directory name
- Apply singularization
- Return "document" for root-level files

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.2) Get parent directory name

Extract the immediate parent folder name.

**Context:**
- Use path.parent() then file_name()
- Handle paths with no parent (root level)
- Convert OsStr to String

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.3) Implement simple singularize (remove trailing 's')

Convert plural folder names to singular type.

**Context:**
- "people" → "people" (irregular, keep as-is or handle specially)
- "projects" → "project"
- "companies" → "companie" (imperfect but acceptable)
- Keep it simple - not a full inflection library

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.4) Return "document" for files in root

Handle files not in a subdirectory.

**Context:**
- If parent is the repo root, type is "document"
- Check if parent equals repo path
- Or check if parent.file_name() is None

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 11.5) Add unit tests for type derivation

Test type derivation logic.

**Context:**
- Test: "people/john.md" → "person" or "people"
- Test: "projects/foo.md" → "project"
- Test: "notes.md" (root) → "document"
- Test: nested paths like "work/projects/foo.md"

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 12) Full scan implementation

Combine scanner and processor to perform a complete repository scan.

**Context:**
- Find all .md files
- For each file: extract/inject ID, parse title, derive type, compute hash
- Compare against database: detect new, modified, deleted
- Update database with results
- Return ScanResult with counts

### Subtasks

#### [ ] 12.1) Implement full_scan(&self, repo: &Repository, db: &Database) -> ScanResult

Main scan orchestration function.

**Context:**
- Coordinates scanner and processor
- Tracks statistics in ScanResult
- Handles errors per-file (log and continue)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.2) Get all files from scanner

Use scanner to find markdown files.

**Context:**
- Call scanner.find_markdown_files(&repo.path)
- Log count of files found
- Handle empty directory gracefully

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.3) Get all known documents from database

Load existing documents for comparison.

**Context:**
- Call db.get_documents_for_repo(&repo.id)
- Returns HashMap<id, Document>
- Includes deleted documents for tracking

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.4) For each file: read content, compute SHA256 hash

Read file and compute content hash.

**Context:**
- Use std::fs::read_to_string
- Compute SHA256 with sha2 crate
- Convert hash to hex string
- Handle read errors (log, skip file)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.5) Extract or generate ID (inject if new)

Get document ID, creating if needed.

**Context:**
- Try extract_id from content
- If None, generate_unique_id and inject_header
- Re-read content after injection

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.6) Parse title and derive type

Extract metadata from content and path.

**Context:**
- Call extract_title with content and path
- Call derive_type with path
- Both always return valid strings

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.7) Create Document struct

Build the document model.

**Context:**
- Populate all fields from extracted data
- Set indexed_at to current time
- Set is_deleted to false
- Compute relative file_path from repo root

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.8) Compare hash to detect modifications

Determine if document changed since last scan.

**Context:**
- Look up existing document by ID
- Compare file_hash values
- If different, document was modified
- If same, document unchanged

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.9) Upsert document to database

Save document to database.

**Context:**
- Call db.upsert_document
- Only upsert if new or modified (optimization)
- Handle database errors

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.10) Track seen IDs, mark unseen as deleted

Detect deleted files.

**Context:**
- Maintain HashSet of seen IDs during scan
- After processing all files, check known_docs
- Any ID not in seen set → file was deleted
- Call db.mark_deleted for each

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 12.11) Return ScanResult with counts

Return scan statistics.

**Context:**
- Populate added, updated, deleted, unchanged counts
- Log summary with tracing::info!
- Return for CLI display

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 13) CLI: `factbase init` command

Initialize a new knowledge base repository.

**Context:**
- Creates perspective.yaml template in target directory
- Creates .factbase/ directory
- Initializes database
- Adds repository to config (or creates config.yaml)

### Subtasks

#### [ ] 13.1) Set up clap CLI structure in main.rs with subcommands

Configure command-line argument parsing.

**Context:**
- Use clap derive macros
- Define Cli struct with subcommands enum
- Subcommands: Init, Scan, Status (more in later phases)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.2) Define InitArgs struct (path argument)

Define arguments for init command.

**Context:**
- path: PathBuf (required) - directory to initialize
- Optional: --name for repository name
- Optional: --id for repository ID

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.3) Implement init command handler

Main logic for init command.

**Context:**
- Validate path doesn't already have .factbase
- Create directory structure
- Initialize database
- Print success message

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.4) Create target directory if needed

Ensure the repository directory exists.

**Context:**
- Use std::fs::create_dir_all
- Handle already exists gracefully
- Check write permissions

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.5) Create perspective.yaml with template content

Generate initial perspective file.

**Context:**
- Create template with placeholder values
- Include comments explaining fields
- Don't overwrite if exists

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.6) Create .factbase/ directory

Create metadata directory.

**Context:**
- Path: {repo_path}/.factbase/
- Will contain database file
- Add to .gitignore recommendation

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.7) Initialize database

Create and set up the database.

**Context:**
- Database path: {repo_path}/.factbase/factbase.db
- Call Database::new to create and init schema
- Add repository record to database

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 13.8) Print success message

Confirm initialization to user.

**Context:**
- Print path that was initialized
- Suggest next steps (add files, run scan)
- Use colored output if terminal supports it

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 14) CLI: `factbase scan` command

Scan a repository and index all documents.

**Context:**
- Takes optional repo ID argument (default: first/only repo)
- Runs full_scan on the repository
- Prints progress and results

### Subtasks

#### [ ] 14.1) Define ScanArgs struct (optional repo argument)

Define arguments for scan command.

**Context:**
- repo: Option<String> - repository ID
- Optional: --verbose for detailed output
- If no repo specified, use default/only repo

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.2) Implement scan command handler

Main logic for scan command.

**Context:**
- Load config and find repository
- Open database
- Run full_scan
- Display results

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.3) Load config and get repository

Find the repository to scan.

**Context:**
- Load config.yaml
- If repo arg provided, find by ID
- If not provided, use first repository
- Error if no repositories configured

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.4) Open database

Connect to the database.

**Context:**
- Get database path from config
- Call Database::new
- Handle connection errors

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.5) Run full_scan

Execute the scan operation.

**Context:**
- Create Scanner and Processor
- Call full_scan with repo and db
- Handle scan errors

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 14.6) Print results (X new, Y updated, Z deleted, W unchanged)

Display scan results to user.

**Context:**
- Format ScanResult nicely
- Show counts for each category
- Include total time if desired

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 15) CLI: `factbase status` command

Show knowledge base statistics.

**Context:**
- Display repository info
- Show document counts by type
- Show total, active, deleted counts
- Show database size

### Subtasks

#### [ ] 15.1) Define StatusArgs struct (optional repo argument)

Define arguments for status command.

**Context:**
- repo: Option<String> - repository ID
- If not provided, show all repositories
- Optional: --json for machine-readable output

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.2) Implement status command handler

Main logic for status command.

**Context:**
- Load config
- Open database
- Query stats
- Format and display

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.3) Load config and get repository

Find repository or list all.

**Context:**
- If repo arg provided, show that repo only
- If not provided, show all repositories
- Handle no repositories case

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.4) Query database for stats

Get statistics from database.

**Context:**
- Call db.get_stats for each repo
- Get counts by type
- Get active vs deleted counts

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.5) Format and print status output

Display status nicely.

**Context:**
- Show repo name, path, ID
- Show document counts by type
- Show total/active/deleted
- Use indentation for readability

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 15.6) Include database file size

Show database size on disk.

**Context:**
- Get file size with std::fs::metadata
- Format as human-readable (KB, MB)
- Show path to database file

**Outcomes:**
<!-- Agent notes -->

---

## Completion Checklist

- [ ] All subtasks completed
- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes
- [ ] `factbase init ./test-kb` creates directory structure
- [ ] `factbase scan` indexes markdown files and injects IDs
- [ ] `factbase status` shows correct counts
- [ ] Code follows Rust conventions (cargo clippy clean)

---

## [ ] 16) Unit tests for core modules

Add comprehensive unit tests for all Phase 1 modules.

**Context:**
- Test each module in isolation
- Use mock data where possible
- Cover edge cases and error conditions

### Subtasks

#### [ ] 16.1) Create tests directory structure

Set up test organization.

**Context:**
- Unit tests in src/*.rs files (#[cfg(test)] mod tests)
- Integration tests in tests/ directory
- Test fixtures in tests/fixtures/

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.2) Unit tests for error.rs

Test error type conversions.

**Context:**
- Test From<std::io::Error> conversion
- Test From<rusqlite::Error> conversion
- Test error message formatting

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.3) Unit tests for models.rs

Test model serialization and defaults.

**Context:**
- Test Document serialization/deserialization
- Test Repository serialization
- Test ScanResult Display impl
- Test Perspective YAML parsing

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.4) Unit tests for config.rs

Test configuration loading and defaults.

**Context:**
- Test Config::default() values
- Test loading valid YAML
- Test loading invalid YAML (error handling)
- Test missing file creates default

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.5) Unit tests for processor.rs

Test document processing functions.

**Context:**
- Test extract_id with valid header
- Test extract_id with no header
- Test extract_id with malformed header
- Test extract_title with H1
- Test extract_title fallback to filename
- Test derive_type from folder
- Test derive_type for root files
- Test generate_id format (6 hex chars)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.6) Unit tests for scanner.rs

Test file discovery and filtering.

**Context:**
- Test find_markdown_files finds .md files
- Test ignore patterns filter correctly
- Test nested directories traversed
- Test non-.md files excluded

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 16.7) Unit tests for database.rs

Test database operations.

**Context:**
- Test schema creation
- Test upsert_document
- Test get_document
- Test mark_deleted
- Test get_stats
- Use in-memory SQLite for tests

**Outcomes:**
<!-- Agent notes -->

---

## [ ] 17) Integration tests with test fixtures

Create integration tests using real file system operations.

**Context:**
- Create temporary directories with test markdown files
- Test full scan workflow end-to-end
- Verify database state after operations

### Subtasks

#### [ ] 17.1) Create test fixture markdown files

Set up reusable test data.

**Context:**
- Create tests/fixtures/sample-repo/
- Add people/john-doe.md, projects/test-project.md
- Include files with and without headers
- Include files with various title formats

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 17.2) Integration test: init command

Test repository initialization end-to-end.

**Context:**
- Create temp directory
- Run init command
- Verify perspective.yaml created
- Verify .factbase/ directory created
- Verify database initialized
- Clean up temp directory

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 17.3) Integration test: scan command

Test full scan workflow.

**Context:**
- Copy fixture files to temp directory
- Run init then scan
- Verify documents in database
- Verify IDs injected into files
- Verify titles extracted correctly
- Verify types derived from folders

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 17.4) Integration test: incremental scan

Test scanning after file changes.

**Context:**
- Initial scan
- Modify a file
- Add a new file
- Delete a file
- Run scan again
- Verify correct counts (added, updated, deleted, unchanged)

**Outcomes:**
<!-- Agent notes -->

---

#### [ ] 17.5) Integration test: status command

Test status output accuracy.

**Context:**
- Scan a test repo
- Run status command
- Verify document counts match
- Verify type breakdown correct
- Verify database size reported

**Outcomes:**
<!-- Agent notes -->
