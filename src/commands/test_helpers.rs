//! Shared test helpers for binary-crate test modules.

use chrono::Utc;
use factbase::{Database, Document, Repository};
use std::path::PathBuf;
use tempfile::TempDir;

pub fn test_db() -> (Database, TempDir) {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().join("test.db");
    let db = Database::new(&db_path).expect("failed to create database");
    (db, tmp)
}

/// Returns a Repository with sensible defaults. Use struct update syntax to override fields:
/// ```ignore
/// let repo = Repository { id: "custom".into(), ..make_test_repo() };
/// ```
pub fn make_test_repo() -> Repository {
    Repository {
        id: "test-repo".to_string(),
        name: "Test Repo".to_string(),
        path: PathBuf::from("/tmp/test"),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_check_at: None,
    }
}

/// Returns a Document with sensible defaults for the given ID. Use struct update syntax to override:
/// ```ignore
/// let doc = Document { title: "Custom".into(), ..make_test_doc("abc123") };
/// ```
pub fn make_test_doc(id: &str) -> Document {
    Document {
        id: id.to_string(),
        repo_id: "test-repo".to_string(),
        title: format!("Doc {id}"),
        doc_type: Some("document".to_string()),
        content: "content".to_string(),
        file_path: format!("{id}.md"),
        file_hash: "hash".to_string(),
        file_modified_at: None,
        indexed_at: Utc::now(),
        is_deleted: false,
    }
}
