//! Shared test helpers for lint execute modules.

use chrono::Utc;
use factbase::Document;

/// Create a test Document with the given content and default fields.
pub fn make_test_doc(content: &str) -> Document {
    Document {
        id: "abc123".to_string(),
        title: "Test".to_string(),
        content: content.to_string(),
        doc_type: Some("note".to_string()),
        file_path: "/test.md".to_string(),
        file_hash: "hash".to_string(),
        repo_id: "repo".to_string(),
        indexed_at: Utc::now(),
        file_modified_at: Some(Utc::now()),
        is_deleted: false,
    }
}

/// Create a test Document with a custom ID and content.
pub fn make_test_doc_with_id(id: &str, content: &str) -> Document {
    Document {
        id: id.to_string(),
        title: format!("Test {}", id),
        file_path: format!("/test_{}.md", id),
        repo_id: "test-repo".to_string(),
        ..make_test_doc(content)
    }
}
