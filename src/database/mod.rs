//! Database module for SQLite operations.
//!
//! This module provides the [`Database`] struct for all database operations
//! including document storage, embedding management, search, and statistics.
//!
//! # Module Organization
//!
//! The database module is split into focused submodules:
//!
//! - `schema` - Schema initialization and migrations
//! - `compression` - Content compression and encoding helpers
//! - `documents/` - Document operations (crud, list, batch submodules)
//! - `repositories` - Repository CRUD operations
//! - `links` - Document link operations
//! - `embeddings` - Embedding storage and retrieval
//! - `search` - Search operations (semantic, title, content)
//! - `stats` - Statistics and caching
//!
//! # Public API
//!
//! All public items are re-exported from this module.
//!
//! ## Structs (2)
//!
//! - [`Database`] - Main database connection and operations
//! - [`EmbeddingStatus`] - Result of checking embedding coverage
//!
//! ## Methods (55 total)
//!
//! ### Constructor & Pool Management (4)
//! - [`Database::new`] - Default constructor (pool_size=4, no compression)
//! - [`Database::with_pool_size`] - Constructor with custom pool size
//! - [`Database::with_options`] - Full constructor (pool_size + compression)
//! - [`Database::pool_stats`] - Get pool statistics
//!
//! ### Repository Operations (8)
//! - [`Database::upsert_repository`] - Insert or update repository
//! - [`Database::get_repository`] - Get repository by ID
//! - [`Database::list_repositories`] - List all repositories
//! - [`Database::add_repository`] - Add new repository
//! - [`Database::remove_repository`] - Remove repository and docs
//! - [`Database::get_repository_by_path`] - Find repo by filesystem path
//! - [`Database::list_repositories_with_stats`] - List repos with doc counts
//! - [`Database::update_last_check_at`] - Update check timestamp
//!
//! ### Document Operations (17) — `documents/` submodule
//!
//! #### CRUD (`documents/crud.rs`) — 10 methods
//! - [`Database::upsert_document`] - Insert or update document
//! - [`Database::update_document_content`] - Update content and FTS5 index
//! - [`Database::update_document_hash`] - Update content hash
//! - [`Database::update_document_type`] - Update document type
//! - [`Database::needs_update`] - Check if doc needs re-indexing
//! - [`Database::get_document`] - Get document by ID
//! - [`Database::require_document`] - Get document by ID or error
//! - [`Database::get_document_by_path`] - Get document by file path
//! - [`Database::mark_deleted`] - Mark document as deleted
//! - [`Database::hard_delete_document`] - Permanent delete
//!
//! #### Listing (`documents/list.rs`) — 3 methods
//! - [`Database::get_documents_for_repo`] - Get all docs in repository
//! - [`Database::get_documents_with_review_queue`] - Get docs with review content
//! - [`Database::list_documents`] - List docs with filters
//!
//! #### Batch (`documents/batch.rs`) — 4 methods
//! - [`Database::needs_cross_check`] - Check if doc needs cross-validation
//! - [`Database::set_cross_check_hash`] - Mark doc as cross-checked
//! - [`Database::clear_cross_check_hashes`] - Reset cross-check state for docs
//! - [`Database::backfill_word_counts`] - Populate word counts for existing docs
//!
//! ### Transaction Control (2)
//! - [`Database::with_transaction`] - Execute operations in a transaction
//!
//! ### Statistics & Caching (7)
//! - [`Database::get_stats`] - Basic repo stats
//! - [`Database::get_detailed_stats`] - Extended stats
//! - [`Database::invalidate_stats_cache`] - Clear cached stats
//! - [`Database::compute_temporal_stats`] - Temporal tag statistics
//! - [`Database::compute_source_stats`] - Source reference statistics
//! - [`Database::health_check`] - Verify DB connectivity
//! - [`Database::vacuum`] - Optimize database
//!
//! ### Embedding Operations (6)
//! - [`Database::upsert_embedding`] - Store document embedding
//! - [`Database::upsert_embedding_chunk`] - Store chunked embedding
//! - [`Database::delete_embedding`] - Remove embedding
//! - [`Database::get_chunk_metadata`] - Get chunk info for doc
//! - [`Database::check_embedding_status`] - Check embedding coverage
//! - [`Database::get_embedding_dimension`] - Get vector dimension
//!
//! ### Search Operations (5)
//! - [`Database::find_similar_documents`] - Find duplicates by similarity
//! - [`Database::search_semantic_with_query`] - Vector similarity search
//! - [`Database::search_semantic_paginated`] - Paginated semantic search
//! - [`Database::search_by_title`] - Title-based search
//! - [`Database::search_content`] - Full-text grep search
//!
//! ### Link Operations (4)
//! - [`Database::get_all_document_titles`] - Get titles for link detection
//! - [`Database::update_links`] - Update document links
//! - [`Database::get_links_from`] - Get outgoing links
//! - [`Database::get_links_to`] - Get incoming links

// Submodules
mod compression;
mod documents;
mod embeddings;
mod links;
mod repositories;
mod schema;
mod search;
mod stats;

pub use search::ContentSearchParams;

pub(crate) use compression::{compress_content, decode_content, decode_content_lossy};
#[cfg(feature = "compression")]
pub(crate) use compression::{decompress_content, ZSTD_PREFIX};

pub(crate) use crate::error::doc_not_found;
pub(crate) use crate::error::repo_not_found;
use crate::error::FactbaseError;
use crate::models::{DetailedStats, RepoStats, SourceStats, TemporalStats};
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

/// Type alias for a pooled SQLite connection, used across all database submodules.
pub(crate) type DbConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// Shared base64 engine constant, replacing repeated `base64::engine::general_purpose::STANDARD`.
pub(crate) const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Parse an RFC 3339 timestamp string to `DateTime<Utc>`, falling back to `Utc::now()`.
pub(crate) fn parse_rfc3339_utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).map_or_else(|_| Utc::now(), |d| d.with_timezone(&Utc))
}

/// Parse an RFC 3339 timestamp string to `Option<DateTime<Utc>>`.
pub(crate) fn parse_rfc3339_utc_opt(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Cached statistics for a repository
#[derive(Clone)]
pub(crate) struct CachedStats {
    pub(crate) stats: RepoStats,
    pub(crate) detailed: DetailedStats,
    pub(crate) temporal: Option<TemporalStats>,
    pub(crate) source: Option<SourceStats>,
}

/// Result of checking embedding status for a repository
#[derive(Debug, Clone)]
pub struct EmbeddingStatus {
    /// Document IDs that have embeddings
    pub with_embeddings: Vec<String>,
    /// Documents missing embeddings: (id, title)
    pub without_embeddings: Vec<(String, String)>,
    /// Orphaned embedding document IDs (deleted or non-existent docs)
    pub orphaned: Vec<String>,
}

#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
    stats_cache: Arc<RwLock<HashMap<String, CachedStats>>>,
    compression: bool,
}

impl Database {
    /// Creates a new database connection with default settings.
    ///
    /// Uses a pool size of 4 connections and no compression.
    /// Creates parent directories if they don't exist.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` if the connection cannot be established.
    pub fn new(path: &Path) -> Result<Self, FactbaseError> {
        Self::with_options(path, 4, false)
    }

    /// Creates a new database connection with a custom pool size.
    ///
    /// Pool size is clamped to 1-32 connections.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` if the connection cannot be established.
    pub fn with_pool_size(path: &Path, pool_size: u32) -> Result<Self, FactbaseError> {
        Self::with_options(path, pool_size, false)
    }

    /// Creates a new database connection with full configuration options.
    ///
    /// Initializes the SQLite database with WAL mode, runs migrations,
    /// and sets up the connection pool.
    ///
    /// # Arguments
    /// * `path` - Path to the SQLite database file
    /// * `pool_size` - Number of connections (clamped to 1-32)
    /// * `compression` - Enable zstd compression for document content
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` if the connection cannot be established
    /// or migrations fail.
    pub fn with_options(
        path: &Path,
        pool_size: u32,
        compression: bool,
    ) -> Result<Self, FactbaseError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Load sqlite-vec as auto extension before opening any connection
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let manager = SqliteConnectionManager::file(path).with_init(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
            )?;
            Ok(())
        });

        // Validate pool_size (1-32)
        let size = pool_size.clamp(1, 32);

        let pool = Pool::builder().max_size(size).build(manager).map_err(|e| {
            FactbaseError::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some(e.to_string()),
            ))
        })?;

        let db = Self {
            pool,
            stats_cache: Arc::new(RwLock::new(HashMap::new())),
            compression,
        };
        tracing::debug!("Database opened: {}", path.display());
        db.init_schema()?;
        Ok(db)
    }

    /// Get a connection from the pool.
    ///
    /// This is used internally by submodules for database operations.
    pub(crate) fn get_conn(&self) -> Result<DbConn, FactbaseError> {
        self.pool.get().map_err(|e| {
            FactbaseError::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some(e.to_string()),
            ))
        })
    }

    /// Execute a closure within a single-connection transaction.
    ///
    /// Checks out one connection, begins a transaction, runs the closure,
    /// and commits on success or rolls back on error. The closure receives
    /// the connection so all operations run on the same connection.
    ///
    /// # Errors
    /// Returns the closure's error (after rollback) or a transaction error.
    pub fn with_transaction<F, T>(&self, f: F) -> Result<T, FactbaseError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, FactbaseError>,
    {
        let conn = self.get_conn()?;
        conn.execute("BEGIN TRANSACTION", [])?;
        match f(&conn) {
            Ok(val) => {
                conn.execute("COMMIT", [])?;
                Ok(val)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    // Stats operations are in stats.rs

    /// Lists documents with optional filtering.
    ///
    /// Returns full document records (including content) for MCP tools.
    /// Results are ordered by title.
    ///
    /// # Arguments
    /// * `doc_type` - Optional filter by document type
    /// * `repo_id` - Optional filter by repository
    /// * `title_filter` - Optional SQL LIKE pattern for title
    /// * `limit` - Maximum number of results
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    /// Run VACUUM and ANALYZE to optimize database
    pub fn health_check(&self) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.query_row("SELECT 1", [], |_| Ok(()))?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::{Document, Repository};
    use tempfile::TempDir;

    pub(crate) fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let db_path = tmp.path().join("test.db");
        let db = Database::new(&db_path).expect("failed to create database");
        (db, tmp)
    }

    #[test]
    fn test_with_pool_size_clamps_values() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        // Test that pool_size 0 gets clamped to 1
        let db_path = tmp.path().join("test1.db");
        let db = Database::with_pool_size(&db_path, 0).expect("pool_size 0 should clamp to 1");
        drop(db);
        // Test that pool_size 100 gets clamped to 32
        let db_path = tmp.path().join("test2.db");
        let db = Database::with_pool_size(&db_path, 100).expect("pool_size 100 should clamp to 32");
        drop(db);
        // Test normal pool_size works
        let db_path = tmp.path().join("test3.db");
        let db = Database::with_pool_size(&db_path, 8).expect("pool_size 8 should work");
        drop(db);
    }

    #[test]
    fn test_health_check_succeeds() {
        let (db, _tmp) = test_db();
        db.health_check().expect("health check should succeed");
    }

    pub(crate) fn test_repo_in_db(db: &Database, id: &str, path: &std::path::Path) {
        let repo = Repository {
            id: id.to_string(),
            name: id.to_string(),
            path: path.to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).expect("create repo");
    }

    pub(crate) fn test_repo() -> Repository {
        Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: std::path::PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        }
    }

    pub(crate) fn test_repo_with_id(id: &str) -> Repository {
        Repository {
            id: id.to_string(),
            name: format!("Test Repo {id}"),
            path: std::path::PathBuf::from(format!("/tmp/{id}")),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        }
    }

    pub(crate) fn test_doc(id: &str, title: &str) -> Document {
        Document {
            id: id.to_string(),
            file_path: format!("{id}.md"),
            title: title.to_string(),
            doc_type: Some("document".to_string()),
            content: format!("# {title}\n\nContent here."),
            ..Document::test_default()
        }
    }

    pub(crate) fn test_doc_with_repo(id: &str, repo_id: &str, title: &str) -> Document {
        Document {
            id: id.to_string(),
            repo_id: repo_id.to_string(),
            file_path: format!("{id}.md"),
            title: title.to_string(),
            doc_type: Some("document".to_string()),
            content: format!("# {title}\n\nContent here."),
            ..Document::test_default()
        }
    }

    #[test]
    fn test_database_new_creates_tables() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        // Should not error - tables exist
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
    }

    // Stats tests are in stats.rs
    // Embedding tests are in embeddings.rs
    // Search tests are in search.rs
    // Compression tests are in compression.rs

    #[test]
    fn test_database_with_compression() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let db_path = tmp.path().join("compressed.db");
        let db =
            Database::with_options(&db_path, 4, true).expect("should create db with compression");

        // Create a repository first
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert repo should succeed");

        // Create a test document
        let doc = test_doc("abc123", "Test Doc");
        db.upsert_document(&doc).expect("upsert should succeed");

        // Read it back
        let retrieved = db
            .get_document("abc123")
            .expect("get should succeed")
            .expect("doc should exist");
        assert_eq!(
            retrieved.content, doc.content,
            "Content should match after compression roundtrip"
        );
    }

    // Note: Schema version tests moved to schema.rs

    #[test]
    fn test_with_transaction_commits_on_success() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.with_transaction(|conn| {
            let doc = test_doc("aaa111", "Tx Test");
            conn.execute(
                "INSERT INTO documents (id, repo_id, file_path, title, content, file_hash, indexed_at, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), FALSE)",
                rusqlite::params![doc.id, "test-repo", doc.file_path, doc.title, doc.content, "hash1"],
            )?;
            Ok(())
        })
        .unwrap();

        // Document should be visible after commit
        assert!(db.get_document("aaa111").unwrap().is_some());
    }

    #[test]
    fn test_with_transaction_rolls_back_on_error() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let result: Result<(), _> = db.with_transaction(|conn| {
            conn.execute(
                "INSERT INTO documents (id, repo_id, file_path, title, content, file_hash, indexed_at, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), FALSE)",
                rusqlite::params!["bbb222", "test-repo", "bbb222.md", "Rollback", "content", "hash2"],
            )?;
            Err(FactbaseError::internal("forced error"))
        });

        assert!(result.is_err());
        // Document should NOT exist after rollback
        assert!(db.get_document("bbb222").unwrap().is_none());
    }
}
