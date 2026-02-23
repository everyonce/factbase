use crate::error::FactbaseError;
use crate::llm::DetectedLink;
use crate::models::{Document, Link, RepoStats, Repository, SearchResult};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self, FactbaseError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Load sqlite-vec as auto extension before opening connection
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT UNIQUE NOT NULL,
                perspective TEXT,
                created_at TIMESTAMP NOT NULL,
                last_indexed_at TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                title TEXT NOT NULL,
                doc_type TEXT,
                content TEXT NOT NULL,
                file_modified_at TIMESTAMP,
                indexed_at TIMESTAMP NOT NULL,
                is_deleted BOOLEAN DEFAULT FALSE,
                UNIQUE(repo_id, file_path),
                FOREIGN KEY (repo_id) REFERENCES repositories(id)
            );
            CREATE TABLE IF NOT EXISTS document_links (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                context TEXT,
                created_at TIMESTAMP NOT NULL,
                PRIMARY KEY (source_id, target_id),
                FOREIGN KEY (source_id) REFERENCES documents(id),
                FOREIGN KEY (target_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_documents_repo ON documents(repo_id);
            CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(doc_type);
            CREATE INDEX IF NOT EXISTS idx_documents_title ON documents(title);
            CREATE INDEX IF NOT EXISTS idx_documents_deleted ON documents(is_deleted);
            CREATE INDEX IF NOT EXISTS idx_links_source ON document_links(source_id);
            CREATE INDEX IF NOT EXISTS idx_links_target ON document_links(target_id);",
        )?;

        // Create virtual table for embeddings
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS document_embeddings USING vec0(
                document_id TEXT PRIMARY KEY,
                embedding FLOAT[768]
            )",
            [],
        )?;

        Ok(())
    }

    pub fn upsert_repository(&self, repo: &Repository) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let perspective = repo
            .perspective
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap());
        conn.execute(
            "INSERT OR REPLACE INTO repositories (id, name, path, perspective, created_at, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                repo.id, repo.name, repo.path.to_string_lossy(), perspective,
                repo.created_at.to_rfc3339(), repo.last_indexed_at.map(|t| t.to_rfc3339())
            ],
        )?;
        Ok(())
    }

    pub fn get_repository(&self, id: &str) -> Result<Option<Repository>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, path, perspective, created_at, last_indexed_at FROM repositories WHERE id = ?1")?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_repository(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_repositories(&self) -> Result<Vec<Repository>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, path, perspective, created_at, last_indexed_at FROM repositories ORDER BY name")?;
        let rows = stmt.query_map([], |row| Ok(Self::row_to_repository(row).unwrap()))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn row_to_repository(row: &rusqlite::Row) -> Result<Repository, FactbaseError> {
        let perspective_str: Option<String> = row.get(3)?;
        let perspective = perspective_str.and_then(|s| serde_json::from_str(&s).ok());
        let created_str: String = row.get(4)?;
        let last_indexed_str: Option<String> = row.get(5)?;
        Ok(Repository {
            id: row.get(0)?,
            name: row.get(1)?,
            path: std::path::PathBuf::from(row.get::<_, String>(2)?),
            perspective,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .unwrap()
                .with_timezone(&chrono::Utc),
            last_indexed_at: last_indexed_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc))
            }),
        })
    }

    pub fn upsert_document(&self, doc: &Document) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO documents (id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, FALSE)",
            rusqlite::params![
                doc.id, doc.repo_id, doc.file_path, doc.file_hash, doc.title, doc.doc_type, doc.content,
                doc.file_modified_at.map(|t| t.to_rfc3339()), doc.indexed_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Result<Option<Document>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted FROM documents WHERE id = ?1 AND is_deleted = FALSE")?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_document_by_path(
        &self,
        repo_id: &str,
        path: &str,
    ) -> Result<Option<Document>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted FROM documents WHERE repo_id = ?1 AND file_path = ?2")?;
        let mut rows = stmt.query([repo_id, path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_documents_for_repo(
        &self,
        repo_id: &str,
    ) -> Result<HashMap<String, Document>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted FROM documents WHERE repo_id = ?1")?;
        let rows = stmt.query_map([repo_id], |row| Ok(Self::row_to_document(row).unwrap()))?;
        Ok(rows
            .filter_map(|r| r.ok())
            .map(|d| (d.id.clone(), d))
            .collect())
    }

    fn row_to_document(row: &rusqlite::Row) -> Result<Document, FactbaseError> {
        let file_modified_str: Option<String> = row.get(7)?;
        let indexed_str: String = row.get(8)?;
        Ok(Document {
            id: row.get(0)?,
            repo_id: row.get(1)?,
            file_path: row.get(2)?,
            file_hash: row.get(3)?,
            title: row.get(4)?,
            doc_type: row.get(5)?,
            content: row.get(6)?,
            file_modified_at: file_modified_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc))
            }),
            indexed_at: chrono::DateTime::parse_from_rfc3339(&indexed_str)
                .unwrap()
                .with_timezone(&chrono::Utc),
            is_deleted: row.get(9)?,
        })
    }

    pub fn mark_deleted(&self, id: &str) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE documents SET is_deleted = TRUE WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get_stats(&self, repo_id: &str) -> Result<RepoStats, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE repo_id = ?1",
            [repo_id],
            |r| r.get(0),
        )?;
        let active: usize = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
            [repo_id],
            |r| r.get(0),
        )?;
        let deleted = total - active;
        let mut stmt = conn.prepare("SELECT doc_type, COUNT(*) FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE GROUP BY doc_type")?;
        let by_type: HashMap<String, usize> = stmt
            .query_map([repo_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "unknown".into()),
                    row.get(1)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(RepoStats {
            total,
            active,
            deleted,
            by_type,
        })
    }

    // Embedding operations
    pub fn upsert_embedding(&self, doc_id: &str, embedding: &[f32]) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        // vec0 doesn't support INSERT OR REPLACE, so delete first
        conn.execute(
            "DELETE FROM document_embeddings WHERE document_id = ?1",
            [doc_id],
        )?;
        conn.execute(
            "INSERT INTO document_embeddings (document_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![doc_id, embedding.as_bytes()],
        )?;
        Ok(())
    }

    pub fn delete_embedding(&self, doc_id: &str) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM document_embeddings WHERE document_id = ?1",
            [doc_id],
        )?;
        Ok(())
    }

    pub fn search_semantic(
        &self,
        embedding: &[f32],
        limit: usize,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, FactbaseError> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT d.id, d.title, d.doc_type, d.file_path, d.content, e.distance
             FROM document_embeddings e
             JOIN documents d ON e.document_id = d.id
             WHERE d.is_deleted = FALSE
             AND e.embedding MATCH ?1",
        );

        if doc_type.is_some() {
            sql.push_str(" AND d.doc_type = ?2");
        }
        if repo_id.is_some() {
            sql.push_str(if doc_type.is_some() {
                " AND d.repo_id = ?3"
            } else {
                " AND d.repo_id = ?2"
            });
        }

        sql.push_str(&format!(" ORDER BY e.distance LIMIT {}", limit));

        let mut stmt = conn.prepare(&sql)?;

        let mut results = Vec::new();

        let mut query_rows = |params: &[&dyn rusqlite::ToSql]| -> Result<(), FactbaseError> {
            let mut rows = stmt.query(params)?;
            while let Some(row) = rows.next()? {
                results.push(Self::row_to_search_result(row));
            }
            Ok(())
        };

        match (doc_type, repo_id) {
            (Some(t), Some(r)) => query_rows(&[&embedding.as_bytes(), &t, &r])?,
            (Some(t), None) => query_rows(&[&embedding.as_bytes(), &t])?,
            (None, Some(r)) => query_rows(&[&embedding.as_bytes(), &r])?,
            (None, None) => query_rows(&[&embedding.as_bytes()])?,
        };

        Ok(results)
    }

    fn row_to_search_result(row: &rusqlite::Row) -> SearchResult {
        let content: String = row.get(4).unwrap_or_default();
        let distance: f32 = row.get(5).unwrap_or(1.0);

        // Generate snippet from content
        let snippet = content
            .lines()
            .filter(|l| !l.starts_with("<!--"))
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(200)
            .collect::<String>();

        SearchResult {
            id: row.get(0).unwrap_or_default(),
            title: row.get(1).unwrap_or_default(),
            doc_type: row.get(2).ok(),
            file_path: row.get(3).unwrap_or_default(),
            relevance_score: 1.0 - distance,
            snippet,
        }
    }

    // Link operations
    pub fn get_all_document_titles(&self) -> Result<Vec<(String, String)>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, title FROM documents WHERE is_deleted = FALSE")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn update_links(
        &self,
        source_id: &str,
        links: &[DetectedLink],
    ) -> Result<(), FactbaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM document_links WHERE source_id = ?1",
            [source_id],
        )?;

        let now = chrono::Utc::now().to_rfc3339();
        for link in links {
            conn.execute(
                "INSERT OR IGNORE INTO document_links (source_id, target_id, context, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![source_id, link.target_id, link.context, now],
            )?;
        }
        Ok(())
    }

    pub fn get_links_from(&self, source_id: &str) -> Result<Vec<Link>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT source_id, target_id, context, created_at FROM document_links WHERE source_id = ?1")?;
        let rows = stmt.query_map([source_id], |row| Ok(Self::row_to_link(row)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_links_to(&self, target_id: &str) -> Result<Vec<Link>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT source_id, target_id, context, created_at FROM document_links WHERE target_id = ?1")?;
        let rows = stmt.query_map([target_id], |row| Ok(Self::row_to_link(row)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn row_to_link(row: &rusqlite::Row) -> Link {
        let created_str: String = row.get(3).unwrap_or_default();
        Link {
            source_id: row.get(0).unwrap_or_default(),
            target_id: row.get(1).unwrap_or_default(),
            context: row.get(2).ok(),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }

    pub fn list_documents(
        &self,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Document>, FactbaseError> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, repo_id, file_path, file_hash, title, doc_type, content, file_modified_at, indexed_at, is_deleted
             FROM documents WHERE is_deleted = FALSE",
        );

        if doc_type.is_some() {
            sql.push_str(" AND doc_type = ?1");
        }
        if repo_id.is_some() {
            sql.push_str(if doc_type.is_some() {
                " AND repo_id = ?2"
            } else {
                " AND repo_id = ?1"
            });
        }

        sql.push_str(&format!(" ORDER BY title LIMIT {}", limit));

        let mut stmt = conn.prepare(&sql)?;
        let mut results = Vec::new();

        let mut query_rows = |params: &[&dyn rusqlite::ToSql]| -> Result<(), FactbaseError> {
            let mut rows = stmt.query(params)?;
            while let Some(row) = rows.next()? {
                results.push(Self::row_to_document(row)?);
            }
            Ok(())
        };

        match (doc_type, repo_id) {
            (Some(t), Some(r)) => query_rows(&[&t, &r])?,
            (Some(t), None) => query_rows(&[&t])?,
            (None, Some(r)) => query_rows(&[&r])?,
            (None, None) => query_rows(&[])?,
        };

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = Database::new(&db_path).unwrap();
        (db, tmp)
    }

    fn test_repo() -> Repository {
        Repository {
            id: "test".to_string(),
            name: "Test Repo".to_string(),
            path: std::path::PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
        }
    }

    fn test_doc(id: &str, title: &str) -> Document {
        Document {
            id: id.to_string(),
            repo_id: "test".to_string(),
            file_path: format!("{}.md", id),
            file_hash: "abc123".to_string(),
            title: title.to_string(),
            doc_type: Some("document".to_string()),
            content: format!("# {}\n\nContent here.", title),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        }
    }

    #[test]
    fn test_database_new_creates_tables() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        // Should not error - tables exist
        db.upsert_repository(&repo).unwrap();
    }

    #[test]
    fn test_upsert_and_get_document() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc = test_doc("abc123", "Test Doc");
        db.upsert_document(&doc).unwrap();

        let retrieved = db.get_document("abc123").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Doc");
    }

    #[test]
    fn test_get_document_not_found() {
        let (db, _tmp) = test_db();
        let result = db.get_document("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_mark_deleted() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc = test_doc("abc123", "Test Doc");
        db.upsert_document(&doc).unwrap();

        db.mark_deleted("abc123").unwrap();

        // get_document should return None for deleted docs
        let result = db.get_document("abc123").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_documents_for_repo() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let docs = db.get_documents_for_repo("test").unwrap();
        assert_eq!(docs.len(), 2);
        assert!(docs.contains_key("doc1"));
        assert!(docs.contains_key("doc2"));
    }

    #[test]
    fn test_get_stats() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();
        db.mark_deleted("doc2").unwrap();

        let stats = db.get_stats("test").unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 1);
        assert_eq!(stats.deleted, 1);
    }

    #[test]
    fn test_upsert_embedding() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 768];
        db.upsert_embedding("doc1", &embedding).unwrap();

        // Upsert again should not error
        let embedding2: Vec<f32> = vec![0.2; 768];
        db.upsert_embedding("doc1", &embedding2).unwrap();
    }

    #[test]
    fn test_delete_embedding() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 768];
        db.upsert_embedding("doc1", &embedding).unwrap();
        db.delete_embedding("doc1").unwrap();

        // Delete non-existent should not error
        db.delete_embedding("nonexistent").unwrap();
    }

    #[test]
    fn test_get_all_document_titles() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.upsert_document(&test_doc("doc1", "First Doc")).unwrap();
        db.upsert_document(&test_doc("doc2", "Second Doc")).unwrap();

        let titles = db.get_all_document_titles().unwrap();
        assert_eq!(titles.len(), 2);
        assert!(titles.iter().any(|(id, _)| id == "doc1"));
        assert!(titles.iter().any(|(_, title)| title == "Second Doc"));
    }

    #[test]
    fn test_update_and_get_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let links = vec![DetectedLink {
            target_id: "doc2".to_string(),
            target_title: "Doc 2".to_string(),
            mention_text: "Doc 2".to_string(),
            context: "references Doc 2".to_string(),
        }];

        db.update_links("doc1", &links).unwrap();

        let from_links = db.get_links_from("doc1").unwrap();
        assert_eq!(from_links.len(), 1);
        assert_eq!(from_links[0].target_id, "doc2");

        let to_links = db.get_links_to("doc2").unwrap();
        assert_eq!(to_links.len(), 1);
        assert_eq!(to_links[0].source_id, "doc1");
    }

    #[test]
    fn test_update_links_replaces_existing() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();
        db.upsert_document(&test_doc("doc3", "Doc 3")).unwrap();

        // First update
        let links1 = vec![DetectedLink {
            target_id: "doc2".to_string(),
            target_title: "Doc 2".to_string(),
            mention_text: "Doc 2".to_string(),
            context: "".to_string(),
        }];
        db.update_links("doc1", &links1).unwrap();

        // Second update replaces
        let links2 = vec![DetectedLink {
            target_id: "doc3".to_string(),
            target_title: "Doc 3".to_string(),
            mention_text: "Doc 3".to_string(),
            context: "".to_string(),
        }];
        db.update_links("doc1", &links2).unwrap();

        let from_links = db.get_links_from("doc1").unwrap();
        assert_eq!(from_links.len(), 1);
        assert_eq!(from_links[0].target_id, "doc3");
    }
}
