//! Parallel file pre-reading for scan orchestration

use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

use crate::DocumentProcessor;

use super::types::PreReadFile;

/// Pre-read files in parallel (I/O bound)
///
/// Reads file content, computes hash, extracts existing ID, and gets modification time.
/// Uses rayon for parallel I/O.
pub(super) fn pre_read_files(files: Vec<PathBuf>) -> Vec<PreReadFile> {
    files
        .into_par_iter()
        .map(|path| {
            let content = fs::read_to_string(&path).map_err(|e| e.to_string());
            let (hash, existing_id) = if let Ok(ref c) = content {
                let h = DocumentProcessor::compute_hash(c);
                let id = DocumentProcessor::extract_id_static(c);
                (Some(h), id)
            } else {
                (None, None)
            };
            let modified_at = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::<Utc>::from);
            PreReadFile {
                path,
                content,
                hash,
                existing_id,
                modified_at,
            }
        })
        .collect()
}
