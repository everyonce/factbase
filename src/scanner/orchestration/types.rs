//! Internal types for scan orchestration

use std::path::PathBuf;

/// Document pending embedding generation
#[derive(Clone)]
pub(super) struct PendingDoc {
    pub id: String,
    pub content: String,
    pub relative: String,
    pub hash: String,
    pub title: String,
    pub doc_type: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Pre-read file data from parallel I/O phase
pub(super) struct PreReadFile {
    pub path: PathBuf,
    pub content: Result<String, String>,
    pub hash: Option<String>,
    pub existing_id: Option<String>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Chunk information for embedding generation
pub(super) struct ChunkInfo {
    pub doc_idx: usize,
    pub chunk_idx: usize,
    pub chunk_start: usize,
    pub chunk_end: usize,
    pub content: String,
}
