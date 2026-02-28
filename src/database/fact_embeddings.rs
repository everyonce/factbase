//! Fact-level embedding storage and retrieval for cross-validation.
//!
//! Stores per-fact embeddings alongside document embeddings.
//! Fact IDs follow the format `{doc_id}_{line_number}`.

use crate::error::FactbaseError;
use crate::models::{FactPair, FactSearchResult};
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

    /// Search for similar facts from other documents, excluding self-matches.
    pub fn search_similar_facts(
        &self,
        fact_id: &str,
        doc_id: &str,
        embedding: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<FactSearchResult>, FactbaseError> {
        let conn = self.get_conn()?;
        let k = limit + 50; // over-fetch to account for exclusions
        let mut stmt = conn.prepare_cached(
            "SELECT m.id, m.document_id, m.line_number, m.fact_text, e.distance
             FROM fact_embeddings e
             JOIN fact_metadata m ON e.id = m.id
             WHERE e.embedding MATCH ?1 AND k = ?2
             ORDER BY e.distance",
        )?;
        let max_distance = 1.0 - threshold;
        let mut results = Vec::with_capacity(limit);
        let mut rows = stmt.query(rusqlite::params![embedding.as_bytes(), k as i32])?;
        while let Some(row) = rows.next()? {
            if results.len() >= limit {
                break;
            }
            let distance: f32 = row.get(4)?;
            if distance > max_distance {
                break;
            }
            let id: String = row.get(0)?;
            let result_doc_id: String = row.get(1)?;
            if id == fact_id || result_doc_id == doc_id {
                continue;
            }
            results.push(FactSearchResult {
                id,
                document_id: result_doc_id,
                line_number: row.get::<_, i64>(2)? as usize,
                fact_text: row.get(3)?,
                similarity: 1.0 - distance,
            });
        }
        Ok(results)
    }

    /// Find all cross-document fact pairs above a similarity threshold.
    ///
    /// Iterates all facts, finds neighbors from other documents, and returns
    /// deduplicated pairs (if (A,B) exists, (B,A) is skipped).
    pub fn find_all_cross_doc_fact_pairs(
        &self,
        threshold: f32,
        limit_per_fact: usize,
    ) -> Result<Vec<FactPair>, FactbaseError> {
        let conn = self.get_conn()?;

        // Load all fact metadata + embeddings
        let mut stmt = conn.prepare(
            "SELECT m.id, m.document_id, m.line_number, m.fact_text, e.embedding
             FROM fact_metadata m
             JOIN fact_embeddings e ON m.id = e.id",
        )?;
        let facts: Vec<(String, String, i64, String, Vec<u8>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        drop(conn);

        let mut seen = std::collections::HashSet::new();
        let mut pairs = Vec::new();

        for (fact_id, doc_id, line_number, fact_text, emb_bytes) in &facts {
            let embedding: &[f32] = zerocopy::FromBytes::ref_from_bytes(emb_bytes)
                .map_err(|e| FactbaseError::Database(format!("bad embedding bytes: {e}")))?;

            let neighbors =
                self.search_similar_facts(fact_id, doc_id, embedding, limit_per_fact, threshold)?;

            for neighbor in neighbors {
                let key = if fact_id.as_str() < neighbor.id.as_str() {
                    (fact_id.clone(), neighbor.id.clone())
                } else {
                    (neighbor.id.clone(), fact_id.clone())
                };
                if !seen.insert(key) {
                    continue;
                }
                let sim = neighbor.similarity;
                pairs.push(FactPair {
                    fact_a: FactSearchResult {
                        id: fact_id.clone(),
                        document_id: doc_id.clone(),
                        line_number: *line_number as usize,
                        fact_text: fact_text.clone(),
                        similarity: sim,
                    },
                    fact_b: neighbor,
                    similarity: sim,
                });
            }
        }

        // Sort by descending similarity
        pairs.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        Ok(pairs)
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

    #[test]
    fn test_search_similar_facts_cross_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();
        db.upsert_document(&test_doc("doc3", "Doc 3")).unwrap();

        // Similar embeddings for doc1 and doc2, different for doc3
        let mut emb_a = vec![0.0f32; 1024];
        emb_a[0] = 1.0;
        emb_a[1] = 0.1;
        let mut emb_b = vec![0.0f32; 1024];
        emb_b[0] = 1.0;
        emb_b[1] = 0.2; // very similar to emb_a
        let mut emb_c = vec![0.0f32; 1024];
        emb_c[500] = 1.0; // very different

        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Fact A", "h1", &emb_a).unwrap();
        db.upsert_fact_embedding("doc2_3", "doc2", 3, "Fact B", "h2", &emb_b).unwrap();
        db.upsert_fact_embedding("doc3_1", "doc3", 1, "Fact C", "h3", &emb_c).unwrap();

        // Search from doc1's fact — should find doc2 (similar) but not doc3 (dissimilar) at high threshold
        let results = db.search_similar_facts("doc1_5", "doc1", &emb_a, 10, 0.9).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, "doc2");
        assert!(results[0].similarity >= 0.9);

        // Excludes self-match (same fact_id)
        let results = db.search_similar_facts("doc1_5", "doc1", &emb_a, 10, 0.0).unwrap();
        assert!(results.iter().all(|r| r.id != "doc1_5"));
        // Excludes same-doc matches
        assert!(results.iter().all(|r| r.document_id != "doc1"));
    }

    #[test]
    fn test_search_similar_facts_respects_threshold() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let mut emb_a = vec![0.0f32; 1024];
        emb_a[0] = 1.0;
        let mut emb_b = vec![0.0f32; 1024];
        emb_b[500] = 1.0; // orthogonal — similarity ~0

        db.upsert_fact_embedding("doc1_1", "doc1", 1, "Fact A", "h1", &emb_a).unwrap();
        db.upsert_fact_embedding("doc2_1", "doc2", 1, "Fact B", "h2", &emb_b).unwrap();

        // High threshold should filter out the dissimilar fact
        let results = db.search_similar_facts("doc1_1", "doc1", &emb_a, 10, 0.5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_all_cross_doc_fact_pairs_deduplicates() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        // Two very similar facts in different docs
        let mut emb_a = vec![0.0f32; 1024];
        emb_a[0] = 1.0;
        emb_a[1] = 0.1;
        let mut emb_b = vec![0.0f32; 1024];
        emb_b[0] = 1.0;
        emb_b[1] = 0.2;

        db.upsert_fact_embedding("doc1_5", "doc1", 5, "Fact A", "h1", &emb_a).unwrap();
        db.upsert_fact_embedding("doc2_3", "doc2", 3, "Fact B", "h2", &emb_b).unwrap();

        let pairs = db.find_all_cross_doc_fact_pairs(0.5, 10).unwrap();
        // Should have exactly 1 pair (A,B) — not (A,B) and (B,A)
        assert_eq!(pairs.len(), 1);
        let pair = &pairs[0];
        assert!(pair.similarity >= 0.5);
        // One fact from each doc
        assert_ne!(pair.fact_a.document_id, pair.fact_b.document_id);
    }

    #[test]
    fn test_find_all_cross_doc_fact_pairs_excludes_same_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        // Two identical facts in the SAME doc
        let emb = vec![0.5f32; 1024];
        db.upsert_fact_embedding("doc1_1", "doc1", 1, "Fact X", "h1", &emb).unwrap();
        db.upsert_fact_embedding("doc1_2", "doc1", 2, "Fact Y", "h2", &emb).unwrap();

        let pairs = db.find_all_cross_doc_fact_pairs(0.0, 10).unwrap();
        // No pairs — both facts are in the same document
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_find_all_cross_doc_fact_pairs_empty_db() {
        let (db, _tmp) = test_db();
        let pairs = db.find_all_cross_doc_fact_pairs(0.5, 10).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_find_all_cross_doc_fact_pairs_sorted_by_similarity() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();
        db.upsert_document(&test_doc("doc3", "Doc 3")).unwrap();

        // doc1 and doc2 facts nearly identical; doc3 fact slightly different
        let mut emb1 = vec![0.0f32; 1024];
        emb1[0] = 1.0;
        let mut emb2 = vec![0.0f32; 1024];
        emb2[0] = 1.0;
        emb2[1] = 0.01; // nearly identical to emb1
        let mut emb3 = vec![0.0f32; 1024];
        emb3[0] = 1.0;
        emb3[1] = 0.3; // still close but less similar

        db.upsert_fact_embedding("doc1_1", "doc1", 1, "Fact 1", "h1", &emb1).unwrap();
        db.upsert_fact_embedding("doc2_1", "doc2", 1, "Fact 2", "h2", &emb2).unwrap();
        db.upsert_fact_embedding("doc3_1", "doc3", 1, "Fact 3", "h3", &emb3).unwrap();

        // Use threshold 0.0 so all pairs are included
        let pairs = db.find_all_cross_doc_fact_pairs(0.0, 10).unwrap();
        assert!(pairs.len() >= 2, "expected at least 2 pairs, got {}", pairs.len());
        // Should be sorted descending by similarity
        for w in pairs.windows(2) {
            assert!(w[0].similarity >= w[1].similarity);
        }
    }
}
