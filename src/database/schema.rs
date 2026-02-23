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
pub(super) const SCHEMA_VERSION: i32 = 7;

/// Database migrations. Each entry is (version, description, sql).
/// Migrations are run in order for versions > current user_version.
/// Version 1 is the baseline schema (created by init_schema).
pub(super) const MIGRATIONS: &[(i32, &str, &str)] = &[
    // Version 2: Add last_lint_at column for incremental linting
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

                Self::set_schema_version(conn, *version)?;
                tracing::info!("Migration {} complete", version);
            }
        }

        Ok(())
    }

    /// Backfill has_review_queue for existing documents
    fn backfill_has_review_queue(conn: &DbConn) -> Result<(), FactbaseError> {
        use crate::patterns::REVIEW_QUEUE_MARKER;

        let mut stmt =
            conn.prepare("SELECT id, content FROM documents WHERE is_deleted = FALSE")?;
        let mut rows = stmt.query([])?;
        let mut ids_with_review: Vec<String> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let stored: String = row.get(1)?;
            let content = super::decode_content_lossy(stored);
            if content.contains(REVIEW_QUEUE_MARKER) {
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
}
