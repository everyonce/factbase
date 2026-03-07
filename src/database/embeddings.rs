//! Embedding storage and retrieval.
//!
//! This module handles:
//! - Embedding insertion ([`Database::upsert_embedding`], [`Database::upsert_embedding_chunk`])
//! - Embedding deletion ([`Database::delete_embedding`])
//! - Chunk metadata ([`Database::get_chunk_metadata`])
//! - Embedding status checks ([`Database::check_embedding_status`], [`Database::get_embedding_dimension`])
//!
//! # Chunked Embeddings
//!
//! Long documents are split into chunks, each with its own embedding.
//! Chunk IDs follow the format `{doc_id}_{chunk_index}`.
//! Metadata is stored in `embedding_chunks` table.
//!
//! # Vector Storage
//!
//! Embeddings are stored in a sqlite-vec virtual table with 1024 dimensions.

use crate::error::FactbaseError;
use std::collections::HashSet;
use zerocopy::IntoBytes;

use super::{Database, EmbeddingStatus};

impl Database {
    /// Upsert embedding for a single-chunk document (backward compatible)
    pub fn upsert_embedding(&self, doc_id: &str, embedding: &[f32]) -> Result<(), FactbaseError> {
        self.upsert_embedding_chunk(doc_id, 0, 0, 0, embedding)
    }

    /// Upsert embedding for a specific chunk of a document
    pub fn upsert_embedding_chunk(
        &self,
        doc_id: &str,
        chunk_index: usize,
        chunk_start: usize,
        chunk_end: usize,
        embedding: &[f32],
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let chunk_id = format!("{doc_id}_{chunk_index}");

        // Delete existing chunk if any
        conn.execute("DELETE FROM document_embeddings WHERE id = ?1", [&chunk_id])?;
        conn.execute("DELETE FROM embedding_chunks WHERE id = ?1", [&chunk_id])?;

        // Insert embedding
        conn.execute(
            "INSERT INTO document_embeddings (id, embedding) VALUES (?1, ?2)",
            rusqlite::params![chunk_id, embedding.as_bytes()],
        )?;

        // Insert chunk metadata
        conn.execute(
            "INSERT INTO embedding_chunks (id, document_id, chunk_index, chunk_start, chunk_end)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                chunk_id,
                doc_id,
                chunk_index as i64,
                chunk_start as i64,
                chunk_end as i64
            ],
        )?;

        Ok(())
    }

    /// Delete all embeddings for a document (all chunks)
    pub fn delete_embedding(&self, doc_id: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        // Delete from chunks table first (has foreign key)
        conn.execute(
            "DELETE FROM embedding_chunks WHERE document_id = ?1",
            [doc_id],
        )?;
        // Delete from embeddings using LIKE pattern for all chunks
        conn.execute(
            "DELETE FROM document_embeddings WHERE id LIKE ?1",
            [format!("{doc_id}_%")],
        )?;
        // Also delete single-chunk format (doc_id_0)
        conn.execute(
            "DELETE FROM document_embeddings WHERE id = ?1",
            [format!("{doc_id}_0")],
        )?;
        Ok(())
    }

    /// Get chunk metadata for an embedding ID.
    /// Uses prepared statement caching for performance on repeated calls.
    pub fn get_chunk_metadata(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(String, usize, usize, usize)>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT document_id, chunk_index, chunk_start, chunk_end FROM embedding_chunks WHERE id = ?1",
        )?;
        let result = stmt.query_row([chunk_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get::<_, i64>(2)? as usize,
                row.get::<_, i64>(3)? as usize,
            ))
        });
        match result {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check embedding status for all documents in a repository.
    /// Uses prepared statement caching for performance on repeated calls.
    pub fn check_embedding_status(&self, repo_id: &str) -> Result<EmbeddingStatus, FactbaseError> {
        let conn = self.get_conn()?;

        // Get all non-deleted document IDs for this repo
        let mut doc_stmt = conn.prepare_cached(
            "SELECT id, title FROM documents WHERE repo_id = ?1 AND is_deleted = FALSE",
        )?;
        let doc_ids: Vec<(String, String)> = doc_stmt
            .query_map([repo_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        // Get all document IDs that have embeddings
        let mut emb_stmt = conn.prepare_cached(
            "SELECT DISTINCT document_id FROM embedding_chunks WHERE document_id IN (SELECT id FROM documents WHERE repo_id = ?1)",
        )?;
        let docs_with_emb: HashSet<String> = emb_stmt
            .query_map([repo_id], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;

        // Pre-allocate based on doc_ids length (worst case: all in one category)
        let mut with_embeddings = Vec::with_capacity(doc_ids.len());
        let mut without_embeddings = Vec::with_capacity(doc_ids.len().min(16));
        for (id, title) in doc_ids {
            if docs_with_emb.contains(&id) {
                with_embeddings.push(id);
            } else {
                without_embeddings.push((id, title));
            }
        }

        // Find orphaned embeddings (embeddings for deleted/non-existent docs)
        let mut orphan_stmt = conn.prepare_cached(
            "SELECT DISTINCT c.document_id FROM embedding_chunks c
             LEFT JOIN documents d ON c.document_id = d.id
             WHERE d.id IS NULL OR d.is_deleted = TRUE",
        )?;
        let orphaned: Vec<String> = orphan_stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(EmbeddingStatus {
            with_embeddings,
            without_embeddings,
            orphaned,
        })
    }

    /// Export all embeddings with chunk metadata, optionally filtered by repo.
    pub fn export_all_embeddings(
        &self,
        repo_id: Option<&str>,
    ) -> Result<Vec<crate::embeddings_io::EmbeddingRecord>, FactbaseError> {
        let conn = self.get_conn()?;
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(rid) =
            repo_id
        {
            (
                "SELECT c.document_id, c.chunk_index, c.chunk_start, c.chunk_end, e.embedding
                 FROM embedding_chunks c
                 JOIN document_embeddings e ON c.id = e.id
                 JOIN documents d ON c.document_id = d.id
                 WHERE d.repo_id = ?1 AND d.is_deleted = FALSE
                 ORDER BY c.document_id, c.chunk_index"
                    .to_string(),
                vec![Box::new(rid.to_string())],
            )
        } else {
            (
                "SELECT c.document_id, c.chunk_index, c.chunk_start, c.chunk_end, e.embedding
                 FROM embedding_chunks c
                 JOIN document_embeddings e ON c.id = e.id
                 ORDER BY c.document_id, c.chunk_index"
                    .to_string(),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(&params))?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            let doc_id: String = row.get(0)?;
            let chunk_index: i64 = row.get(1)?;
            let chunk_start: i64 = row.get(2)?;
            let chunk_end: i64 = row.get(3)?;
            let bytes: Vec<u8> = row.get(4)?;
            let embedding: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();

            records.push(crate::embeddings_io::EmbeddingRecord {
                record_type: "chunk".to_string(),
                doc_id,
                chunk_index: chunk_index as usize,
                chunk_start: chunk_start as usize,
                chunk_end: chunk_end as usize,
                embedding,
            });
        }
        Ok(records)
    }

    /// Count total embedding chunks in the database.
    pub fn count_embedding_chunks(&self) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM embedding_chunks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get all non-deleted document IDs.
    pub fn get_all_document_ids(&self) -> Result<std::collections::HashSet<String>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt =
            conn.prepare("SELECT id FROM documents WHERE is_deleted = FALSE")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<std::collections::HashSet<String>, _>>()?;
        Ok(ids)
    }

    /// Get embedding dimension from a sample embedding
    pub fn get_embedding_dimension(&self) -> Result<Option<usize>, FactbaseError> {
        let conn = self.get_conn()?;
        let result: Result<Vec<u8>, _> = conn.query_row(
            "SELECT embedding FROM document_embeddings LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(bytes) => {
                // sqlite-vec stores as f32 array, so dimension = bytes.len() / 4
                Ok(Some(bytes.len() / 4))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get embedding metadata value by key.
    pub fn get_embedding_meta(&self, key: &str) -> Result<Option<String>, FactbaseError> {
        let conn = self.get_conn()?;
        // Table may not exist yet (pre-migration-14 databases)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='embedding_metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !exists {
            return Ok(None);
        }
        match conn.query_row(
            "SELECT value FROM embedding_metadata WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set embedding metadata value.
    pub fn set_embedding_meta(&self, key: &str, value: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO embedding_metadata (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }

    /// Get the stored embedding dimension from metadata, falling back to probing actual embeddings.
    pub fn get_stored_embedding_dim(&self) -> Result<Option<usize>, FactbaseError> {
        if let Some(dim_str) = self.get_embedding_meta("embedding_dim")? {
            if let Ok(dim) = dim_str.parse::<usize>() {
                return Ok(Some(dim));
            }
        }
        // Fallback: probe actual embeddings
        self.get_embedding_dimension()
    }

    /// Get the stored embedding model name from metadata.
    pub fn get_stored_embedding_model(&self) -> Result<Option<String>, FactbaseError> {
        self.get_embedding_meta("embedding_model")
    }

    /// Record the embedding model and dimension in metadata.
    pub fn set_embedding_info(&self, model: &str, dim: usize) -> Result<(), FactbaseError> {
        self.set_embedding_meta("embedding_model", model)?;
        self.set_embedding_meta("embedding_dim", &dim.to_string())?;
        Ok(())
    }

    /// Recreate embedding virtual tables with a new dimension.
    /// Drops all existing embeddings — caller must re-index after this.
    pub fn rebuild_embedding_tables(&self, dimension: usize) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        tracing::warn!(
            "Rebuilding embedding tables for {dimension}-dim vectors. All embeddings will be regenerated."
        );
        conn.execute("DROP TABLE IF EXISTS document_embeddings", [])?;
        conn.execute("DROP TABLE IF EXISTS embedding_chunks", [])?;
        conn.execute("DROP TABLE IF EXISTS fact_embeddings", [])?;
        conn.execute("DROP TABLE IF EXISTS fact_metadata", [])?;
        // Clear cached fact pairs since they reference old embeddings
        conn.execute("DELETE FROM cached_fact_pairs", []).ok();
        conn.execute("DELETE FROM cached_fact_pairs_meta", []).ok();
        conn.execute("DELETE FROM cross_validation_state", []).ok();

        conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS document_embeddings USING vec0(
                    id TEXT PRIMARY KEY,
                    embedding FLOAT[{dimension}]
                )"
            ),
            [],
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embedding_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                chunk_start INTEGER NOT NULL,
                chunk_end INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_doc ON embedding_chunks(document_id);",
        )?;
        conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS fact_embeddings USING vec0(
                    id TEXT PRIMARY KEY,
                    embedding FLOAT[{dimension}]
                )"
            ),
            [],
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fact_metadata (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                line_number INTEGER NOT NULL,
                fact_text TEXT NOT NULL,
                fact_hash TEXT NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_fact_meta_doc ON fact_metadata(document_id);",
        )?;
        // Clear query cache since dimensions changed
        conn.execute("DELETE FROM query_embedding_cache", []).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_upsert_embedding() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding("doc1", &embedding)
            .expect("upsert_embedding should succeed");

        // Upsert again should not error
        let embedding2: Vec<f32> = vec![0.2; 1024];
        db.upsert_embedding("doc1", &embedding2)
            .expect("upsert_embedding update should succeed");
    }

    #[test]
    fn test_delete_embedding() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding("doc1", &embedding)
            .expect("upsert_embedding should succeed");
        db.delete_embedding("doc1")
            .expect("delete_embedding should succeed");

        // Delete non-existent should not error
        db.delete_embedding("nonexistent")
            .expect("delete_embedding nonexistent should succeed");
    }

    #[test]
    fn test_upsert_embedding_chunk() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding_chunk("doc1", 0, 0, 1000, &embedding)
            .expect("upsert_embedding_chunk should succeed");
        db.upsert_embedding_chunk("doc1", 1, 1000, 2000, &embedding)
            .expect("upsert_embedding_chunk should succeed");

        // Verify chunk metadata
        let meta = db
            .get_chunk_metadata("doc1_0")
            .expect("get_chunk_metadata should succeed");
        assert!(meta.is_some());
        let (doc_id, chunk_idx, start, end) = meta.unwrap();
        assert_eq!(doc_id, "doc1");
        assert_eq!(chunk_idx, 0);
        assert_eq!(start, 0);
        assert_eq!(end, 1000);
    }

    #[test]
    fn test_get_chunk_metadata_not_found() {
        let (db, _tmp) = test_db();
        let meta = db
            .get_chunk_metadata("nonexistent_0")
            .expect("get_chunk_metadata should succeed");
        assert!(meta.is_none());
    }

    #[test]
    fn test_check_embedding_status() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert_document should succeed");

        // Only doc1 has embedding
        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding("doc1", &embedding)
            .expect("upsert_embedding should succeed");

        let status = db
            .check_embedding_status("test-repo")
            .expect("check_embedding_status should succeed");
        assert_eq!(status.with_embeddings.len(), 1);
        assert!(status.with_embeddings.contains(&"doc1".to_string()));
        assert_eq!(status.without_embeddings.len(), 1);
        assert_eq!(status.without_embeddings[0].0, "doc2");
    }

    #[test]
    fn test_get_embedding_dimension() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert_document should succeed");

        // No embeddings yet
        let dim = db
            .get_embedding_dimension()
            .expect("get_embedding_dimension should succeed");
        assert!(dim.is_none());

        // Add embedding
        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding("doc1", &embedding)
            .expect("upsert_embedding should succeed");

        let dim = db
            .get_embedding_dimension()
            .expect("get_embedding_dimension should succeed");
        assert_eq!(dim, Some(1024));
    }

    #[test]
    fn test_embedding_metadata() {
        let (db, _tmp) = test_db();

        // No metadata initially
        assert_eq!(db.get_embedding_meta("embedding_model").unwrap(), None);
        assert_eq!(db.get_embedding_meta("embedding_dim").unwrap(), None);
        assert_eq!(db.get_stored_embedding_dim().unwrap(), None);
        assert_eq!(db.get_stored_embedding_model().unwrap(), None);

        // Set metadata
        db.set_embedding_info("BAAI/bge-small-en-v1.5", 384).unwrap();
        assert_eq!(db.get_stored_embedding_dim().unwrap(), Some(384));
        assert_eq!(
            db.get_stored_embedding_model().unwrap(),
            Some("BAAI/bge-small-en-v1.5".to_string())
        );

        // Update metadata
        db.set_embedding_info("amazon.titan-embed-text-v2:0", 1024).unwrap();
        assert_eq!(db.get_stored_embedding_dim().unwrap(), Some(1024));
        assert_eq!(
            db.get_stored_embedding_model().unwrap(),
            Some("amazon.titan-embed-text-v2:0".to_string())
        );
    }

    #[test]
    fn test_rebuild_embedding_tables() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        // Add 1024-dim embedding
        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_embedding("doc1", &embedding).unwrap();
        assert_eq!(db.get_embedding_dimension().unwrap(), Some(1024));

        // Rebuild for 384-dim
        db.rebuild_embedding_tables(384).unwrap();

        // Old embeddings should be gone
        assert_eq!(db.get_embedding_dimension().unwrap(), None);

        // Can now insert 384-dim embeddings
        let small_embedding: Vec<f32> = vec![0.2; 384];
        db.upsert_embedding("doc1", &small_embedding).unwrap();
        assert_eq!(db.get_embedding_dimension().unwrap(), Some(384));
    }
}
