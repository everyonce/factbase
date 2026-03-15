//! Document link operations.
//!
//! This module handles:
//! - Link updates (update_links)
//! - Link retrieval (get_links_from, get_links_to, get_links_for_documents)
//! - Document title listing for link detection (get_all_document_titles)
//!
//! # Link Model
//!
//! Links are directional relationships between documents.
//! Each link has a source document, target document, and optional context.
//! Links are stored in the `document_links` table.

use std::collections::HashMap;

use crate::error::FactbaseError;
use crate::link_detection::DetectedLink;
use crate::models::Link;
use chrono::Utc;

use super::Database;

/// Type alias for batch link results: HashMap of doc_id -> (outgoing_links, incoming_links)
pub type DocumentLinksMap = HashMap<String, (Vec<Link>, Vec<Link>)>;

impl Database {
    /// Gets document titles for a set of IDs. Returns a map of id → title.
    pub fn get_document_titles_by_ids(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, String>, FactbaseError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.get_conn()?;
        let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, title FROM documents WHERE is_deleted = FALSE AND id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt.query(params.as_slice())?;
        let mut map = HashMap::new();
        while let Some(row) = rows.next()? {
            map.insert(row.get(0)?, row.get(1)?);
        }
        Ok(map)
    }

    /// Gets all document IDs and titles for link detection.
    ///
    /// Used by the link detector to build the entity list for LLM prompts.
    /// Returns only non-deleted documents.
    /// Uses prepared statement caching for performance on repeated calls.
    ///
    /// # Arguments
    /// * `repo_id` - Optional filter by repository (None = all repos)
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_all_document_titles(
        &self,
        repo_id: Option<&str>,
    ) -> Result<Vec<(String, String)>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut results = Vec::new();
        match repo_id {
            Some(r) => {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title FROM documents WHERE is_deleted = FALSE AND repo_id = ?1",
                )?;
                let mut rows = stmt.query([r])?;
                while let Some(row) = rows.next()? {
                    results.push((row.get(0)?, row.get(1)?));
                }
            }
            None => {
                let mut stmt = conn
                    .prepare_cached("SELECT id, title FROM documents WHERE is_deleted = FALSE")?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    results.push((row.get(0)?, row.get(1)?));
                }
            }
        }
        Ok(results)
    }

    /// Replaces all outgoing links from a document.
    ///
    /// Deletes existing links from the source document and inserts new ones.
    /// Used after LLM link detection to update the graph.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn update_links(
        &self,
        source_id: &str,
        links: &[DetectedLink],
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "DELETE FROM document_links WHERE source_id = ?1",
            [source_id],
        )?;

        let now = Utc::now().to_rfc3339();
        for link in links {
            conn.execute(
                "INSERT OR IGNORE INTO document_links (source_id, target_id, context, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![source_id, link.target_id, link.context, now],
            )?;
        }
        Ok(())
    }

    /// Gets all outgoing links from a document.
    ///
    /// Returns links where the specified document is the source.
    /// Uses prepared statement caching for performance on repeated calls.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_links_from(&self, source_id: &str) -> Result<Vec<Link>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached("SELECT source_id, target_id, context, created_at FROM document_links WHERE source_id = ?1")?;
        let mut rows = stmt.query([source_id])?;
        let mut links = Vec::new();
        while let Some(row) = rows.next()? {
            links.push(Self::row_to_link(row)?);
        }
        Ok(links)
    }

    /// Gets all incoming links to a document.
    ///
    /// Returns links where the specified document is the target.
    /// Uses prepared statement caching for performance on repeated calls.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_links_to(&self, target_id: &str) -> Result<Vec<Link>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached("SELECT source_id, target_id, context, created_at FROM document_links WHERE target_id = ?1")?;
        let mut rows = stmt.query([target_id])?;
        let mut links = Vec::new();
        while let Some(row) = rows.next()? {
            links.push(Self::row_to_link(row)?);
        }
        Ok(links)
    }

    /// Gets all links (outgoing and incoming) for multiple documents in batch.
    ///
    /// Eliminates N+1 query pattern by fetching all links in two queries
    /// instead of 2*N queries.
    ///
    /// # Arguments
    /// * `doc_ids` - Slice of document IDs to fetch links for
    ///
    /// # Returns
    /// HashMap where key is document ID and value is tuple of (outgoing, incoming) links.
    /// Documents with no links will have empty vectors.
    ///
    /// # Errors
    /// Returns `FactbaseError::Database` on SQL errors.
    pub fn get_links_for_documents(
        &self,
        doc_ids: &[&str],
    ) -> Result<DocumentLinksMap, FactbaseError> {
        if doc_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = self.get_conn()?;

        // Initialize result map with empty vectors for all requested IDs
        let mut result: HashMap<String, (Vec<Link>, Vec<Link>)> = doc_ids
            .iter()
            .map(|id| ((*id).to_string(), (Vec::new(), Vec::new())))
            .collect();

        // Build placeholders for IN clause
        let placeholders: String = doc_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        // Fetch outgoing links (where doc is source)
        let outgoing_sql = format!(
            "SELECT source_id, target_id, context, created_at FROM document_links WHERE source_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&outgoing_sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            doc_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let link = Self::row_to_link(row)?;
            if let Some((outgoing, _)) = result.get_mut(&link.source_id) {
                outgoing.push(link);
            }
        }

        // Fetch incoming links (where doc is target)
        let incoming_sql = format!(
            "SELECT source_id, target_id, context, created_at FROM document_links WHERE target_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&incoming_sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            doc_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let link = Self::row_to_link(row)?;
            if let Some((_, incoming)) = result.get_mut(&link.target_id) {
                incoming.push(link);
            }
        }

        Ok(result)
    }

    /// Gets outgoing link count for each document in a repository.
    ///
    /// Returns (doc_id, title, outgoing_link_count) for all non-deleted documents.
    pub fn get_document_link_counts(
        &self,
        repo_id: Option<&str>,
    ) -> Result<Vec<(String, String, String, usize)>, FactbaseError> {
        let conn = self.get_conn()?;
        let sql = match repo_id {
            Some(_) => {
                "SELECT d.id, d.title, COALESCE(d.doc_type, ''), COUNT(dl.target_id) as link_count
                 FROM documents d
                 LEFT JOIN document_links dl ON d.id = dl.source_id
                 WHERE d.is_deleted = FALSE AND d.repo_id = ?1
                 GROUP BY d.id, d.title, d.doc_type"
            }
            None => {
                "SELECT d.id, d.title, COALESCE(d.doc_type, ''), COUNT(dl.target_id) as link_count
                 FROM documents d
                 LEFT JOIN document_links dl ON d.id = dl.source_id
                 WHERE d.is_deleted = FALSE
                 GROUP BY d.id, d.title, d.doc_type"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let mut results = Vec::new();
        let mut rows = match repo_id {
            Some(r) => stmt.query([r])?,
            None => stmt.query([])?,
        };
        while let Some(row) = rows.next()? {
            let count: i64 = row.get(3)?;
            results.push((row.get(0)?, row.get(1)?, row.get(2)?, count as usize));
        }
        Ok(results)
    }

    /// Adds links from a source document to target documents without replacing existing links.
    ///
    /// Unlike `update_links` which replaces all links, this appends new links.
    /// Returns the number of links actually inserted (skips existing).
    pub fn add_links(&self, source_id: &str, target_ids: &[&str]) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let now = Utc::now().to_rfc3339();
        let mut added = 0;
        for target_id in target_ids {
            let result = conn.execute(
                "INSERT OR IGNORE INTO document_links (source_id, target_id, context, created_at) VALUES (?1, ?2, '', ?3)",
                rusqlite::params![source_id, target_id, now],
            )?;
            added += result;
        }
        Ok(added)
    }

    /// Returns outgoing and incoming link counts for multiple documents in batch.
    ///
    /// Eliminates N+1 query pattern when only counts are needed (not full link data).
    /// Returns a map of doc_id → (outgoing_count, incoming_count).
    /// Documents with no links will have (0, 0).
    pub fn get_link_counts_batch(
        &self,
        doc_ids: &[&str],
    ) -> Result<HashMap<String, (usize, usize)>, FactbaseError> {
        if doc_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.get_conn()?;
        let mut counts: HashMap<String, (usize, usize)> = doc_ids
            .iter()
            .map(|id| ((*id).to_string(), (0usize, 0usize)))
            .collect();
        let placeholders: String = doc_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let params: Vec<&dyn rusqlite::ToSql> =
            doc_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        // Outgoing counts
        let sql = format!(
            "SELECT source_id, COUNT(*) FROM document_links WHERE source_id IN ({placeholders}) GROUP BY source_id"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let n: i64 = row.get(1)?;
            if let Some(e) = counts.get_mut(&id) {
                e.0 = n as usize;
            }
        }
        // Incoming counts
        let sql = format!(
            "SELECT target_id, COUNT(*) FROM document_links WHERE target_id IN ({placeholders}) GROUP BY target_id"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let n: i64 = row.get(1)?;
            if let Some(e) = counts.get_mut(&id) {
                e.1 = n as usize;
            }
        }
        Ok(counts)
    }

    /// Returns true if any links exist for documents in the given repository.    ///
    /// Used to detect empty link tables (e.g., migrated/copied KBs) so that
    /// link detection can be triggered even when no documents changed.
    pub fn has_links_for_repo(&self, repo_id: &str) -> Result<bool, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM document_links dl
             JOIN documents d ON dl.source_id = d.id
             WHERE d.repo_id = ?1 LIMIT 1",
            [repo_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Converts a database row to a Link struct.
    fn row_to_link(row: &rusqlite::Row) -> Result<Link, FactbaseError> {
        let created_str: String = row.get(3)?;
        Ok(Link {
            source_id: row.get(0)?,
            target_id: row.get(1)?,
            context: row.get(2).ok(),
            created_at: super::parse_rfc3339_utc(&created_str),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo};
    use crate::models::Repository;

    #[test]
    fn test_get_all_document_titles() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "First Doc"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Second Doc"))
            .expect("upsert doc2 should succeed");

        // Test with repo filter
        let titles = db
            .get_all_document_titles(Some("test-repo"))
            .expect("get_all_document_titles should succeed");
        assert_eq!(titles.len(), 2);
        assert!(titles.iter().any(|(id, _)| id == "doc1"));
        assert!(titles.iter().any(|(_, title)| title == "Second Doc"));

        // Test without filter (all repos)
        let all_titles = db
            .get_all_document_titles(None)
            .expect("get_all_document_titles without filter should succeed");
        assert_eq!(all_titles.len(), 2);
    }

    #[test]
    fn test_get_document_titles_by_ids() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("d1", "First")).unwrap();
        db.upsert_document(&test_doc("d2", "Second")).unwrap();
        db.upsert_document(&test_doc("d3", "Third")).unwrap();

        let map = db.get_document_titles_by_ids(&["d1", "d3"]).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["d1"], "First");
        assert_eq!(map["d3"], "Third");

        // Empty input
        let empty = db.get_document_titles_by_ids(&[]).unwrap();
        assert!(empty.is_empty());

        // Non-existent ID
        let missing = db.get_document_titles_by_ids(&["nope"]).unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_update_and_get_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");

        let links = vec![DetectedLink {
            target_id: "doc2".to_string(),
            target_title: "Doc 2".to_string(),
            mention_text: "Doc 2".to_string(),
            context: "references Doc 2".to_string(),
        }];

        db.update_links("doc1", &links)
            .expect("update_links should succeed");

        let from_links = db
            .get_links_from("doc1")
            .expect("get_links_from should succeed");
        assert_eq!(from_links.len(), 1);
        assert_eq!(from_links[0].target_id, "doc2");

        let to_links = db
            .get_links_to("doc2")
            .expect("get_links_to should succeed");
        assert_eq!(to_links.len(), 1);
        assert_eq!(to_links[0].source_id, "doc1");
    }

    #[test]
    fn test_update_links_replaces_existing() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");
        db.upsert_document(&test_doc("doc3", "Doc 3"))
            .expect("upsert doc3 should succeed");

        // First update
        let links1 = vec![DetectedLink {
            target_id: "doc2".to_string(),
            target_title: "Doc 2".to_string(),
            mention_text: "Doc 2".to_string(),
            context: "".to_string(),
        }];
        db.update_links("doc1", &links1)
            .expect("update_links first should succeed");

        // Second update replaces
        let links2 = vec![DetectedLink {
            target_id: "doc3".to_string(),
            target_title: "Doc 3".to_string(),
            mention_text: "Doc 3".to_string(),
            context: "".to_string(),
        }];
        db.update_links("doc1", &links2)
            .expect("update_links second should succeed");

        let from_links = db
            .get_links_from("doc1")
            .expect("get_links_from should succeed");
        assert_eq!(from_links.len(), 1);
        assert_eq!(from_links[0].target_id, "doc3");
    }

    #[test]
    fn test_get_all_document_titles_with_repo_filter() {
        let (db, _tmp) = test_db();
        let repo1 = test_repo();
        db.add_repository(&repo1)
            .expect("add_repository repo1 should succeed");

        let repo2 = Repository {
            id: "repo2".to_string(),
            name: "Repo 2".to_string(),
            path: std::path::PathBuf::from("/tmp/repo2"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.add_repository(&repo2)
            .expect("add_repository repo2 should succeed");

        db.upsert_document(&test_doc("doc1", "Title One"))
            .expect("upsert doc1 should succeed");
        let mut doc2 = test_doc("doc2", "Title Two");
        doc2.repo_id = "repo2".to_string();
        db.upsert_document(&doc2)
            .expect("upsert doc2 should succeed");

        // No filter
        let all = db
            .get_all_document_titles(None)
            .expect("get_all_document_titles all should succeed");
        assert_eq!(all.len(), 2);

        // Filter by repo
        let filtered = db
            .get_all_document_titles(Some("test-repo"))
            .expect("get_all_document_titles filtered should succeed");
        assert_eq!(filtered.len(), 1);
        assert!(filtered.iter().any(|(_, t)| t == "Title One"));
    }

    #[test]
    fn test_get_links_for_documents_empty_input() {
        let (db, _tmp) = test_db();
        let result = db
            .get_links_for_documents(&[])
            .expect("get_links_for_documents empty should succeed");
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_links_for_documents_single_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");

        let links = vec![DetectedLink {
            target_id: "doc2".to_string(),
            target_title: "Doc 2".to_string(),
            mention_text: "Doc 2".to_string(),
            context: "references Doc 2".to_string(),
        }];
        db.update_links("doc1", &links)
            .expect("update_links should succeed");

        let result = db
            .get_links_for_documents(&["doc1"])
            .expect("get_links_for_documents should succeed");

        assert_eq!(result.len(), 1);
        let (outgoing, incoming) = result.get("doc1").expect("doc1 should be in result");
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target_id, "doc2");
        assert!(incoming.is_empty());
    }

    #[test]
    fn test_get_links_for_documents_multiple_docs() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");
        db.upsert_document(&test_doc("doc3", "Doc 3"))
            .expect("upsert doc3 should succeed");

        // doc1 -> doc2
        db.update_links(
            "doc1",
            &[DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "".to_string(),
            }],
        )
        .expect("update_links doc1 should succeed");

        // doc2 -> doc3
        db.update_links(
            "doc2",
            &[DetectedLink {
                target_id: "doc3".to_string(),
                target_title: "Doc 3".to_string(),
                mention_text: "Doc 3".to_string(),
                context: "".to_string(),
            }],
        )
        .expect("update_links doc2 should succeed");

        let result = db
            .get_links_for_documents(&["doc1", "doc2", "doc3"])
            .expect("get_links_for_documents should succeed");

        assert_eq!(result.len(), 3);

        // doc1: outgoing to doc2, no incoming
        let (out1, in1) = result.get("doc1").expect("doc1 should be in result");
        assert_eq!(out1.len(), 1);
        assert_eq!(out1[0].target_id, "doc2");
        assert!(in1.is_empty());

        // doc2: outgoing to doc3, incoming from doc1
        let (out2, in2) = result.get("doc2").expect("doc2 should be in result");
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].target_id, "doc3");
        assert_eq!(in2.len(), 1);
        assert_eq!(in2[0].source_id, "doc1");

        // doc3: no outgoing, incoming from doc2
        let (out3, in3) = result.get("doc3").expect("doc3 should be in result");
        assert!(out3.is_empty());
        assert_eq!(in3.len(), 1);
        assert_eq!(in3[0].source_id, "doc2");
    }

    #[test]
    fn test_get_links_for_documents_no_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");

        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");

        let result = db
            .get_links_for_documents(&["doc1"])
            .expect("get_links_for_documents should succeed");

        assert_eq!(result.len(), 1);
        let (outgoing, incoming) = result.get("doc1").expect("doc1 should be in result");
        assert!(outgoing.is_empty());
        assert!(incoming.is_empty());
    }

    #[test]
    fn test_get_link_counts_batch_empty() {
        let (db, _tmp) = test_db();
        let result = db.get_link_counts_batch(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_link_counts_batch_no_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("d1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("d2", "Doc 2")).unwrap();

        let counts = db.get_link_counts_batch(&["d1", "d2"]).unwrap();
        assert_eq!(counts["d1"], (0, 0));
        assert_eq!(counts["d2"], (0, 0));
    }

    #[test]
    fn test_get_link_counts_batch_with_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("d1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("d2", "Doc 2")).unwrap();
        db.upsert_document(&test_doc("d3", "Doc 3")).unwrap();

        // d1 -> d2, d1 -> d3
        db.update_links(
            "d1",
            &[
                DetectedLink { target_id: "d2".into(), target_title: "Doc 2".into(), mention_text: "Doc 2".into(), context: "".into() },
                DetectedLink { target_id: "d3".into(), target_title: "Doc 3".into(), mention_text: "Doc 3".into(), context: "".into() },
            ],
        )
        .unwrap();
        // d2 -> d3
        db.update_links(
            "d2",
            &[DetectedLink { target_id: "d3".into(), target_title: "Doc 3".into(), mention_text: "Doc 3".into(), context: "".into() }],
        )
        .unwrap();

        let counts = db.get_link_counts_batch(&["d1", "d2", "d3"]).unwrap();
        // d1: 2 outgoing, 0 incoming
        assert_eq!(counts["d1"], (2, 0));
        // d2: 1 outgoing, 1 incoming (from d1)
        assert_eq!(counts["d2"], (1, 1));
        // d3: 0 outgoing, 2 incoming (from d1 and d2)
        assert_eq!(counts["d3"], (0, 2));
    }

    #[test]
    fn test_has_links_for_repo_empty() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        assert!(!db.has_links_for_repo("test-repo").unwrap());
    }

    #[test]
    fn test_has_links_for_repo_with_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        db.update_links(
            "doc1",
            &[DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "".to_string(),
            }],
        )
        .unwrap();

        assert!(db.has_links_for_repo("test-repo").unwrap());
    }

    #[test]
    fn test_has_links_for_repo_other_repo() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        db.update_links(
            "doc1",
            &[DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "".to_string(),
            }],
        )
        .unwrap();

        // Different repo should have no links
        let repo2 = Repository {
            id: "other-repo".to_string(),
            name: "Other".to_string(),
            path: std::path::PathBuf::from("/tmp/other"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.add_repository(&repo2).unwrap();
        assert!(!db.has_links_for_repo("other-repo").unwrap());
    }
}
