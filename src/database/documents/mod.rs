//! Document CRUD operations.
//!
//! This module handles:
//! - Document insertion and updates (upsert_document)
//! - Document retrieval (get_document, get_document_by_path)
//! - Document listing (get_documents_for_repo, list_documents)
//! - Document deletion (mark_deleted, hard_delete)
//! - Content hash management (update_document_hash, needs_update)
//! - Cross-validation tracking (needs_cross_check, set_cross_check_hash)
//!
//! # Module Organization
//!
//! - `crud` - Core CRUD: upsert, get, update, delete
//! - `list` - Listing and filtering: list_documents, get_documents_for_repo
//! - `batch` - Batch operations: cross-check hashes, backfill word counts
//!
//! # Content Compression
//!
//! When the `compression` feature is enabled, document content
//! is compressed with zstd before storage and decompressed on retrieval.

mod batch;
mod crud;
mod list;

use crate::error::FactbaseError;
use crate::models::Document;

use super::{decode_content, Database};

/// Column list for SELECT queries that map to `row_to_document()`.
pub(crate) const DOCUMENT_COLUMNS: &str =
    "id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted";

/// Lightweight document stub — id, title, file_path, is_deleted only.
/// Used where full content is not needed (e.g. link detection filtering).
#[derive(Debug, Clone)]
pub struct DocStub {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub is_deleted: bool,
}

/// Look up the repo_id for a document (for cache invalidation).
fn repo_id_for_doc(conn: &super::DbConn, id: &str) -> Option<String> {
    conn.query_row("SELECT repo_id FROM documents WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .ok()
}

impl Database {
    /// Converts a database row to a Document struct.
    ///
    /// Handles content decompression automatically.
    pub(crate) fn row_to_document(row: &rusqlite::Row) -> Result<Document, FactbaseError> {
        let file_modified_str: Option<String> = row.get(7)?;
        let indexed_str: String = row.get(8)?;
        let stored_content: String = row.get(6)?;

        // Auto-detect and decompress content
        let content = decode_content(&stored_content)?;

        Ok(Document {
            id: row.get(0)?,
            repo_id: row.get(1)?,
            file_path: row.get(2)?,
            file_hash: row.get(3)?,
            title: row.get(4)?,
            doc_type: row.get(5)?,
            content,
            file_modified_at: file_modified_str.and_then(|s| super::parse_rfc3339_utc_opt(&s)),
            indexed_at: super::parse_rfc3339_utc(&indexed_str),
            is_deleted: row.get(9)?,
        })
    }
}
