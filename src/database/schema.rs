//! Database schema initialization and migrations.
//!
//! This module handles:
//! - Schema creation (tables, indexes, virtual tables)
//! - Schema version tracking via PRAGMA user_version
//! - Database migrations for schema evolution
//!
//! # Schema Version
//!
//! The current schema version is tracked in [`SCHEMA_VERSION`].
//! Migrations are defined in [`MIGRATIONS`] and run automatically
//! when the database is opened.

use super::{Database, DbConn};
use crate::error::FactbaseError;

/// Current schema version. Increment when adding migrations.
pub(super) const SCHEMA_VERSION: i32 = 20;

/// Database migrations. Each entry is (version, description, sql).
/// Migrations are run in order for versions > current user_version.
/// Version 1 is the baseline schema (created by init_schema).
pub(super) const MIGRATIONS: &[(i32, &str, &str)] = &[
    // Version 2: Add last_lint_at column (renamed to last_check_at in v8, restored in v17)
    (
        2,
        "Add last_lint_at to repositories",
        "ALTER TABLE repositories ADD COLUMN last_lint_at TIMESTAMP;",
    ),
    // Version 3: Add index on file_modified_at for --since filter performance
    (
        3,
        "Add index on file_modified_at",
        "CREATE INDEX IF NOT EXISTS idx_documents_modified ON documents(file_modified_at);",
    ),
    // Version 4: Add word_count column to avoid decompressing content for stats
    (
        4,
        "Add word_count to documents",
        "ALTER TABLE documents ADD COLUMN word_count INTEGER;",
    ),
    // Version 5: Add cross_check_hash for cross-validation skip tracking
    (
        5,
        "Add cross_check_hash to documents",
        "ALTER TABLE documents ADD COLUMN cross_check_hash TEXT;",
    ),
    // Version 6: Add has_review_queue flag to avoid full-scan when listing review questions
    (
        6,
        "Add has_review_queue to documents",
        "ALTER TABLE documents ADD COLUMN has_review_queue BOOLEAN DEFAULT FALSE;",
    ),
    // Version 7: FTS5 virtual table for full-text content search
    (
        7,
        "Add FTS5 full-text search index",
        "CREATE VIRTUAL TABLE IF NOT EXISTS document_content_fts USING fts5(doc_id UNINDEXED, content);",
    ),
    // Version 8: Rename last_lint_at → last_check_at (reverted in v17)
    (
        8,
        "Rename last_lint_at to last_check_at",
        "ALTER TABLE repositories RENAME COLUMN last_lint_at TO last_check_at;",
    ),
    // Version 9: Persistent query embedding cache
    (
        9,
        "Add query_embedding_cache table",
        "CREATE TABLE IF NOT EXISTS query_embedding_cache (
            text_hash TEXT NOT NULL,
            model TEXT NOT NULL,
            text TEXT NOT NULL,
            dimension INTEGER NOT NULL,
            embedding BLOB NOT NULL,
            created_at TIMESTAMP NOT NULL,
            last_used_at TIMESTAMP NOT NULL,
            PRIMARY KEY (text_hash, model)
        );
        CREATE INDEX IF NOT EXISTS idx_query_cache_last_used ON query_embedding_cache(last_used_at);",
    ),
    // Version 10: Fact-level embeddings for cross-validation
    (
        10,
        "Add fact_embeddings and fact_metadata tables",
        "CREATE VIRTUAL TABLE IF NOT EXISTS fact_embeddings USING vec0(
            id TEXT PRIMARY KEY,
            embedding FLOAT[1024]
        );
        CREATE TABLE IF NOT EXISTS fact_metadata (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            line_number INTEGER NOT NULL,
            fact_text TEXT NOT NULL,
            fact_hash TEXT NOT NULL,
            FOREIGN KEY (document_id) REFERENCES documents(id)
        );
        CREATE INDEX IF NOT EXISTS idx_fact_meta_doc ON fact_metadata(document_id);",
    ),
    // Version 11: Server-side cross-validation cursor
    (
        11,
        "Add cross_validation_state table",
        "CREATE TABLE IF NOT EXISTS cross_validation_state (
            scope_key TEXT PRIMARY KEY,
            pair_offset INTEGER NOT NULL DEFAULT 0,
            fact_count INTEGER NOT NULL DEFAULT 0,
            updated_at TIMESTAMP NOT NULL
        );",
    ),
    // Version 12: Lock/lease columns for cross-validation concurrency
    // SQL is empty — handled in post-migration hook to be idempotent
    (
        12,
        "Add lock columns to cross_validation_state",
        "",
    ),
    // Version 13: Cached fact pairs to avoid O(n²) recomputation
    (
        13,
        "Add cached_fact_pairs tables",
        "CREATE TABLE IF NOT EXISTS cached_fact_pairs (
            scope TEXT NOT NULL,
            fact_a_id TEXT NOT NULL,
            fact_a_doc_id TEXT NOT NULL,
            fact_a_line INTEGER NOT NULL,
            fact_a_text TEXT NOT NULL,
            fact_b_id TEXT NOT NULL,
            fact_b_doc_id TEXT NOT NULL,
            fact_b_line INTEGER NOT NULL,
            fact_b_text TEXT NOT NULL,
            similarity REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_cached_pairs_scope ON cached_fact_pairs(scope);
        CREATE TABLE IF NOT EXISTS cached_fact_pairs_meta (
            scope TEXT PRIMARY KEY,
            fact_count INTEGER NOT NULL,
            threshold REAL NOT NULL,
            limit_per_fact INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );",
    ),
    // Version 14: Embedding metadata for dimension/model tracking
    (
        14,
        "Add embedding_metadata table",
        "CREATE TABLE IF NOT EXISTS embedding_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    ),
    // Version 15: Organization suggestions for deferred move/rename/title operations
    (
        15,
        "Add organization_suggestions table",
        "CREATE TABLE IF NOT EXISTS organization_suggestions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            doc_id TEXT NOT NULL,
            suggestion_type TEXT NOT NULL,
            suggested_value TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'update',
            created_at TIMESTAMP NOT NULL,
            FOREIGN KEY (doc_id) REFERENCES documents(id)
        );
        CREATE INDEX IF NOT EXISTS idx_org_suggestions_doc ON organization_suggestions(doc_id);",
    ),
    // Version 16: Review questions table for fast indexed access
    (
        16,
        "Add review_questions table",
        "CREATE TABLE IF NOT EXISTS review_questions (
            id INTEGER PRIMARY KEY,
            doc_id TEXT NOT NULL,
            question_index INTEGER NOT NULL,
            question_type TEXT NOT NULL,
            description TEXT NOT NULL,
            line_ref INTEGER,
            answer TEXT,
            status TEXT NOT NULL DEFAULT 'open',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (doc_id) REFERENCES documents(id),
            UNIQUE(doc_id, question_index)
        );
        CREATE INDEX IF NOT EXISTS idx_rq_doc ON review_questions(doc_id);
        CREATE INDEX IF NOT EXISTS idx_rq_status ON review_questions(status);
        CREATE INDEX IF NOT EXISTS idx_rq_type ON review_questions(question_type);",
    ),
    // Version 17: Rename last_check_at back to last_lint_at
    // SQL is empty — handled in post-migration hook to be idempotent
    (
        17,
        "Rename last_check_at to last_lint_at",
        "",
    ),
    // Version 18: Composite index on review_questions(doc_id, status) for status-filtered queries
    (
        18,
        "Add composite index on review_questions(doc_id, status)",
        "CREATE INDEX IF NOT EXISTS idx_review_questions_doc_status ON review_questions(doc_id, status);",
    ),
    // Version 19: Track review section hash separately for review-only change detection
    (
        19,
        "Add review_section_hash to documents",
        "", // handled in post-migration hook (idempotent column add)
    ),
    // Version 20: Reset legacy 'believed' status → 'open' (confidence level removed)
    (
        20,
        "Reset believed review questions to open",
        "UPDATE review_questions SET status = 'open', answer = NULL WHERE status = 'believed';",
    ),
];

impl Database {
    /// Initialize the database schema.
    ///
    /// Creates all required tables, indexes, and virtual tables.
    /// Also runs any pending migrations.
    pub(super) fn init_schema(&self) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT UNIQUE NOT NULL,
                perspective TEXT,
                created_at TIMESTAMP NOT NULL,
                last_indexed_at TIMESTAMP,
                last_lint_at TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                title TEXT NOT NULL,
                doc_type TEXT,
                content TEXT NOT NULL,
                file_modified_at TIMESTAMP,
                indexed_at TIMESTAMP NOT NULL,
                is_deleted BOOLEAN DEFAULT FALSE,
                word_count INTEGER,
                cross_check_hash TEXT,
                has_review_queue BOOLEAN DEFAULT FALSE,
                review_section_hash TEXT,
                UNIQUE(repo_id, file_path),
                FOREIGN KEY (repo_id) REFERENCES repositories(id)
            );
            CREATE TABLE IF NOT EXISTS document_links (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                context TEXT,
                created_at TIMESTAMP NOT NULL,
                PRIMARY KEY (source_id, target_id),
                FOREIGN KEY (source_id) REFERENCES documents(id),
                FOREIGN KEY (target_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_documents_repo ON documents(repo_id);
            CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(doc_type);
            CREATE INDEX IF NOT EXISTS idx_documents_title ON documents(title);
            CREATE INDEX IF NOT EXISTS idx_documents_deleted ON documents(is_deleted);
            CREATE INDEX IF NOT EXISTS idx_documents_modified ON documents(file_modified_at);
            CREATE INDEX IF NOT EXISTS idx_links_source ON document_links(source_id);
            CREATE INDEX IF NOT EXISTS idx_links_target ON document_links(target_id);",
        )?;

        // Create virtual table for embeddings
        // Check if we need to migrate from 768 to 1024 dimensions or old schema
        let needs_migration = Self::check_embedding_migration(&conn)?;
        if needs_migration {
            tracing::warn!("Migrating embeddings schema. Full rescan required.");
            conn.execute("DROP TABLE IF EXISTS document_embeddings", [])?;
            conn.execute("DROP TABLE IF EXISTS embedding_chunks", [])?;
        }

        // Embedding vectors - id is "{doc_id}_{chunk_index}"
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS document_embeddings USING vec0(
                id TEXT PRIMARY KEY,
                embedding FLOAT[1024]
            )",
            [],
        )?;

        // Chunk metadata (vec0 doesn't support extra columns)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embedding_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                chunk_start INTEGER NOT NULL,
                chunk_end INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_doc ON embedding_chunks(document_id);",
        )?;

        // FTS5 full-text search index for content search optimization
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS document_content_fts USING fts5(doc_id UNINDEXED, content)",
            [],
        )?;

        // Fact-level embeddings for cross-validation
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS fact_embeddings USING vec0(
                id TEXT PRIMARY KEY,
                embedding FLOAT[1024]
            )",
            [],
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fact_metadata (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                line_number INTEGER NOT NULL,
                fact_text TEXT NOT NULL,
                fact_hash TEXT NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_fact_meta_doc ON fact_metadata(document_id);",
        )?;

        // Persistent query embedding cache
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS query_embedding_cache (
                text_hash TEXT NOT NULL,
                model TEXT NOT NULL,
                text TEXT NOT NULL,
                dimension INTEGER NOT NULL,
                embedding BLOB NOT NULL,
                created_at TIMESTAMP NOT NULL,
                last_used_at TIMESTAMP NOT NULL,
                PRIMARY KEY (text_hash, model)
            );
            CREATE INDEX IF NOT EXISTS idx_query_cache_last_used ON query_embedding_cache(last_used_at);",
        )?;

        // Embedding metadata (model name, dimension) for mismatch detection
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embedding_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        // Organization suggestions for deferred move/rename/title operations
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS organization_suggestions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                doc_id TEXT NOT NULL,
                suggestion_type TEXT NOT NULL,
                suggested_value TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'update',
                created_at TIMESTAMP NOT NULL,
                FOREIGN KEY (doc_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_org_suggestions_doc ON organization_suggestions(doc_id);",
        )?;

        // Server-side cross-validation cursor
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cross_validation_state (
                scope_key TEXT PRIMARY KEY,
                pair_offset INTEGER NOT NULL DEFAULT 0,
                fact_count INTEGER NOT NULL DEFAULT 0,
                updated_at TIMESTAMP NOT NULL,
                locked_by TEXT,
                locked_at TEXT
            );",
        )?;

        // Cached fact pairs to avoid O(n²) recomputation
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cached_fact_pairs (
                scope TEXT NOT NULL,
                fact_a_id TEXT NOT NULL,
                fact_a_doc_id TEXT NOT NULL,
                fact_a_line INTEGER NOT NULL,
                fact_a_text TEXT NOT NULL,
                fact_b_id TEXT NOT NULL,
                fact_b_doc_id TEXT NOT NULL,
                fact_b_line INTEGER NOT NULL,
                fact_b_text TEXT NOT NULL,
                similarity REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cached_pairs_scope ON cached_fact_pairs(scope);
            CREATE TABLE IF NOT EXISTS cached_fact_pairs_meta (
                scope TEXT PRIMARY KEY,
                fact_count INTEGER NOT NULL,
                threshold REAL NOT NULL,
                limit_per_fact INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );",
        )?;

        // Review questions table for fast indexed access
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS review_questions (
                id INTEGER PRIMARY KEY,
                doc_id TEXT NOT NULL,
                question_index INTEGER NOT NULL,
                question_type TEXT NOT NULL,
                description TEXT NOT NULL,
                line_ref INTEGER,
                answer TEXT,
                status TEXT NOT NULL DEFAULT 'open',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (doc_id) REFERENCES documents(id),
                UNIQUE(doc_id, question_index)
            );
            CREATE INDEX IF NOT EXISTS idx_rq_doc ON review_questions(doc_id);
            CREATE INDEX IF NOT EXISTS idx_rq_status ON review_questions(status);
            CREATE INDEX IF NOT EXISTS idx_rq_type ON review_questions(question_type);",
        )?;

        // Run any pending migrations
        Self::run_migrations(&conn)?;

        Ok(())
    }

    /// Get current schema version from database
    fn get_schema_version(conn: &DbConn) -> Result<i32, FactbaseError> {
        let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(version)
    }

    /// Set schema version in database
    fn set_schema_version(conn: &DbConn, version: i32) -> Result<(), FactbaseError> {
        // PRAGMA doesn't support parameters, so we format directly
        conn.execute(&format!("PRAGMA user_version = {version}"), [])?;
        Ok(())
    }

    /// Run pending database migrations
    fn run_migrations(conn: &DbConn) -> Result<(), FactbaseError> {
        let current_version = Self::get_schema_version(conn)?;

        // If database is new (version 0), set to current schema version
        if current_version == 0 {
            Self::set_schema_version(conn, SCHEMA_VERSION)?;
            tracing::debug!("Initialized schema version to {}", SCHEMA_VERSION);
            return Ok(());
        }

        // Forward-compatibility: database was created by a newer version
        if current_version > SCHEMA_VERSION {
            return Err(FactbaseError::config(format!(
                "Database schema version ({current_version}) is newer than this binary supports ({SCHEMA_VERSION}). \
                 Please update factbase: npm i -g @everyonce/factbase (or cargo install --path .)"
            )));
        }

        // Run any migrations newer than current version
        for (version, description, sql) in MIGRATIONS {
            if *version > current_version {
                tracing::info!("Running migration {}: {}", version, description);
                conn.execute_batch(sql)?;

                // Post-migration hooks
                if *version == 6 {
                    Self::backfill_has_review_queue(conn)?;
                }
                if *version == 7 {
                    Self::backfill_fts5(conn)?;
                }
                if *version == 12 {
                    Self::add_cv_lock_columns(conn)?;
                }
                if *version == 16 {
                    Self::backfill_review_questions(conn)?;
                }
                if *version == 17 {
                    Self::rename_check_at_to_lint_at(conn)?;
                }
                if *version == 18 {
                    Self::add_review_section_hash_column(conn)?;
                }
                if *version == 20 {
                    // Log how many believed questions were reset
                    let count: i64 = conn
                        .query_row("SELECT changes()", [], |row| row.get(0))
                        .unwrap_or(0);
                    if count > 0 {
                        tracing::info!(
                            "Migration v20: reset {} 'believed' question(s) to 'open' — they are back in the review queue",
                            count
                        );
                    }
                }

                Self::set_schema_version(conn, *version)?;
                tracing::info!("Migration {} complete", version);
            }
        }

        Ok(())
    }

    /// Idempotently rename last_check_at → last_lint_at (migration 17).
    /// Skips the rename if last_lint_at already exists (handles databases where
    /// both columns were present due to a prior inconsistent migration state).
    fn rename_check_at_to_lint_at(conn: &DbConn) -> Result<(), FactbaseError> {
        let has_lint_at: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('repositories') WHERE name = 'last_lint_at'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_lint_at {
            conn.execute_batch(
                "ALTER TABLE repositories RENAME COLUMN last_check_at TO last_lint_at;",
            )?;
        }
        Ok(())
    }

    /// Backfill has_review_queue for existing documents
    fn backfill_has_review_queue(conn: &DbConn) -> Result<(), FactbaseError> {
        let mut stmt =
            conn.prepare("SELECT id, content FROM documents WHERE is_deleted = FALSE")?;
        let mut rows = stmt.query([])?;
        let mut ids_with_review: Vec<String> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let stored: String = row.get(1)?;
            let content = super::decode_content_lossy(stored);
            if crate::patterns::has_review_section(&content) {
                ids_with_review.push(id);
            }
        }
        if !ids_with_review.is_empty() {
            tracing::info!(
                "Backfilling has_review_queue for {} documents",
                ids_with_review.len()
            );
            for id in &ids_with_review {
                conn.execute(
                    "UPDATE documents SET has_review_queue = TRUE WHERE id = ?1",
                    [id],
                )?;
            }
        }
        Ok(())
    }

    /// Idempotently add lock columns to cross_validation_state (migration 12)
    fn add_cv_lock_columns(conn: &DbConn) -> Result<(), FactbaseError> {
        let has_locked_by: bool = conn
            .prepare("SELECT locked_by FROM cross_validation_state LIMIT 0")
            .is_ok();
        if !has_locked_by {
            conn.execute_batch(
                "ALTER TABLE cross_validation_state ADD COLUMN locked_by TEXT;
                 ALTER TABLE cross_validation_state ADD COLUMN locked_at TEXT;",
            )?;
        }
        Ok(())
    }

    /// Backfill review_questions table from existing documents (migration 16)
    fn backfill_review_questions(conn: &DbConn) -> Result<(), FactbaseError> {
        use crate::processor::parse_review_queue;

        let mut stmt =
            conn.prepare("SELECT id, content FROM documents WHERE is_deleted = FALSE")?;
        let mut rows = stmt.query([])?;
        let mut count = 0usize;
        let now = chrono::Utc::now().to_rfc3339();

        while let Some(row) = rows.next()? {
            let doc_id: String = row.get(0)?;
            let stored: String = row.get(1)?;
            let content = super::decode_content_lossy(stored);
            if !crate::patterns::has_review_section(&content) {
                continue;
            }
            if let Some(questions) = parse_review_queue(&content) {
                for (idx, q) in questions.iter().enumerate() {
                    let status = if q.answered {
                        "verified"
                    } else if q.is_deferred() {
                        // is_believed() is a subset of is_deferred(); both map to "deferred"
                        "deferred"
                    } else {
                        "open"
                    };
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO review_questions \
                         (doc_id, question_index, question_type, description, line_ref, answer, status, created_at, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                        rusqlite::params![
                            doc_id,
                            idx as i64,
                            q.question_type.as_str(),
                            q.description,
                            q.line_ref.map(|l| l as i64),
                            q.answer,
                            status,
                            now
                        ],
                    );
                    count += 1;
                }
            }
        }
        if count > 0 {
            tracing::info!("Backfilled review_questions table with {} rows", count);
        }
        Ok(())
    }

    /// Backfill FTS5 index from existing documents
    fn backfill_fts5(conn: &DbConn) -> Result<(), FactbaseError> {
        let mut stmt =
            conn.prepare("SELECT id, content FROM documents WHERE is_deleted = FALSE")?;
        let mut rows = stmt.query([])?;
        let mut count = 0usize;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let stored: String = row.get(1)?;
            let content = super::decode_content_lossy(stored);
            conn.execute(
                "INSERT INTO document_content_fts (doc_id, content) VALUES (?1, ?2)",
                rusqlite::params![id, content],
            )?;
            count += 1;
        }
        if count > 0 {
            tracing::info!("Backfilled FTS5 index for {} documents", count);
        }
        Ok(())
    }

    /// Add review_section_hash column if it doesn't already exist (idempotent).
    fn add_review_section_hash_column(conn: &DbConn) -> Result<(), FactbaseError> {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('documents') WHERE name = 'review_section_hash'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !exists {
            conn.execute_batch("ALTER TABLE documents ADD COLUMN review_section_hash TEXT;")?;
        }
        Ok(())
    }

    /// Check if embedding schema needs migration
    fn check_embedding_migration(conn: &DbConn) -> Result<bool, FactbaseError> {
        // Check if table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='document_embeddings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !table_exists {
            return Ok(false);
        }

        // Check for old schema (768 dims or document_id primary key)
        let sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='document_embeddings'",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(sql) = sql {
            // Needs migration if old dimension or old primary key name
            if sql.contains("FLOAT[768]") || sql.contains("document_id TEXT PRIMARY KEY") {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_schema_version_constant() {
        // Ensure schema version is positive (compile-time check)
        const _: () = assert!(SCHEMA_VERSION > 0);
    }

    #[test]
    fn test_migrations_ordered() {
        // Ensure migrations are in ascending order
        let mut prev_version = 0;
        for (version, _, _) in MIGRATIONS {
            assert!(
                *version > prev_version,
                "Migration versions must be ascending"
            );
            prev_version = *version;
        }
    }

    #[test]
    fn test_init_schema_creates_tables() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let db = Database::new(&db_path).expect("create database");

        // Verify tables exist by querying them
        let conn = db.get_conn().expect("get connection");

        // Check repositories table
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))
            .expect("query repositories");
        assert_eq!(count, 0);

        // Check documents table
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .expect("query documents");
        assert_eq!(count, 0);

        // Check document_links table
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM document_links", [], |row| row.get(0))
            .expect("query document_links");
        assert_eq!(count, 0);

        // Check embedding_chunks table
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM embedding_chunks", [], |row| {
                row.get(0)
            })
            .expect("query embedding_chunks");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_schema_version_tracking() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let db = Database::new(&db_path).expect("create database");

        let conn = db.get_conn().expect("get connection");
        let version = Database::get_schema_version(&conn).expect("get version");

        // Should be set to current schema version
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_file_modified_at_index_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        // Open connection directly to check index
        let conn = rusqlite::Connection::open(&db_path).expect("open connection");
        let index_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name='idx_documents_modified'",
                [],
                |row| row.get(0),
            )
            .expect("query index");

        assert!(index_exists, "idx_documents_modified index should exist");
    }

    #[test]
    fn test_word_count_column_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        // Open connection directly to check column
        let conn = rusqlite::Connection::open(&db_path).expect("open connection");

        // Query table info to verify word_count column exists
        let column_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('documents') WHERE name = 'word_count'",
                [],
                |row| row.get(0),
            )
            .expect("query column");

        assert!(
            column_exists,
            "word_count column should exist in documents table"
        );
    }

    #[test]
    fn test_cross_check_hash_column_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        let conn = rusqlite::Connection::open(&db_path).expect("open connection");
        let column_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('documents') WHERE name = 'cross_check_hash'",
                [],
                |row| row.get(0),
            )
            .expect("query column");

        assert!(
            column_exists,
            "cross_check_hash column should exist in documents table"
        );
    }

    #[test]
    fn test_fts5_table_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        let conn = rusqlite::Connection::open(&db_path).expect("open connection");
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='document_content_fts'",
                [],
                |row| row.get(0),
            )
            .expect("query table");

        assert!(table_exists, "document_content_fts FTS5 table should exist");
    }

    #[test]
    fn test_query_embedding_cache_table_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        let conn = rusqlite::Connection::open(&db_path).expect("open connection");
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='query_embedding_cache'",
                [],
                |row| row.get(0),
            )
            .expect("query table");

        assert!(table_exists, "query_embedding_cache table should exist");
    }

    #[test]
    fn test_last_lint_at_column_exists() {
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = Database::new(&db_path).expect("create database");

        let conn = rusqlite::Connection::open(&db_path).expect("open connection");
        let column_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('repositories') WHERE name = 'last_lint_at'",
                [],
                |row| row.get(0),
            )
            .expect("query column");

        assert!(
            column_exists,
            "last_lint_at column should exist in repositories table"
        );
    }

    #[test]
    fn test_migration_v17_idempotent_when_lint_at_already_exists() {
        // Simulate a database that somehow has both last_check_at AND last_lint_at
        // (the broken state described in the bug report). Migration v17 must not fail.
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");

        {
            let conn = rusqlite::Connection::open(&db_path).expect("open connection");
            conn.execute_batch(
                "CREATE TABLE repositories (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    path TEXT UNIQUE NOT NULL,
                    perspective TEXT,
                    created_at TIMESTAMP NOT NULL,
                    last_indexed_at TIMESTAMP,
                    last_check_at TIMESTAMP
                );
                PRAGMA user_version = 16;",
            )
            .expect("create table");
            // Simulate the broken state: last_lint_at was added via ALTER TABLE
            // while last_check_at still exists.
            conn.execute_batch("ALTER TABLE repositories ADD COLUMN last_lint_at TIMESTAMP;")
                .expect("add duplicate column");
        }

        // Opening the database must succeed (not panic or return an error).
        let db = Database::new(&db_path).expect("open database with duplicate column state");
        let conn = db.get_conn().expect("get connection");

        let has_lint_at: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('repositories') WHERE name = 'last_lint_at'",
                [],
                |row| row.get(0),
            )
            .expect("query column");
        assert!(has_lint_at, "last_lint_at should exist after migration v17");
    }

    #[test]
    fn test_migration_v17_renames_last_check_at() {
        // Simulate a v16 database that has last_check_at and apply migration v17
        let temp = TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");

        // Create a database with last_check_at (simulating pre-v17 state)
        {
            let conn = rusqlite::Connection::open(&db_path).expect("open connection");
            conn.execute_batch(
                "CREATE TABLE repositories (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    path TEXT UNIQUE NOT NULL,
                    perspective TEXT,
                    created_at TIMESTAMP NOT NULL,
                    last_indexed_at TIMESTAMP,
                    last_check_at TIMESTAMP
                );
                PRAGMA user_version = 16;",
            )
            .expect("create table");
        }

        // Opening the database should apply migration v17
        let db = Database::new(&db_path).expect("open database");
        let conn = db.get_conn().expect("get connection");

        let has_lint_at: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('repositories') WHERE name = 'last_lint_at'",
                [],
                |row| row.get(0),
            )
            .expect("query column");
        assert!(has_lint_at, "last_lint_at should exist after migration v17");

        let has_check_at: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('repositories') WHERE name = 'last_check_at'",
                [],
                |row| row.get(0),
            )
            .expect("query column");
        assert!(
            !has_check_at,
            "last_check_at should not exist after migration v17"
        );
    }
}
