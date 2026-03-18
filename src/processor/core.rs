//! Core document processing: ID extraction/injection, title, type, hash.
//!
//! This module contains the fundamental document processing functions
//! that handle document identity and metadata extraction.

use crate::database::Database;
use getrandom::getrandom;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Organizational folder names that are skipped when deriving document type.
/// When a document lives directly inside one of these folders, the grandparent
/// folder is used for type derivation instead.
pub(crate) const STRUCTURAL_FOLDERS: &[&str] = &[
    "archive",
    "archived",
    "old",
    "inactive",
    "deprecated",
    "drafts",
    "temp",
];

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

    /// Static version of extract_id for use in parallel contexts.
    ///
    /// Checks for a factbase document ID in two formats (in priority order):
    /// 1. YAML frontmatter: `---\nfactbase_id: abc123\n---`
    /// 2. HTML comment header: `<!-- factbase:abc123 -->`
    ///
    /// When both are present (e.g. a legacy comment header followed by frontmatter),
    /// the frontmatter ID wins. This handles the migration case where files have a
    /// stale comment header but a valid frontmatter ID that matches the database.
    pub fn extract_id_static(content: &str) -> Option<String> {
        let mut lines = content.lines().peekable();
        let first_line = lines.peek()?;

        // Check for legacy HTML comment header: <!-- factbase:abc123 -->
        // If found, consume it and then check for frontmatter below.
        let comment_id = if let Some(rest) = first_line.trim().strip_prefix("<!-- factbase:") {
            if let Some(id) = rest.strip_suffix(" -->") {
                if crate::patterns::DOC_ID_REGEX.is_match(id) {
                    lines.next(); // consume the comment line
                    Some(id.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Skip blank lines between comment header and frontmatter
        while lines.peek().map(|l| l.trim().is_empty()).unwrap_or(false) {
            lines.next();
        }

        // Check for YAML frontmatter: ---\nfactbase_id: abc123\n---
        // Frontmatter ID takes priority over comment ID.
        if lines.peek().map(|l| l.trim() == "---").unwrap_or(false) {
            lines.next(); // consume opening ---
            for line in lines {
                let trimmed = line.trim();
                if trimmed == "---" {
                    break;
                }
                if let Some(id) = trimmed.strip_prefix("factbase_id:") {
                    let id = id.trim();
                    if crate::patterns::DOC_ID_REGEX.is_match(id) {
                        return Some(id.to_string());
                    }
                }
            }
        }

        // Fall back to comment ID if no frontmatter ID was found
        comment_id
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

    /// Inject the factbase ID into content according to format config.
    ///
    /// Adds `factbase_id: id` (and optionally `type: …`) to existing frontmatter,
    /// or creates a new frontmatter block if none exists.
    pub fn inject_id_with_format(
        &self,
        content: &str,
        id: &str,
        format: &crate::models::format::ResolvedFormat,
        doc_type: Option<&str>,
    ) -> String {
        use crate::models::format::IdPlacement;
        match format.id_placement {
            IdPlacement::Comment | IdPlacement::Frontmatter => {
                let mut lines = content.lines();
                if let Some(first) = lines.next() {
                    if first.trim() == "---" {
                        // Existing frontmatter — write factbase_id (and type) at the top,
                        // skipping any pre-existing factbase_id/type lines to avoid duplicates.
                        let mut result = String::from("---\n");
                        result.push_str(&format!("factbase_id: {id}\n"));
                        if let Some(t) = doc_type {
                            result.push_str(&format!("type: {t}\n"));
                        }
                        let mut in_fm = true;
                        for line in lines {
                            if in_fm {
                                let trimmed = line.trim();
                                if trimmed == "---" {
                                    in_fm = false;
                                    result.push_str(line);
                                    result.push('\n');
                                    continue;
                                }
                                // Drop stale managed fields — we already wrote fresh ones above
                                if trimmed.starts_with("factbase_id:") {
                                    continue;
                                }
                                if doc_type.is_some() && trimmed.starts_with("type:") {
                                    continue;
                                }
                            }
                            result.push_str(line);
                            result.push('\n');
                        }
                        return result;
                    }
                }
                // No existing frontmatter — create one
                let mut fm = format!("---\nfactbase_id: {id}\n");
                if let Some(t) = doc_type {
                    fm.push_str(&format!("type: {t}\n"));
                }
                fm.push_str("---\n");
                fm.push_str(content);
                fm
            }
        }
    }

    /// Extract the document title from the first H1 heading, falling back to filename.
    pub fn extract_title(&self, content: &str, path: &Path) -> String {
        let mut in_frontmatter = false;
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            // Skip YAML frontmatter block
            if i == 0 && trimmed == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if trimmed == "---" {
                    in_frontmatter = false;
                }
                continue;
            }
            if let Some(title) = trimmed.strip_prefix("# ") {
                return crate::patterns::clean_title(title);
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
    ///
    /// Structural/organizational folder names are skipped in favour of the grandparent:
    /// - `people/archive/john.md` → skips "archive" → type "people" → "person"
    /// - `services/deprecated/old-api.md` → skips "deprecated" → type "services" → "service"
    pub fn derive_type(&self, path: &Path, repo_root: &Path) -> String {
        let relative = path.strip_prefix(repo_root).unwrap_or(path);
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        if let Some(parent) = relative.parent() {
            let parent_name = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");

            // Skip structural/organizational folder names — use grandparent instead.
            if !parent_name.is_empty()
                && STRUCTURAL_FOLDERS
                    .iter()
                    .any(|&s| s.eq_ignore_ascii_case(parent_name))
            {
                if let Some(grandparent) = parent.parent() {
                    if let Some(gp_name) = grandparent.file_name().and_then(|s| s.to_str()) {
                        if !gp_name.is_empty() {
                            return normalize_type(gp_name);
                        }
                    }
                }
                // Structural folder at repo root with no grandparent → fall through to "document"
                return "document".to_string();
            }

            // If filename matches parent folder (e.g., acme/acme.md),
            // derive type from grandparent instead (e.g., companies/acme/acme.md → "company")
            if !parent_name.is_empty() && parent_name.eq_ignore_ascii_case(file_stem) {
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
        let content = "---\nfactbase_id: a1b2c3\n---\n# Title";
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
        let content = "---\nfactbase_id: abc123\n---\n# Test";
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
    fn test_extract_title_from_h1() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\n---\n# My Title\nContent";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "My Title");
    }

    #[test]
    fn test_extract_title_from_filename() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\n---\nNo heading here";
        let path = PathBuf::from("/test/my-document.md");
        assert_eq!(processor.extract_title(content, &path), "my-document");
    }

    #[test]
    fn test_extract_title_skips_factbase_header() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\n---\n\n# Actual Title";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "Actual Title");
    }

    #[test]
    fn test_extract_title_strips_footnote_refs() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\n---\n# Joan Butters [^8] [^9]\nContent";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "Joan Butters");
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
    fn test_derive_type_skips_archive_folder() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/people/archive/john-smith.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
    }

    #[test]
    fn test_derive_type_skips_old_folder() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/people/old/tim-leidig.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
    }

    #[test]
    fn test_derive_type_skips_deprecated_folder() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/services/deprecated/old-api.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "service");
    }

    #[test]
    fn test_derive_type_skips_inactive_folder() {
        let processor = DocumentProcessor::new();
        let path = PathBuf::from("/repo/customers/inactive/acme.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "customer");
    }

    #[test]
    fn test_derive_type_structural_at_root_falls_back_to_document() {
        let processor = DocumentProcessor::new();
        // archive/john.md — structural folder at repo root, no grandparent
        let path = PathBuf::from("/repo/archive/john.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "document");
    }

    #[test]
    fn test_derive_type_deep_path_with_structural_folder() {
        let processor = DocumentProcessor::new();
        // customers/acme/people/archive/john-smith.md → skips archive → people → person
        let path = PathBuf::from("/repo/customers/acme/people/archive/john-smith.md");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(processor.derive_type(&path, &repo_root), "people");
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

    // --- Frontmatter ID extraction tests ---

    #[test]
    fn test_extract_id_from_frontmatter() {
        let content = "---\nfactbase_id: abc123\ntype: person\n---\n# John Doe\n\nContent";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_id_frontmatter_only_id() {
        let content = "---\nfactbase_id: def456\n---\n# Title";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("def456".to_string())
        );
    }

    #[test]
    fn test_extract_id_frontmatter_invalid_id_ignored() {
        // Not a valid 6-char hex ID
        let content = "---\nfactbase_id: not-hex\n---\n# Title";
        assert_eq!(DocumentProcessor::extract_id_static(content), None);
    }

    #[test]
    fn test_extract_id_first_wins() {
        // First factbase_id in frontmatter is used
        let content = "---\nfactbase_id: aaa111\nfactbase_id: bbb222\n---\n# Title";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("aaa111".to_string())
        );
    }

    #[test]
    fn test_extract_title_from_frontmatter_doc() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\ntype: person\n---\n# John Doe\n\nContent";
        let path = PathBuf::from("/test/doc.md");
        assert_eq!(processor.extract_title(content, &path), "John Doe");
    }

    #[test]
    fn test_extract_title_frontmatter_no_title() {
        let processor = DocumentProcessor::new();
        let content = "---\nfactbase_id: abc123\n---\nNo heading here";
        let path = PathBuf::from("/test/my-doc.md");
        assert_eq!(processor.extract_title(content, &path), "my-doc");
    }

    // --- inject_id_with_format tests ---

    #[test]
    fn test_inject_id_default_format() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat::default();
        let content = "# Title\nContent";
        let result = processor.inject_id_with_format(content, "abc123", &fmt, None);
        assert_eq!(result, "---\nfactbase_id: abc123\n---\n# Title\nContent");
    }

    #[test]
    fn test_inject_id_frontmatter_format_no_existing() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat {
            id_placement: crate::models::format::IdPlacement::Frontmatter,
            ..Default::default()
        };
        let content = "# Title\nContent";
        let result = processor.inject_id_with_format(content, "abc123", &fmt, None);
        assert!(result.starts_with("---\nfactbase_id: abc123\n---\n"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_inject_id_frontmatter_format_existing_frontmatter() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat {
            id_placement: crate::models::format::IdPlacement::Frontmatter,
            ..Default::default()
        };
        let content = "---\ntype: person\ntags: [test]\n---\n# Title\nContent";
        let result = processor.inject_id_with_format(content, "abc123", &fmt, None);
        assert!(result.starts_with("---\nfactbase_id: abc123\n"));
        assert!(result.contains("type: person"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_inject_id_frontmatter_with_type() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat {
            id_placement: crate::models::format::IdPlacement::Frontmatter,
            ..Default::default()
        };
        let content = "# John Smith\nContent";
        let result = processor.inject_id_with_format(content, "abc123", &fmt, Some("person"));
        assert!(result.contains("factbase_id: abc123\n"));
        assert!(result.contains("type: person\n"));
    }

    // --- HTML comment header ID extraction tests ---

    #[test]
    fn test_extract_id_from_comment_header() {
        // Legacy format: <!-- factbase:abc123 -->
        let content = "<!-- factbase:abc123 -->\n# Title\n\nContent";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_id_comment_header_invalid_id_ignored() {
        // Non-hex value in comment header — not a valid doc ID
        let content = "<!-- factbase:review -->\n# Title";
        assert_eq!(DocumentProcessor::extract_id_static(content), None);
    }

    #[test]
    fn test_extract_id_frontmatter_wins_over_comment_header() {
        // When both are present, frontmatter ID takes priority.
        // This is the core fix: files with a stale comment header but valid
        // frontmatter should use the frontmatter ID (which matches the DB).
        let content =
            "<!-- factbase:aaa111 -->\n---\nfactbase_id: bbb222\ntype: person\n---\n# Title";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("bbb222".to_string())
        );
    }

    #[test]
    fn test_extract_id_comment_header_fallback_when_no_frontmatter() {
        // Comment header is used when there is no frontmatter
        let content = "<!-- factbase:abc123 -->\n# Title\n\n- Fact";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_id_comment_header_with_blank_line_before_frontmatter() {
        // Blank line between comment header and frontmatter is tolerated
        let content = "<!-- factbase:aaa111 -->\n\n---\nfactbase_id: bbb222\n---\n# Title";
        assert_eq!(
            DocumentProcessor::extract_id_static(content),
            Some("bbb222".to_string())
        );
    }

    // --- inject_id_with_format deduplication tests ---

    #[test]
    fn test_inject_id_replaces_existing_factbase_id_in_frontmatter() {
        // When frontmatter already has factbase_id, it should be replaced, not duplicated
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat::default();
        let content = "---\nfactbase_id: old111\ntype: person\n---\n# Title\nContent";
        let result = processor.inject_id_with_format(content, "new222", &fmt, Some("person"));
        assert!(result.contains("factbase_id: new222\n"));
        assert!(!result.contains("factbase_id: old111"));
        // type should appear exactly once
        assert_eq!(result.matches("type: person").count(), 1);
    }

    #[test]
    fn test_inject_id_replaces_existing_type_in_frontmatter() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat::default();
        let content = "---\nfactbase_id: old111\ntype: old_type\n---\n# Title";
        let result = processor.inject_id_with_format(content, "new222", &fmt, Some("new_type"));
        assert!(result.contains("type: new_type\n"));
        assert!(!result.contains("type: old_type"));
    }

    #[test]
    fn test_inject_id_preserves_other_frontmatter_fields() {
        let processor = DocumentProcessor::new();
        let fmt = crate::models::format::ResolvedFormat::default();
        let content = "---\nfactbase_id: old111\nreviewed: 2026-01-01\ntags: [a, b]\n---\n# Title";
        let result = processor.inject_id_with_format(content, "new222", &fmt, None);
        assert!(result.contains("factbase_id: new222\n"));
        assert!(result.contains("reviewed: 2026-01-01\n"));
        assert!(result.contains("tags: [a, b]\n"));
    }
}
