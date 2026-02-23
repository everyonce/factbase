//! Compression statistics computation.

use super::super::{Database, DbConn};
use crate::error::FactbaseError;
use crate::models::CompressionStats;
use base64::Engine;

impl Database {
    pub(crate) fn compute_compression_stats(
        conn: &DbConn,
        repo_id: &str,
    ) -> Result<Option<CompressionStats>, FactbaseError> {
        let mut stmt = conn.prepare_cached(super::CONTENT_ONLY_QUERY)?;
        let mut rows = stmt.query([repo_id])?;

        #[allow(unused_mut)]
        let mut compressed_docs = 0usize;
        let mut total_docs = 0usize;
        let mut compressed_size = 0usize;
        let mut original_size = 0usize;

        while let Some(row) = rows.next()? {
            let content: String = row.get(0)?;
            total_docs += 1;
            compressed_size += content.len();

            if let Ok(decoded) = super::super::B64.decode(&content) {
                #[cfg(feature = "compression")]
                {
                    if decoded.starts_with(super::super::ZSTD_PREFIX) {
                        compressed_docs += 1;
                        if let Ok(decompressed) = super::super::decompress_content(&decoded) {
                            original_size += decompressed.len();
                        } else {
                            original_size += content.len();
                        }
                        continue;
                    }
                }
                original_size += decoded.len();
                continue;
            }
            original_size += content.len();
        }

        if total_docs == 0 {
            return Ok(None);
        }

        let savings_percent = if original_size > 0 {
            ((original_size - compressed_size) as f64 / original_size as f64) * 100.0
        } else {
            0.0
        };

        Ok(Some(CompressionStats {
            compressed_docs,
            total_docs,
            compressed_size,
            original_size,
            savings_percent,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_doc, test_repo};
    use crate::database::Database;

    #[test]
    #[cfg(feature = "compression")]
    fn test_compression_stats() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let db_path = tmp.path().join("stats.db");
        let db = Database::with_options(&db_path, 4, true).expect("db with compression");

        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "a".repeat(1000);
        db.upsert_document(&doc).expect("upsert");

        let stats = db
            .get_detailed_stats(&repo.id, None)
            .expect("get_detailed_stats");

        let cs = stats.compression_stats.expect("compression_stats");
        assert_eq!(cs.total_docs, 1);
        assert_eq!(cs.compressed_docs, 1);
        assert_eq!(cs.original_size, 1000);
        assert!(cs.compressed_size < cs.original_size);
        assert!(cs.savings_percent > 0.0);
    }
}
