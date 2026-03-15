//! Move execution for document reorganization.
//!
//! Moves a document to a new folder, updating its type based on the
//! destination folder. Links remain unchanged since they're ID-based.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::processor::DocumentProcessor;

/// Result of executing a move operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResult {
    /// ID of the moved document
    pub doc_id: String,
    /// Original file path (relative to repo)
    pub old_path: String,
    /// New file path (relative to repo)
    pub new_path: String,
    /// Original document type
    pub old_type: Option<String>,
    /// New document type (derived from destination folder)
    pub new_type: String,
}

/// Execute a move operation, relocating a document to a new folder.
///
/// The document's type is automatically derived from the destination folder.
/// Links to/from this document remain unchanged since they're ID-based.
///
/// # Arguments
/// * `doc_id` - ID of the document to move
/// * `new_path` - New path relative to repo root (e.g., "projects/doc.md")
/// * `db` - Database connection
/// * `repo_path` - Path to the repository root
///
/// # Returns
/// `MoveResult` with old and new paths/types.
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist
/// - `FactbaseError::Io` on file operation failures
/// - `FactbaseError::Database` on database errors
pub fn execute_move(
    doc_id: &str,
    new_path: &Path,
    db: &Database,
    repo_path: &Path,
) -> Result<MoveResult, FactbaseError> {
    // Get the document
    let doc = db.require_document(doc_id)?;

    let old_path = doc.file_path.clone();
    let old_abs_path = repo_path.join(&old_path);
    let new_abs_path = repo_path.join(new_path);

    // Validate source exists
    if !old_abs_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "Source file not found: {}",
            old_abs_path.display()
        )));
    }

    // Validate destination doesn't already exist
    if new_abs_path.exists() {
        return Err(FactbaseError::internal(format!(
            "Destination already exists: {}",
            new_abs_path.display()
        )));
    }

    // Create destination directory if needed
    if let Some(parent) = new_abs_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Move the file (preserves content including factbase ID header)
    fs::rename(&old_abs_path, &new_abs_path)?;

    // Derive new type from destination folder
    let processor = DocumentProcessor::new();
    let new_type = processor.derive_type(new_path, Path::new(""));

    // Update database with new path and type
    let new_path_str = new_path.to_string_lossy().to_string();
    let mut updated_doc = doc.clone();
    updated_doc.file_path = new_path_str.clone();
    updated_doc.doc_type = Some(new_type.clone());
    db.upsert_document(&updated_doc)?;

    Ok(MoveResult {
        doc_id: doc_id.to_string(),
        old_path,
        new_path: new_path_str,
        old_type: doc.doc_type,
        new_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::make_test_doc as test_doc;

    #[test]
    fn test_move_result_construction() {
        let result = MoveResult {
            doc_id: "abc123".to_string(),
            old_path: "people/john.md".to_string(),
            new_path: "projects/john.md".to_string(),
            old_type: Some("person".to_string()),
            new_type: "project".to_string(),
        };
        assert_eq!(result.doc_id, "abc123");
        assert_eq!(result.old_path, "people/john.md");
        assert_eq!(result.new_path, "projects/john.md");
        assert_eq!(result.old_type, Some("person".to_string()));
        assert_eq!(result.new_type, "project");
    }

    #[test]
    fn test_execute_move_document_not_found() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        let result = execute_move("nonexistent", Path::new("new/path.md"), &db, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_execute_move_source_file_missing() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create doc in DB but not on disk
        let doc = test_doc("abc123", "Test Doc", "people/test.md", Some("person"));
        db.upsert_document(&doc).unwrap();

        let result = execute_move("abc123", Path::new("projects/test.md"), &db, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_execute_move_destination_exists() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create source file
        let people_dir = temp.path().join("people");
        fs::create_dir_all(&people_dir).unwrap();
        let source_path = people_dir.join("test.md");
        fs::write(&source_path, "---\nfactbase_id: abc123\n---\n# Test").unwrap();

        // Create destination file
        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let dest_path = projects_dir.join("test.md");
        fs::write(&dest_path, "existing content").unwrap();

        // Create doc in DB
        let doc = test_doc("abc123", "Test Doc", "people/test.md", Some("person"));
        db.upsert_document(&doc).unwrap();

        let result = execute_move("abc123", Path::new("projects/test.md"), &db, temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_execute_move_success() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create source file
        let people_dir = temp.path().join("people");
        fs::create_dir_all(&people_dir).unwrap();
        let source_path = people_dir.join("john.md");
        let content = "---\nfactbase_id: abc123\n---\n# John Doe\n\nSome content.";
        fs::write(&source_path, content).unwrap();

        // Create doc in DB
        let doc = test_doc("abc123", "John Doe", "people/john.md", Some("person"));
        db.upsert_document(&doc).unwrap();

        // Execute move
        let result =
            execute_move("abc123", Path::new("projects/john.md"), &db, temp.path()).unwrap();

        // Verify result
        assert_eq!(result.doc_id, "abc123");
        assert_eq!(result.old_path, "people/john.md");
        assert_eq!(result.new_path, "projects/john.md");
        assert_eq!(result.old_type, Some("person".to_string()));
        assert_eq!(result.new_type, "project");

        // Verify file moved
        assert!(!source_path.exists());
        let new_path = temp.path().join("projects/john.md");
        assert!(new_path.exists());

        // Verify content preserved (including factbase header)
        let moved_content = fs::read_to_string(&new_path).unwrap();
        assert!(moved_content.contains("---\nfactbase_id: abc123\n---"));
        assert!(moved_content.contains("# John Doe"));

        // Verify database updated
        let updated_doc = db.get_document("abc123").unwrap().unwrap();
        assert_eq!(updated_doc.file_path, "projects/john.md");
        assert_eq!(updated_doc.doc_type, Some("project".to_string()));
    }

    #[test]
    fn test_execute_move_creates_destination_dir() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create source file at root
        let source_path = temp.path().join("test.md");
        fs::write(&source_path, "---\nfactbase_id: abc123\n---\n# Test").unwrap();

        // Create doc in DB
        let doc = test_doc("abc123", "Test", "test.md", None);
        db.upsert_document(&doc).unwrap();

        // Move to nested directory that doesn't exist
        let result = execute_move(
            "abc123",
            Path::new("deep/nested/folder/test.md"),
            &db,
            temp.path(),
        )
        .unwrap();

        // Verify directory was created
        assert!(temp.path().join("deep/nested/folder").exists());
        assert!(temp.path().join("deep/nested/folder/test.md").exists());
        assert_eq!(result.new_type, "folder");
    }

    #[test]
    fn test_execute_move_type_derived_from_folder() {
        let (db, temp) = test_db();
        test_repo(&db, "test", temp.path());

        // Create source file
        let source_path = temp.path().join("test.md");
        fs::write(&source_path, "---\nfactbase_id: abc123\n---\n# Test").unwrap();

        let doc = test_doc("abc123", "Test", "test.md", None);
        db.upsert_document(&doc).unwrap();

        // Move to "People" folder (normalizes to lowercase "people")
        let result = execute_move("abc123", Path::new("People/test.md"), &db, temp.path()).unwrap();
        assert_eq!(result.new_type, "people");

        // Move to "persons" folder (singularizes to "person")
        let result2 =
            execute_move("abc123", Path::new("persons/test.md"), &db, temp.path()).unwrap();
        assert_eq!(result2.new_type, "person");
    }
}
