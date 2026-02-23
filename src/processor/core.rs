//! Core document processing: ID extraction/injection, title, type, hash.
//!
//! This module contains the fundamental document processing functions
//! that handle document identity and metadata extraction.

use crate::database::Database;
use crate::patterns::ID_REGEX;
use getrandom::getrandom;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Compute SHA256 hash of content, returning lowercase hex string.
pub fn content_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// Core document processor for ID extraction/injection, title, type, and hash operations.
pub struct DocumentProcessor;

impl DocumentProcessor {
    /// Create a new DocumentProcessor.
    pub fn new() -> Self {
        Self
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
        format!("<!-- factbase:{id} -->\n{content}")
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
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if let Some(parent) = relative.parent() {
            let parent_name = parent
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // If filename matches parent folder (e.g., xsolis/xsolis.md),
            // derive type from grandparent instead (e.g., companies/xsolis/xsolis.md → "company")
            if !parent_name.is_empty()
                && parent_name.eq_ignore_ascii_case(file_stem)
            {
                if let Some(grandparent) = parent.parent() {
                    if let Some(gp_name) = grandparent.file_name().and_then(|s| s.to_str()) {
                        if !gp_name.is_empty() {
                            return normalize_type(gp_name);
                        }
                    }
                }
            }

            if !parent_name.is_empty() {
                return normalize_type(parent_name);
            }
        }
        "document".to_string()
    }
}

/// Normalize a type name: lowercase and strip trailing 's' (naive singularization).
pub(crate) fn normalize_type(word: &str) -> String {
    let lower = word.to_lowercase();
    if lower.ends_with('s') && lower.len() > 1 {
        lower[..lower.len() - 1].to_string()
    } else {
        lower
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
    fn test_derive_type_entity_folder_convention() {
        let processor = DocumentProcessor::new();
        // projects/alpha/alpha.md → type "project" (grandparent, singularized)
        let path = PathBuf::from("/repo/projects/alpha/alpha.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "project");
    }

    #[test]
    fn test_derive_type_entity_folder_case_insensitive() {
        let processor = DocumentProcessor::new();
        // projects/Alpha/alpha.md → still matches
        let path = PathBuf::from("/repo/projects/Alpha/alpha.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "project");
    }

    #[test]
    fn test_derive_type_entity_folder_sibling_normal() {
        let processor = DocumentProcessor::new();
        // projects/alpha/people/jane.md → type "people" (normal derivation)
        let path = PathBuf::from("/repo/projects/alpha/people/jane.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
    }

    #[test]
    fn test_derive_type_entity_folder_no_false_positive() {
        let processor = DocumentProcessor::new();
        // projects/alpha/overview.md → type "alpha" (filename doesn't match folder)
        let path = PathBuf::from("/repo/projects/alpha/overview.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "alpha");
    }

    #[test]
    fn test_compute_hash() {
        let hash = content_hash("test content");
        assert_eq!(hash.len(), 64); // SHA256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let hash1 = content_hash("same content");
        let hash2 = content_hash("same content");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_different_content() {
        let hash1 = content_hash("content a");
        let hash2 = content_hash("content b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_default_impl() {
        let processor = DocumentProcessor;
        let id = processor.generate_id();
        assert_eq!(id.len(), 6);
    }
}
