//! Search operations.
//!
//! This module handles:
//! - Semantic search (search_semantic_with_query, search_semantic_paginated)
//! - Title search (search_by_title)
//! - Content search (search_content)
//! - Duplicate detection (find_similar_documents)
//!
//! # Module Organization
//!
//! - `semantic` - Vector similarity search using sqlite-vec
//! - `content` - Full-text search using SQL LIKE patterns
//! - `title` - Title search using SQL LIKE patterns
//!
//! # Semantic Search
//!
//! Uses sqlite-vec for vector similarity search with cosine distance.
//! Supports filtering by repository, document type, and temporal context.
//!
//! # Content Search
//!
//! Full-text search using SQL LIKE patterns with optional regex support.

mod content;
mod semantic;
mod title;

pub use content::ContentSearchParams;

// Re-export all search functions via Database impl
// The actual implementations are in the submodules

/// Common SELECT columns for search queries returning `SearchResult`.
/// Used by title and content search; semantic search uses a different projection (joins).
pub(crate) const SEARCH_COLUMNS: &str = "id, title, doc_type, file_path, content";

/// Appends optional `AND {prefix}doc_type = ?N` / `AND {prefix}repo_id = ?N` filter clauses.
/// `param_idx` is the next available parameter index; returns the updated index.
pub(crate) fn append_type_repo_filters(
    sql: &mut String,
    mut param_idx: usize,
    doc_type: Option<&str>,
    repo_id: Option<&str>,
    column_prefix: &str,
) -> usize {
    if doc_type.is_some() {
        write_str!(sql, " AND {}doc_type = ?{}", column_prefix, param_idx);
        param_idx += 1;
    }
    if repo_id.is_some() {
        write_str!(sql, " AND {}repo_id = ?{}", column_prefix, param_idx);
        param_idx += 1;
    }
    param_idx
}

/// Pushes `doc_type` and `repo_id` values (if present) onto a params vec.
pub(crate) fn push_type_repo_params<'a>(
    params: &mut Vec<&'a dyn rusqlite::ToSql>,
    doc_type: &'a Option<&str>,
    repo_id: &'a Option<&str>,
) {
    if let Some(ref t) = doc_type {
        params.push(t);
    }
    if let Some(ref r) = repo_id {
        params.push(r);
    }
}

/// Generate a snippet from content: filter HTML comments, take 3 lines, truncate to 200 chars.
pub(crate) fn generate_snippet(content: &str) -> String {
    content
        .lines()
        .filter(|l| !l.starts_with("<!--"))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}
