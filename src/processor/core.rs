//! Core document processing: ID extraction/injection, title, type, hash.
//!
//! This module contains the fundamental document processing functions
//! that handle document identity and metadata extraction.

use crate::database::Database;
use crate::patterns::ID_REGEX;
use getrandom::getrandom;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Core document processor for ID extraction/injection, title, type, and hash operations.
pub struct DocumentProcessor;

impl DocumentProcessor {
    /// Create a new DocumentProcessor.
    pub fn new() -> Self {
        Self
    }

    /// Compute SHA256 hash of content, returning lowercase hex string.
    pub fn compute_hash(content: &str) -> String {
        hex::encode(Sha256::digest(content.as_bytes()))
    }

    /// Extract the factbase ID from document content, if present.
    pub fn extract_id(&self, content: &str) -> Option<String> {
        Self::extract_id_static(content)
    }

    /// Static version of extract_id for use in parallel contexts
    pub fn extract_id_static(content: &str) -> Option<String> {
        let first_line = content.lines().next()?;
        ID_REGEX.captures(first_line).map(|c| c[1].to_string())
    }

    /// Generate a random 6-character hex document ID.
    pub fn generate_id(&self) -> String {
        let mut bytes = [0u8; 3];
        getrandom(&mut bytes).expect("getrandom failed");
        hex::encode(bytes)
    }

    /// Check if a document ID is unique in the database.
    pub fn is_id_unique(&self, id: &str, db: &Database) -> bool {
        db.get_document(id).map(|d| d.is_none()).unwrap_or(true)
    }

    /// Generate a unique document ID, retrying up to 100 times on collision.
    pub fn generate_unique_id(&self, db: &Database) -> String {
        for _ in 0..100 {
            let id = self.generate_id();
            if self.is_id_unique(&id, db) {
                return id;
            }
        }
        self.generate_id() // fallback
    }

    /// Inject the factbase ID header comment at the top of content.
    pub fn inject_header(&self, content: &str, id: &str) -> String {
        format!("<!-- factbase:{} -->\n{}", id, content)
    }

    /// Extract the document title from the first H1 heading, falling back to filename.
    pub fn extract_title(&self, content: &str, path: &Path) -> String {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<!-- factbase:") {
                continue;
            }
            if let Some(title) = trimmed.strip_prefix("# ") {
                return title.trim().to_string();
            }
            if !trimmed.is_empty() {
                break;
            }
        }
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    /// Derive the document type from the parent folder name (e.g., "people/" → "person").
    pub fn derive_type(&self, path: &Path, repo_root: &Path) -> String {
        let relative = path.strip_prefix(repo_root).unwrap_or(path);
        if let Some(parent) = relative.parent() {
            if let Some(folder) = parent.file_name().and_then(|s| s.to_str()) {
                if !folder.is_empty() {
                    return self.normalize_type(folder);
                }
            }
        }
        "document".to_string()
    }

    fn normalize_type(&self, word: &str) -> String {
        let lower = word.to_lowercase();
        self.singularize(&lower)
    }

    fn singularize(&self, word: &str) -> String {
        if word.ends_with('s') && word.len() > 1 {
            word[..word.len() - 1].to_string()
        } else {
            word.to_string()
        }
    }
}

impl Default for DocumentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_id_valid() {
        let processor = DocumentProcessor::new();
        let content = "<!-- factbase:a1b2c3 -->\n# Title";
        assert_eq!(processor.extract_id(content), Some("a1b2c3".to_string()));
    }

    #[test]
    fn test_extract_id_missing() {
        let processor = DocumentProcessor::new();
        let content = "# Title\nSome content";
        assert_eq!(processor.extract_id(content), None);
    }

    #[test]
    fn test_extract_id_static() {
        let content = "<!-- factbase:abc123 -->\n# Test";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_generate_id_format() {
        let processor = DocumentProcessor::new();
        let id = processor.generate_id();
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_inject_header() {
        let processor = DocumentProcessor::new();
        let content = "# Title\nContent";
        let result = processor.inject_header(content, "abc123");
        assert!(result.starts_with("<!-- factbase:abc123 -->"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_extract_title_from_h1() {
        let processor = DocumentProcessor::new();
        let content = "<!-- factbase:abc123 -->\n# My Title\nContent";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "My Title");
    }

    #[test]
    fn test_extract_title_from_filename() {
        let processor = DocumentProcessor::new();
        let content = "<!-- factbase:abc123 -->\nNo heading here";
        let path = PathBuf::from("/test/my-document.md");
        assert_eq!(processor.extract_title(content, &path), "my-document");
    }

    #[test]
    fn test_extract_title_skips_factbase_header() {
        let processor = DocumentProcessor::new();
        let content = "<!-- factbase:abc123 -->\n\n# Actual Title";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "Actual Title");
    }

    #[test]
    fn test_derive_type_from_folder() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/people/john.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
    }

    #[test]
    fn test_derive_type_normalizes() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/People/john.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
    }

    #[test]
    fn test_derive_type_singularizes() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/projects/alpha.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "project");
    }

    #[test]
    fn test_derive_type_default() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/doc.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "document");
    }

    #[test]
    fn test_compute_hash() {
        let hash = DocumentProcessor::compute_hash("test content");
        assert_eq!(hash.len(), 64); // SHA256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let hash1 = DocumentProcessor::compute_hash("same content");
        let hash2 = DocumentProcessor::compute_hash("same content");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_different_content() {
        let hash1 = DocumentProcessor::compute_hash("content a");
        let hash2 = DocumentProcessor::compute_hash("content b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_default_impl() {
        let processor = DocumentProcessor;
        let id = processor.generate_id();
        assert_eq!(id.len(), 6);
    }
}
