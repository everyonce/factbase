//! Snapshot and rollback for reorganization operations.
//!
//! Creates backups of files and database state before destructive operations,
//! enabling rollback on failure and cleanup on success.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::Document;
use crate::organize::fs_helpers::{copy_file, create_dir, remove_dir, remove_file};

/// Directory name for snapshots within .factbase
const SNAPSHOT_DIR: &str = "snapshots";

/// A backup of a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBackup {
    /// Original file path (relative to repo)
    pub original_path: String,
    /// Backup file path (absolute)
    pub backup_path: PathBuf,
    /// Whether the file existed before the operation
    pub existed: bool,
}

/// A backup of a document's database state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBackup {
    /// Document ID
    pub id: String,
    /// Full document record (None if document didn't exist)
    pub document: Option<Document>,
    /// Whether the document was marked as deleted
    pub was_deleted: bool,
}

/// A snapshot of files and database state before a reorganization operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot ID (timestamp-based)
    pub id: String,
    /// When the snapshot was created
    pub created_at: DateTime<Local>,
    /// Repository path
    pub repo_path: PathBuf,
    /// Snapshot directory path
    pub snapshot_dir: PathBuf,
    /// File backups
    pub files: Vec<FileBackup>,
    /// Document backups
    pub documents: HashMap<String, DocumentBackup>,
}

impl Snapshot {
    /// Check if the snapshot is empty (no backups).
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.documents.is_empty()
    }

    /// Get the number of backed up files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get the number of backed up documents.
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
}

/// Generate a unique snapshot ID based on timestamp.
fn generate_snapshot_id() -> String {
    Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}

/// Get the snapshot directory for a repository.
fn snapshot_dir(repo_path: &Path) -> PathBuf {
    repo_path.join(".factbase").join(SNAPSHOT_DIR)
}

/// Create a snapshot of the specified documents before a reorganization operation.
///
/// Backs up both files and database state for the given document IDs.
///
/// # Arguments
/// * `doc_ids` - Document IDs to backup
/// * `db` - Database connection
/// * `repo_path` - Path to the repository root
///
/// # Returns
/// A `Snapshot` that can be used for rollback or cleanup.
pub fn create_snapshot(
    doc_ids: &[&str],
    db: &Database,
    repo_path: &Path,
) -> Result<Snapshot, FactbaseError> {
    let snapshot_id = generate_snapshot_id();
    let snap_dir = snapshot_dir(repo_path).join(&snapshot_id);

    // Create snapshot directory
    create_dir(&snap_dir)?;

    let mut files = Vec::new();
    let mut documents = HashMap::new();

    for doc_id in doc_ids {
        // Backup database state
        let doc = db.get_document(doc_id)?;
        let was_deleted = doc.as_ref().is_some_and(|d| d.is_deleted);

        documents.insert(
            doc_id.to_string(),
            DocumentBackup {
                id: doc_id.to_string(),
                document: doc.clone(),
                was_deleted,
            },
        );

        // Backup file if it exists
        if let Some(ref doc) = doc {
            let file_path = repo_path.join(&doc.file_path);
            let backup_path = snap_dir.join(format!("{doc_id}.md"));

            let existed = file_path.exists();
            if existed {
                copy_file(&file_path, &backup_path)?;
            }

            files.push(FileBackup {
                original_path: doc.file_path.clone(),
                backup_path,
                existed,
            });
        }
    }

    Ok(Snapshot {
        id: snapshot_id,
        created_at: Local::now(),
        repo_path: repo_path.to_path_buf(),
        snapshot_dir: snap_dir,
        files,
        documents,
    })
}

/// Rollback a snapshot, restoring files and database state.
///
/// # Arguments
/// * `snapshot` - The snapshot to rollback
/// * `db` - Database connection
///
/// # Returns
/// Number of items restored (files + documents).
pub fn rollback(snapshot: &Snapshot, db: &Database) -> Result<usize, FactbaseError> {
    let mut restored = 0;

    // Restore files
    for backup in &snapshot.files {
        let original_path = snapshot.repo_path.join(&backup.original_path);

        if backup.existed {
            // Restore from backup
            if backup.backup_path.exists() {
                // Ensure parent directory exists
                if let Some(parent) = original_path.parent() {
                    create_dir(parent)?;
                }
                copy_file(&backup.backup_path, &original_path)?;
                restored += 1;
            }
        } else {
            // File didn't exist before - remove it if it was created
            if original_path.exists() {
                remove_file(&original_path)?;
                restored += 1;
            }
        }
    }

    // Restore database state
    for backup in snapshot.documents.values() {
        if let Some(ref doc) = backup.document {
            // Restore the document record
            db.upsert_document(doc)?;

            // Restore deleted state if needed
            if backup.was_deleted && !doc.is_deleted {
                db.mark_deleted(&backup.id)?;
            }
            restored += 1;
        }
    }

    Ok(restored)
}

/// Clean up a snapshot after successful operation.
///
/// Removes the snapshot directory and all backup files.
///
/// # Arguments
/// * `snapshot` - The snapshot to clean up
pub fn cleanup(snapshot: &Snapshot) -> Result<(), FactbaseError> {
    if snapshot.snapshot_dir.exists() {
        remove_dir(&snapshot.snapshot_dir)?;
    }
    Ok(())
}

/// List all snapshots in a repository.
#[cfg(test)]
fn list_snapshots(repo_path: &Path) -> Result<Vec<PathBuf>, FactbaseError> {
    let snap_dir = snapshot_dir(repo_path);

    if !snap_dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots: Vec<PathBuf> = std::fs::read_dir(&snap_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    // Sort by name (which includes timestamp)
    snapshots.sort();

    Ok(snapshots)
}

/// Clean up old snapshots, keeping only the most recent N.
///
/// # Arguments
/// * `repo_path` - Path to the repository root
/// * `keep` - Number of recent snapshots to keep
///
/// # Returns
/// Number of snapshots removed.
#[cfg(test)]
fn cleanup_old_snapshots(repo_path: &Path, keep: usize) -> Result<usize, FactbaseError> {
    let snapshots = list_snapshots(repo_path)?;

    if snapshots.len() <= keep {
        return Ok(0);
    }

    let to_remove = snapshots.len() - keep;
    let mut removed = 0;

    for snapshot_path in snapshots.into_iter().take(to_remove) {
        remove_dir(&snapshot_path)?;
        removed += 1;
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::insert_test_doc as test_doc;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_struct() {
        let snapshot = Snapshot {
            id: "20260203-120000-000".to_string(),
            created_at: Local::now(),
            repo_path: PathBuf::from("/repo"),
            snapshot_dir: PathBuf::from("/repo/.factbase/snapshots/20260203-120000-000"),
            files: vec![],
            documents: HashMap::new(),
        };

        assert!(snapshot.is_empty());
        assert_eq!(snapshot.file_count(), 0);
        assert_eq!(snapshot.document_count(), 0);
    }

    #[test]
    fn test_snapshot_not_empty() {
        let mut documents = HashMap::new();
        documents.insert(
            "abc123".to_string(),
            DocumentBackup {
                id: "abc123".to_string(),
                document: None,
                was_deleted: false,
            },
        );

        let snapshot = Snapshot {
            id: "test".to_string(),
            created_at: Local::now(),
            repo_path: PathBuf::from("/repo"),
            snapshot_dir: PathBuf::from("/repo/.factbase/snapshots/test"),
            files: vec![FileBackup {
                original_path: "doc.md".to_string(),
                backup_path: PathBuf::from("/backup/doc.md"),
                existed: true,
            }],
            documents,
        };

        assert!(!snapshot.is_empty());
        assert_eq!(snapshot.file_count(), 1);
        assert_eq!(snapshot.document_count(), 1);
    }

    #[test]
    fn test_generate_snapshot_id() {
        let id1 = generate_snapshot_id();
        let _id2 = generate_snapshot_id();

        // IDs should be non-empty and contain timestamp format
        assert!(!id1.is_empty());
        assert!(id1.contains('-'));

        // IDs generated close together might be same or different
        // Just verify format
        assert!(id1.len() >= 15); // YYYYMMDD-HHMMSS-mmm
    }

    #[test]
    fn test_snapshot_dir() {
        let repo_path = Path::new("/home/user/repo");
        let dir = snapshot_dir(repo_path);
        assert_eq!(dir, PathBuf::from("/home/user/repo/.factbase/snapshots"));
    }

    #[test]
    fn test_create_snapshot_empty() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        let snapshot = create_snapshot(&[], &db, repo_path).expect("create snapshot");

        assert!(snapshot.is_empty());
        assert!(snapshot.snapshot_dir.exists());

        // Cleanup
        cleanup(&snapshot).expect("cleanup");
        assert!(!snapshot.snapshot_dir.exists());
    }

    #[test]
    fn test_create_snapshot_with_file() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test file
        let doc_path = repo_path.join("doc1.md");
        fs::write(&doc_path, "<!-- factbase:doc1 -->\n# Doc 1\nContent").unwrap();

        // Create document in database
        test_doc(&db, "doc1", "repo1", "Doc 1", "Content", "doc1.md");

        let snapshot = create_snapshot(&["doc1"], &db, repo_path).expect("create snapshot");

        assert!(!snapshot.is_empty());
        assert_eq!(snapshot.file_count(), 1);
        assert_eq!(snapshot.document_count(), 1);

        // Verify backup file exists
        let backup = &snapshot.files[0];
        assert!(backup.backup_path.exists());
        assert!(backup.existed);

        // Verify document backup
        let doc_backup = snapshot.documents.get("doc1").unwrap();
        assert!(doc_backup.document.is_some());
        assert!(!doc_backup.was_deleted);

        // Cleanup
        cleanup(&snapshot).expect("cleanup");
    }

    #[test]
    fn test_create_snapshot_nonexistent_doc() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Snapshot a document that doesn't exist
        let snapshot = create_snapshot(&["nonexistent"], &db, repo_path).expect("create snapshot");

        // Should still create entry but with None document
        assert_eq!(snapshot.document_count(), 1);
        let doc_backup = snapshot.documents.get("nonexistent").unwrap();
        assert!(doc_backup.document.is_none());

        cleanup(&snapshot).expect("cleanup");
    }

    #[test]
    fn test_rollback_restores_file() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test file
        let doc_path = repo_path.join("doc1.md");
        let original_content = "<!-- factbase:doc1 -->\n# Doc 1\nOriginal";
        fs::write(&doc_path, original_content).unwrap();

        test_doc(&db, "doc1", "repo1", "Doc 1", "Original", "doc1.md");

        // Create snapshot
        let snapshot = create_snapshot(&["doc1"], &db, repo_path).expect("create snapshot");

        // Modify the file
        fs::write(&doc_path, "Modified content").unwrap();

        // Rollback
        let restored = rollback(&snapshot, &db).expect("rollback");
        assert!(restored > 0);

        // Verify file was restored
        let content = fs::read_to_string(&doc_path).unwrap();
        assert_eq!(content, original_content);

        cleanup(&snapshot).expect("cleanup");
    }

    #[test]
    fn test_rollback_removes_new_file() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create document in DB but no file
        test_doc(&db, "doc1", "repo1", "Doc 1", "Content", "doc1.md");

        // Create snapshot (file doesn't exist)
        let snapshot = create_snapshot(&["doc1"], &db, repo_path).expect("create snapshot");

        // Verify file backup shows it didn't exist
        let backup = &snapshot.files[0];
        assert!(!backup.existed);

        // Create the file (simulating operation that creates it)
        let doc_path = repo_path.join("doc1.md");
        fs::write(&doc_path, "New content").unwrap();
        assert!(doc_path.exists());

        // Rollback should remove the file
        rollback(&snapshot, &db).expect("rollback");
        assert!(!doc_path.exists());

        cleanup(&snapshot).expect("cleanup");
    }

    #[test]
    fn test_rollback_restores_database() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test file and document
        let doc_path = repo_path.join("doc1.md");
        fs::write(&doc_path, "Original").unwrap();
        test_doc(&db, "doc1", "repo1", "Doc 1", "Original", "doc1.md");

        // Create snapshot
        let snapshot = create_snapshot(&["doc1"], &db, repo_path).expect("create snapshot");

        // Modify document in database
        let mut doc = db.get_document("doc1").unwrap().unwrap();
        doc.title = "Modified Title".to_string();
        doc.content = "Modified Content".to_string();
        db.upsert_document(&doc).unwrap();

        // Verify modification
        let modified = db.get_document("doc1").unwrap().unwrap();
        assert_eq!(modified.title, "Modified Title");

        // Rollback
        rollback(&snapshot, &db).expect("rollback");

        // Verify database was restored
        let restored = db.get_document("doc1").unwrap().unwrap();
        assert_eq!(restored.title, "Doc 1");
        assert_eq!(restored.content, "Original");

        cleanup(&snapshot).expect("cleanup");
    }

    #[test]
    fn test_list_snapshots_empty() {
        let temp = TempDir::new().unwrap();
        let snapshots = list_snapshots(temp.path()).expect("list snapshots");
        assert!(snapshots.is_empty());
    }

    #[test]
    fn test_list_snapshots_sorted() {
        let temp = TempDir::new().unwrap();
        let snap_dir = snapshot_dir(temp.path());
        fs::create_dir_all(&snap_dir).unwrap();

        // Create snapshots out of order
        fs::create_dir_all(snap_dir.join("20260203-140000-000")).unwrap();
        fs::create_dir_all(snap_dir.join("20260201-100000-000")).unwrap();
        fs::create_dir_all(snap_dir.join("20260202-120000-000")).unwrap();

        let snapshots = list_snapshots(temp.path()).expect("list snapshots");
        assert_eq!(snapshots.len(), 3);

        // Should be sorted by name (timestamp)
        assert!(snapshots[0].to_string_lossy().contains("20260201"));
        assert!(snapshots[1].to_string_lossy().contains("20260202"));
        assert!(snapshots[2].to_string_lossy().contains("20260203"));
    }

    #[test]
    fn test_cleanup_old_snapshots() {
        let temp = TempDir::new().unwrap();
        let snap_dir = snapshot_dir(temp.path());
        fs::create_dir_all(&snap_dir).unwrap();

        // Create 5 snapshots
        for i in 1..=5 {
            fs::create_dir_all(snap_dir.join(format!("2026020{}-120000-000", i))).unwrap();
        }

        // Keep only 2
        let removed = cleanup_old_snapshots(temp.path(), 2).expect("cleanup old");
        assert_eq!(removed, 3);

        let remaining = list_snapshots(temp.path()).expect("list");
        assert_eq!(remaining.len(), 2);

        // Should keep the most recent (04 and 05)
        assert!(remaining[0].to_string_lossy().contains("20260204"));
        assert!(remaining[1].to_string_lossy().contains("20260205"));
    }

    #[test]
    fn test_cleanup_old_snapshots_nothing_to_remove() {
        let temp = TempDir::new().unwrap();
        let snap_dir = snapshot_dir(temp.path());
        fs::create_dir_all(&snap_dir).unwrap();

        // Create 2 snapshots
        fs::create_dir_all(snap_dir.join("20260201-120000-000")).unwrap();
        fs::create_dir_all(snap_dir.join("20260202-120000-000")).unwrap();

        // Keep 5 (more than exist)
        let removed = cleanup_old_snapshots(temp.path(), 5).expect("cleanup old");
        assert_eq!(removed, 0);

        let remaining = list_snapshots(temp.path()).expect("list");
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn test_file_backup_struct() {
        let backup = FileBackup {
            original_path: "docs/note.md".to_string(),
            backup_path: PathBuf::from("/backup/abc123.md"),
            existed: true,
        };

        assert_eq!(backup.original_path, "docs/note.md");
        assert!(backup.existed);
    }

    #[test]
    fn test_document_backup_struct() {
        let backup = DocumentBackup {
            id: "abc123".to_string(),
            document: None,
            was_deleted: false,
        };

        assert_eq!(backup.id, "abc123");
        assert!(backup.document.is_none());
        assert!(!backup.was_deleted);
    }
}
