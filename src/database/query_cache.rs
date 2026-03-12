//! Persistent query embedding cache in SQLite.
//!
//! Stores query embeddings keyed by (text_hash, model) to avoid redundant
//! inference calls across process restarts. The in-memory LRU cache sits
//! in front of this for hot-path performance.

use crate::error::FactbaseError;
use zerocopy::IntoBytes;

use super::Database;

impl Database {
    /// Look up a cached query embedding by text hash and model.
    /// Updates `last_used_at` on hit.
    pub fn get_cached_query_embedding(
        &self,
        text_hash: &str,
        model: &str,
    ) -> Result<Option<Vec<f32>>, FactbaseError> {
        let conn = self.get_conn()?;
        let result: Result<Vec<u8>, _> = conn.query_row(
            "SELECT embedding FROM query_embedding_cache WHERE text_hash = ?1 AND model = ?2",
            rusqlite::params![text_hash, model],
            |row| row.get(0),
        );
        match result {
            Ok(bytes) => {
                // Update last_used_at (best-effort)
                let _ = conn.execute(
                    "UPDATE query_embedding_cache SET last_used_at = datetime('now') WHERE text_hash = ?1 AND model = ?2",
                    rusqlite::params![text_hash, model],
                );
                let embedding: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                Ok(Some(embedding))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store a query embedding in the persistent cache.
    pub fn put_cached_query_embedding(
        &self,
        text_hash: &str,
        text: &str,
        model: &str,
        dimension: usize,
        embedding: &[f32],
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO query_embedding_cache (text_hash, text, model, dimension, embedding, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))",
            rusqlite::params![text_hash, text, model, dimension as i64, embedding.as_bytes()],
        )?;
        Ok(())
    }

    /// Evict oldest entries to keep cache at or below `max_entries`.
    pub fn evict_query_cache(&self, max_entries: usize) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM query_embedding_cache", [], |row| {
                row.get(0)
            })?;
        let to_delete = (count as usize).saturating_sub(max_entries);
        if to_delete == 0 {
            return Ok(0);
        }
        conn.execute(
            "DELETE FROM query_embedding_cache WHERE text_hash IN (
                SELECT text_hash FROM query_embedding_cache ORDER BY last_used_at ASC LIMIT ?1
            )",
            [to_delete as i64],
        )?;
        Ok(to_delete)
    }

    /// Count entries in the query embedding cache.
    pub fn count_query_cache(&self) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM query_embedding_cache", [], |row| {
                row.get(0)
            })?;
        Ok(count as usize)
    }

    /// Clear all entries from the query embedding cache.
    pub fn clear_query_cache(&self) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let deleted = conn.execute("DELETE FROM query_embedding_cache", [])?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::test_db;

    #[test]
    fn test_put_and_get_cached_query_embedding() {
        let (db, _tmp) = test_db();
        let embedding = vec![0.1f32, 0.2, 0.3, 0.4];
        db.put_cached_query_embedding("hash1", "hello world", "model-a", 4, &embedding)
            .unwrap();

        let result = db.get_cached_query_embedding("hash1", "model-a").unwrap();
        assert_eq!(result, Some(embedding));
    }

    #[test]
    fn test_cache_miss() {
        let (db, _tmp) = test_db();
        let result = db
            .get_cached_query_embedding("nonexistent", "model-a")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_model_aware_cache() {
        let (db, _tmp) = test_db();
        let emb_a = vec![0.1f32, 0.2];
        let emb_b = vec![0.3f32, 0.4];
        db.put_cached_query_embedding("hash1", "text", "model-a", 2, &emb_a)
            .unwrap();
        db.put_cached_query_embedding("hash1", "text", "model-b", 2, &emb_b)
            .unwrap();

        assert_eq!(
            db.get_cached_query_embedding("hash1", "model-a").unwrap(),
            Some(emb_a)
        );
        assert_eq!(
            db.get_cached_query_embedding("hash1", "model-b").unwrap(),
            Some(emb_b)
        );
    }

    #[test]
    fn test_evict_query_cache() {
        let (db, _tmp) = test_db();
        for i in 0..5 {
            let emb = vec![i as f32; 4];
            db.put_cached_query_embedding(
                &format!("hash{i}"),
                &format!("text{i}"),
                "model",
                4,
                &emb,
            )
            .unwrap();
        }
        assert_eq!(db.count_query_cache().unwrap(), 5);

        let evicted = db.evict_query_cache(3).unwrap();
        assert_eq!(evicted, 2);
        assert_eq!(db.count_query_cache().unwrap(), 3);
    }

    #[test]
    fn test_evict_no_op_when_under_limit() {
        let (db, _tmp) = test_db();
        let emb = vec![0.1f32; 4];
        db.put_cached_query_embedding("hash1", "text", "model", 4, &emb)
            .unwrap();

        let evicted = db.evict_query_cache(100).unwrap();
        assert_eq!(evicted, 0);
        assert_eq!(db.count_query_cache().unwrap(), 1);
    }

    #[test]
    fn test_clear_query_cache() {
        let (db, _tmp) = test_db();
        for i in 0..3 {
            let emb = vec![i as f32; 4];
            db.put_cached_query_embedding(
                &format!("hash{i}"),
                &format!("text{i}"),
                "model",
                4,
                &emb,
            )
            .unwrap();
        }
        assert_eq!(db.count_query_cache().unwrap(), 3);

        let cleared = db.clear_query_cache().unwrap();
        assert_eq!(cleared, 3);
        assert_eq!(db.count_query_cache().unwrap(), 0);
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let (db, _tmp) = test_db();
        let emb1 = vec![0.1f32, 0.2];
        let emb2 = vec![0.3f32, 0.4];
        db.put_cached_query_embedding("hash1", "text", "model", 2, &emb1)
            .unwrap();
        db.put_cached_query_embedding("hash1", "text", "model", 2, &emb2)
            .unwrap();

        let result = db.get_cached_query_embedding("hash1", "model").unwrap();
        assert_eq!(result, Some(emb2));
        assert_eq!(db.count_query_cache().unwrap(), 1);
    }
}
