use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::TemporalTag;

/// A document indexed by factbase, representing a single markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique 6-character hex identifier (e.g., "a1b2c3")
    pub id: String,
    /// Repository this document belongs to
    pub repo_id: String,
    /// Path to the source markdown file
    pub file_path: String,
    /// SHA256 hash of file content for change detection
    pub file_hash: String,
    /// Document title extracted from first H1 or filename
    pub title: String,
    /// Type derived from parent folder (e.g., "person", "project")
    pub doc_type: Option<String>,
    /// Full markdown content of the document
    pub content: String,
    /// Last modification time of the source file
    pub file_modified_at: Option<DateTime<Utc>>,
    /// When this document was last indexed
    pub indexed_at: DateTime<Utc>,
    /// Whether this document has been soft-deleted
    pub is_deleted: bool,
}

impl Document {
    /// Parse temporal tags from document content on-demand.
    /// Tags are not stored in the database - this parses fresh from content each call.
    pub fn temporal_tags(&self) -> Vec<TemporalTag> {
        crate::processor::parse_temporal_tags(&self.content)
    }

    /// Returns a 4-field JSON summary: `id`, `title`, `type`, `file_path`.
    pub fn to_summary_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "type": self.doc_type,
            "file_path": self.file_path
        })
    }

    /// Create a Document with sensible test defaults. Override fields with struct update syntax:
    /// ```ignore
    /// Document { id: "custom".into(), content: "custom".into(), ..Document::test_default() }
    /// ```
    #[cfg(test)]
    pub(crate) fn test_default() -> Self {
        Self {
            id: "test01".to_string(),
            repo_id: "test-repo".to_string(),
            file_path: "test01.md".to_string(),
            file_hash: "abc123".to_string(),
            title: "Test Document".to_string(),
            doc_type: Some("note".to_string()),
            content: "# Test Document\n\nContent here.".to_string(),
            file_modified_at: None,
            indexed_at: Utc::now(),
            is_deleted: false,
        }
    }
}

/// Count words in text using whitespace splitting.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("hello world"), 2);
        assert_eq!(word_count("  spaced  out  "), 2);
        assert_eq!(word_count("one"), 1);
    }

    #[test]
    fn test_to_summary_json() {
        let doc = Document {
            doc_type: Some("person".into()),
            ..Document::test_default()
        };
        let json = doc.to_summary_json();
        assert_eq!(json["id"], "test01");
        assert_eq!(json["title"], "Test Document");
        assert_eq!(json["type"], "person");
        assert_eq!(json["file_path"], "test01.md");
        assert!(json.get("content").is_none());
    }
}
