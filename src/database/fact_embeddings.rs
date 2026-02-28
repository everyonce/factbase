//! Fact-level embedding storage and retrieval for cross-validation.
//!
//! Stores per-fact embeddings alongside document embeddings.
//! Fact IDs follow the format `{doc_id}_{line_number}`.

use crate::error::FactbaseError;
use crate::models::FactSearchResult;
use zerocopy::IntoBytes;

use super::Database;

impl Database {
    /// Upsert a fact embedding with metadata.
    pub fn upsert_fact_embedding(
        &self,
        id: &str,
        doc_id: &str,
        line_number: usize,
        fact_text: &str,
        fact_hash: &str,
        embedding: &[f32],
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;

        // Delete existing if any
        conn.execute("DELETE FROM fact_embeddings WHERE id = ?1", [id])?;
        conn.execute("DELETE FROM fact_metadata WHERE id = ?1", [id])?;

        conn.execute(
            "INSERT INTO fact_embeddings (id, embedding) VALUES (?1, ?2)",
            rusqlite::params![id, embedding.as_bytes()],
        )?;

        conn.execute(
            "INSERT INTO fact_metadata (id, document_id, line_number, fact_text, fact_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, doc_id, line_number as i64, fact_text, fact_hash],
        )?;

        Ok(())
    }

    /// Delete all fact embeddings for a document.
    pub fn delete_fact_embeddings_for_doc(&self, doc_id: &str) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        // Get IDs to delete from vec0 table
        let ids: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT id FROM fact_metadata WHERE document_id = ?1")?;
            let result = stmt
                .query_map([doc_id], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            result
        };
        for id in &ids {
            conn.execute("DELETE FROM fact_embeddings WHERE id = ?1", [id])?;
        }
        conn.execute(
            "DELETE FROM fact_metadata WHERE document_id = ?1",
            [doc_id],
        )?;
        Ok(())
    }

    /// Search fact embeddings by vector similarity, excluding facts from a specific document.
    pub fn search_fact_embeddings(
        &self,
        query_embedding: &[f32],
        limit: usize,
        exclude_doc_id: Option<&str>,
    ) -> Result<Vec<FactSearchResult>, FactbaseError> {
        let conn = self.get_conn()?;
        // Fetch more than limit to account for exclusions
        let k = if exclude_doc_id.is_some() {
            limit + 20
        } else {
            limit
        };
        let mut stmt = conn.prepare_cached(
            "SELECT m.id, m.document_id, m.line_number, m.fact_text, e.distance
             FROM fact_embeddings e
             JOIN fact_metadata m ON e.id = m.id
             WHERE e.embedding MATCH ?1 AND k = ?2
             ORDER BY e.distance",
        )?;
        let mut results = Vec::with_capacity(limit);
        let mut rows = stmt.query(rusqlite::params![query_embedding.as_bytes(), k as i32])?;
        while let Some(row) = rows.next()? {
            if results.len() >= limit {
                break;
            }
            let doc_id: String = row.get(1)?;
            if exclude_doc_id == Some(doc_id.as_str()) {
                continue;
            }
            results.push(FactSearchResult {
                id: row.get(0)?,
                document_id: doc_id,
                line_number: row.get::<_, i64>(2)? as usize,
                fact_text: row.get(3)?,
                similarity: 1.0 - row.get::<_, f32>(4)?,
            });
        }
        Ok(results)
    }

    /// Get fact hashes for a document, keyed by fact ID.
    pub fn get_fact_hashes_for_doc(
        &self,
        doc_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt =
            conn.prepare("SELECT id, fact_hash FROM fact_metadata WHERE document_id = ?1")?;
        let map = stmt
            .query_map([doc_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<std::collections::HashMap<String, String>, _>>()?;
        Ok(map)
    }

    /// Count total fact embeddings in the database.
    pub fn get_fact_embedding_count(&self) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM fact_metadata", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Count fact embeddings for a specific document.
    pub fn get_fact_embedding_count_for_doc(&self, doc_id: &str) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM fact_metadata WHERE document_id = ?1",
            [doc_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_upsert_and_count_fact_embeddings() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Some fact", "hash1", &embedding)
            .unwrap();
        db.upsert_fact_embedding("doc1_10", "doc1", 10, "Another fact", "hash2", &embedding)
            .unwrap();

        assert_eq!(db.get_fact_embedding_count().unwrap(), 2);
        assert_eq!(db.get_fact_embedding_count_for_doc("doc1").unwrap(), 2);
        assert_eq!(db.get_fact_embedding_count_for_doc("doc2").unwrap(), 0);
    }

    #[test]
    fn test_upsert_fact_embedding_overwrites() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Old fact", "hash1", &embedding)
            .unwrap();
        db.upsert_fact_embedding("doc1_5", "doc1", 5, "New fact", "hash2", &embedding)
            .unwrap();

        assert_eq!(db.get_fact_embedding_count().unwrap(), 1);
    }

    #[test]
    fn test_delete_fact_embeddings_for_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Fact A", "h1", &embedding)
            .unwrap();
        db.upsert_fact_embedding("doc2_3", "doc2", 3, "Fact B", "h2", &embedding)
            .unwrap();

        db.delete_fact_embeddings_for_doc("doc1").unwrap();

        assert_eq!(db.get_fact_embedding_count().unwrap(), 1);
        assert_eq!(db.get_fact_embedding_count_for_doc("doc1").unwrap(), 0);
        assert_eq!(db.get_fact_embedding_count_for_doc("doc2").unwrap(), 1);
    }

    #[test]
    fn test_delete_fact_embeddings_for_nonexistent_doc() {
        let (db, _tmp) = test_db();
        // Should not error
        db.delete_fact_embeddings_for_doc("nonexistent").unwrap();
    }

    #[test]
    fn test_search_fact_embeddings() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        // Insert facts with distinct embeddings
        let mut emb1 = vec![0.0f32; 1024];
        emb1[0] = 1.0;
        let mut emb2 = vec![0.0f32; 1024];
        emb2[1] = 1.0;

        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Fact from doc1", "h1", &emb1)
            .unwrap();
        db.upsert_fact_embedding("doc2_3", "doc2", 3, "Fact from doc2", "h2", &emb2)
            .unwrap();

        // Search with emb1-like query — should find doc1's fact as most similar
        let results = db.search_fact_embeddings(&emb1, 10, None).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document_id, "doc1");
        assert_eq!(results[0].line_number, 5);
        assert_eq!(results[0].fact_text, "Fact from doc1");
        assert!(results[0].similarity > results[1].similarity);

        // Search excluding doc1
        let results = db
            .search_fact_embeddings(&emb1, 10, Some("doc1"))
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, "doc2");
    }

    #[test]
    fn test_search_fact_embeddings_empty() {
        let (db, _tmp) = test_db();
        let query: Vec<f32> = vec![0.1; 1024];
        let results = db.search_fact_embeddings(&query, 10, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_fact_embedding_count_empty() {
        let (db, _tmp) = test_db();
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);
    }

    #[test]
    fn test_get_fact_hashes_for_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let embedding: Vec<f32> = vec![0.1; 1024];
        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Fact A", "hashA", &embedding)
            .unwrap();
        db.upsert_fact_embedding("doc1_10", "doc1", 10, "Fact B", "hashB", &embedding)
            .unwrap();

        let hashes = db.get_fact_hashes_for_doc("doc1").unwrap();
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes["doc1_5"], "hashA");
        assert_eq!(hashes["doc1_10"], "hashB");

        // Non-existent doc returns empty
        let empty = db.get_fact_hashes_for_doc("nonexistent").unwrap();
        assert!(empty.is_empty());
    }
}
