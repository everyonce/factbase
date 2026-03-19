//! Retype execution for document reorganization.
//!
//! Overrides a document's type without moving the file. The type is stored
//! in the database and optionally persisted to the file via YAML frontmatter.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::{read_file, write_file};
use crate::processor::normalize_type;

/// Result of executing a retype operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetypeResult {
    /// ID of the retyped document
    pub doc_id: String,
    /// Original document type
    pub old_type: Option<String>,
    /// New document type
    pub new_type: String,
    /// Whether the type was persisted to the file
    pub persisted_to_file: bool,
}

/// Execute a retype operation, changing a document's type without moving it.
///
/// # Arguments
/// * `doc_id` - ID of the document to retype
/// * `new_type` - New type to assign
/// * `db` - Database connection
/// * `persist` - If true, adds type override comment to file
/// * `repo_path` - Path to the repository root (required if persist=true)
///
/// # Returns
/// `RetypeResult` with old and new types.
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist
/// - `FactbaseError::Io` on file operation failures (when persist=true)
/// - `FactbaseError::Database` on database errors
pub fn execute_retype(
    doc_id: &str,
    new_type: &str,
    db: &Database,
    persist: bool,
    repo_path: Option<&Path>,
) -> Result<RetypeResult, FactbaseError> {
    // Get the document
    let doc = db.require_document(doc_id)?;

    let old_type = doc.doc_type.clone();
    let normalized_type = normalize_type(new_type);

    // Update database
    db.update_document_type(doc_id, &normalized_type)?;

    // Optionally persist to file
    let persisted = if persist {
        let repo = repo_path
            .ok_or_else(|| FactbaseError::internal("repo_path required when persist=true"))?;
        let file_path = repo.join(&doc.file_path);
        persist_type_to_file(&file_path, &normalized_type)?;
        true
    } else {
        false
    };

    Ok(RetypeResult {
        doc_id: doc_id.to_string(),
        old_type,
        new_type: normalized_type,
        persisted_to_file: persisted,
    })
}

/// Persist type override to file by updating YAML frontmatter.
fn persist_type_to_file(file_path: &Path, new_type: &str) -> Result<(), FactbaseError> {
    let content = read_file(file_path)?;
    let updated = crate::processor::update_frontmatter_type(&content, new_type);
    write_file(file_path, &updated)?;
    Ok(())
}

/// Extract type override from YAML frontmatter if present.
pub fn extract_type_override(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("type:") {
            let t = val.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::make_test_doc as test_doc;
    use std::fs;

    #[test]
    fn test_retype_result_construction() {
        let result = RetypeResult {
            doc_id: "abc123".to_string(),
            old_type: Some("person".to_string()),
            new_type: "project".to_string(),
            persisted_to_file: false,
        };
        assert_eq!(result.doc_id, "abc123");
        assert_eq!(result.old_type, Some("person".to_string()));
        assert_eq!(result.new_type, "project");
        assert!(!result.persisted_to_file);
    }

    #[test]
    fn test_normalize_type() {
        assert_eq!(normalize_type("Person"), "person");
        assert_eq!(normalize_type("PROJECTS"), "projects");
        assert_eq!(normalize_type("notes"), "notes");
        assert_eq!(normalize_type("s"), "s");
    }

    #[test]
    fn test_persist_type_updates_frontmatter() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n\nContent";
        let result = crate::processor::update_frontmatter_type(content, "person");
        assert!(result.contains("type: person\n"));
        assert!(result.contains("factbase_id: abc123\n"));
    }

    #[test]
    fn test_persist_type_replaces_existing() {
        let content = "---\nfactbase_id: abc123\ntype: project\n---\n# Title\n\nContent";
        let result = crate::processor::update_frontmatter_type(content, "person");
        assert!(result.contains("type: person\n"));
        assert!(!result.contains("type: project"));
    }

    #[test]
    fn test_extract_type_override_present() {
        let content = "---\nfactbase_id: abc123\ntype: person\n---\n# Title";
        assert_eq!(extract_type_override(content), Some("person".to_string()));
    }

    #[test]
    fn test_extract_type_override_absent() {
        let content = "---\nfactbase_id: abc123\n---\n# Title";
        assert_eq!(extract_type_override(content), None);
    }

    #[test]
    fn test_execute_retype_document_not_found() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        let result = execute_retype("nonexistent", "person", &db, false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_execute_retype_db_only() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        let doc = test_doc("abc123", "Test Doc", "test.md", Some("project"));
        db.upsert_document(&doc).unwrap();

        let result = execute_retype("abc123", "person", &db, false, None).unwrap();

        assert_eq!(result.doc_id, "abc123");
        assert_eq!(result.old_type, Some("project".to_string()));
        assert_eq!(result.new_type, "person");
        assert!(!result.persisted_to_file);

        // Verify database updated
        let updated = db.get_document("abc123").unwrap().unwrap();
        assert_eq!(updated.doc_type, Some("person".to_string()));
    }

    #[test]
    fn test_execute_retype_with_persist() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create file
        let file_path = temp.path().join("test.md");
        let content = "---\nfactbase_id: abc123\n---\n# Test Doc\n\nContent here.";
        fs::write(&file_path, content).unwrap();

        let doc = test_doc("abc123", "Test Doc", "test.md", Some("project"));
        db.upsert_document(&doc).unwrap();

        let result = execute_retype("abc123", "person", &db, true, Some(temp.path())).unwrap();

        assert!(result.persisted_to_file);

        // Verify file updated
        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert!(updated_content.contains("type: person"));
    }

    #[test]
    fn test_execute_retype_persist_requires_repo_path() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        let doc = test_doc("abc123", "Test Doc", "test.md", Some("project"));
        db.upsert_document(&doc).unwrap();

        let result = execute_retype("abc123", "person", &db, true, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("repo_path"));
    }

    #[test]
    fn test_execute_retype_normalizes_type() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        let doc = test_doc("abc123", "Test Doc", "test.md", None);
        db.upsert_document(&doc).unwrap();

        let result = execute_retype("abc123", "PERSONS", &db, false, None).unwrap();
        assert_eq!(result.new_type, "persons");
    }
}
