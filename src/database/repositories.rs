//! Repository CRUD operations.
//!
//! This module handles:
//! - Repository insertion and updates ([`Database::upsert_repository`], [`Database::add_repository`])
//! - Repository retrieval ([`Database::get_repository`], [`Database::get_repository_by_path`])
//! - Repository listing ([`Database::list_repositories`], [`Database::list_repositories_with_stats`])
//! - Repository removal ([`Database::remove_repository`])
//! - Metadata updates ([`Database::update_last_check_at`])
//!
//! # Repository Identity
//!
//! Repositories are identified by a unique string ID and must have
//! a unique filesystem path. The [`Database::add_repository`] function enforces
//! these constraints.

use crate::error::FactbaseError;
use crate::models::Repository;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use super::{repo_not_found, Database};

/// Column list for SELECT queries that map to `row_to_repository()`.
const REPOSITORY_COLUMNS: &str =
    "id, name, path, perspective, created_at, last_indexed_at, last_check_at";

impl Database {
    /// Inserts or updates a repository record.
    ///
    /// If a repository with the same ID exists, it will be replaced.
    /// Use `add_repository` if you want to prevent overwrites.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn upsert_repository(&self, repo: &Repository) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let perspective = match &repo.perspective {
            Some(p) => Some(serde_json::to_string(p)?),
            None => None,
        };
        conn.execute(
            "INSERT OR REPLACE INTO repositories (id, name, path, perspective, created_at, last_indexed_at, last_check_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                repo.id, repo.name, repo.path.to_string_lossy(), perspective,
                repo.created_at.to_rfc3339(), repo.last_indexed_at.map(|t| t.to_rfc3339()),
                repo.last_check_at.map(|t| t.to_rfc3339())
            ],
        )?;
        Ok(())
    }

    /// Retrieves a repository by its unique identifier.
    ///
    /// # Returns
    /// `Ok(Some(repo))` if found, `Ok(None)` if not found.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_repository(&self, id: &str) -> Result<Option<Repository>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE id = ?1"
        ))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_repository(row)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves a repository by ID, returning an error if not found.
    ///
    /// Convenience wrapper around [`get_repository`](Self::get_repository) that converts
    /// `None` into a [`FactbaseError::NotFound`] error.
    ///
    /// # Errors
    /// Returns `FactbaseError::NotFound` if the repository doesn't exist,
    /// or `FactbaseError::Database` on SQL errors.
    pub fn require_repository(&self, id: &str) -> Result<Repository, FactbaseError> {
        self.get_repository(id)?.ok_or_else(|| repo_not_found(id))
    }

    /// Lists all registered repositories, ordered by name.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn list_repositories(&self) -> Result<Vec<Repository>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {REPOSITORY_COLUMNS} FROM repositories ORDER BY name"
        ))?;
        let mut repos = Vec::with_capacity(8);
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            repos.push(Self::row_to_repository(row)?);
        }
        Ok(repos)
    }

    /// Add a new repository. Returns error if ID or path already exists.
    pub fn add_repository(&self, repo: &Repository) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        // Check if ID already exists
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM repositories WHERE id = ?1)",
            [&repo.id],
            |r| r.get(0),
        )?;
        if exists {
            return Err(FactbaseError::config(format!(
                "Repository with ID '{}' already exists",
                repo.id
            )));
        }
        // Check if path already registered
        let path_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM repositories WHERE path = ?1)",
            [repo.path.to_string_lossy().as_ref()],
            |r| r.get(0),
        )?;
        if path_exists {
            return Err(FactbaseError::config(format!(
                "Path '{}' is already registered",
                repo.path.display()
            )));
        }
        drop(conn);
        self.upsert_repository(repo)
    }

    /// Remove a repository and delete its documents.
    pub fn remove_repository(&self, id: &str) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        // Get document IDs for this repo to clean up links and embeddings
        let mut stmt = conn.prepare_cached("SELECT id FROM documents WHERE repo_id = ?1")?;
        let doc_ids: Vec<String> = stmt
            .query_map([id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Delete links and embeddings involving these documents
        for doc_id in &doc_ids {
            conn.execute(
                "DELETE FROM document_links WHERE source_id = ?1 OR target_id = ?1",
                [doc_id],
            )?;
            // Delete from chunks table first
            conn.execute(
                "DELETE FROM embedding_chunks WHERE document_id = ?1",
                [doc_id],
            )?;
            // Delete embeddings using LIKE pattern for all chunks
            conn.execute(
                "DELETE FROM document_embeddings WHERE id LIKE ?1",
                [format!("{doc_id}_%")],
            )?;
        }

        // Delete all documents for this repo
        let deleted: usize = conn.execute("DELETE FROM documents WHERE repo_id = ?1", [id])?;
        conn.execute("DELETE FROM repositories WHERE id = ?1", [id])?;
        Ok(deleted)
    }

    /// Find repository by filesystem path.
    pub fn get_repository_by_path(&self, path: &Path) -> Result<Option<Repository>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE path = ?1"
        ))?;
        let mut rows = stmt.query([path.to_string_lossy().as_ref()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_repository(row)?))
        } else {
            Ok(None)
        }
    }

    /// Update the last lint timestamp for a repository.
    pub fn update_last_check_at(
        &self,
        repo_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "UPDATE repositories SET last_check_at = ?1 WHERE id = ?2",
            rusqlite::params![timestamp.to_rfc3339(), repo_id],
        )?;
        Ok(())
    }

    /// List repositories with document counts.
    pub fn list_repositories_with_stats(&self) -> Result<Vec<(Repository, usize)>, FactbaseError> {
        let repos = self.list_repositories()?;
        let conn = self.get_conn()?;
        let mut results = Vec::with_capacity(repos.len());
        for repo in repos {
            let count: usize = conn.query_row(
                "SELECT COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
                [&repo.id],
                |r| r.get(0),
            )?;
            results.push((repo, count));
        }
        Ok(results)
    }

    /// Convert a database row to a Repository struct.
    pub(crate) fn row_to_repository(row: &rusqlite::Row) -> Result<Repository, FactbaseError> {
        let perspective_str: Option<String> = row.get(3)?;
        let perspective = perspective_str.and_then(|s| serde_json::from_str(&s).ok());
        let created_str: String = row.get(4)?;
        let last_indexed_str: Option<String> = row.get(5)?;
        let last_check_str: Option<String> = row.get(6)?;
        Ok(Repository {
            id: row.get(0)?,
            name: row.get(1)?,
            path: PathBuf::from(row.get::<_, String>(2)?),
            perspective,
            created_at: super::parse_rfc3339_utc(&created_str),
            last_indexed_at: last_indexed_str.and_then(|s| super::parse_rfc3339_utc_opt(&s)),
            last_check_at: last_check_str.and_then(|s| super::parse_rfc3339_utc_opt(&s)),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_add_repository_success() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo)
            .expect("add_repository should succeed");

        let retrieved = db
            .get_repository("test-repo")
            .expect("get_repository should succeed");
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.expect("repository should exist").name,
            "Test Repo"
        );
    }

    #[test]
    fn test_add_repository_duplicate_id() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo)
            .expect("add_repository should succeed");

        let result = db.add_repository(&repo);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_repository_duplicate_path() {
        let (db, _tmp) = test_db();
        let repo1 = test_repo();
        db.add_repository(&repo1)
            .expect("add_repository should succeed");

        let mut repo2 = test_repo();
        repo2.id = "other".to_string();
        let result = db.add_repository(&repo2);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_repository() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo)
            .expect("add_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");

        let deleted = db
            .remove_repository("test-repo")
            .expect("remove_repository should succeed");
        assert_eq!(deleted, 1);

        let retrieved = db
            .get_repository("test-repo")
            .expect("get_repository should succeed");
        assert!(retrieved.is_none());

        // Document should be deleted
        let doc = db
            .get_document("doc1")
            .expect("get_document should succeed");
        assert!(doc.is_none());
    }

    #[test]
    fn test_get_repository_by_path() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo)
            .expect("add_repository should succeed");

        let found = db
            .get_repository_by_path(&std::path::PathBuf::from("/tmp/test"))
            .expect("get_repository_by_path should succeed");
        assert!(found.is_some());
        assert_eq!(found.expect("repository should exist").id, "test-repo");

        let not_found = db
            .get_repository_by_path(&std::path::PathBuf::from("/nonexistent"))
            .expect("get_repository_by_path should succeed");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_require_repository_found() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo).unwrap();
        let result = db.require_repository("test-repo");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "test-repo");
    }

    #[test]
    fn test_require_repository_not_found() {
        let (db, _tmp) = test_db();
        let result = db.require_repository("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn test_list_repositories_with_stats() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.add_repository(&repo)
            .expect("add_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");

        let repos = db
            .list_repositories_with_stats()
            .expect("list_repositories_with_stats should succeed");
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].0.id, "test-repo");
        assert_eq!(repos[0].1, 2);
    }
}
