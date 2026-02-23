//! Aggregate lint checks across multiple documents.
//!
//! This module contains functions that operate on collections of documents.

use factbase::{normalize_pair, Database, Document};
use std::collections::HashSet;

/// Check for duplicate documents in a repository.
pub fn check_duplicates(
    docs: &[Document],
    db: &Database,
    min_similarity: f32,
    is_table_format: bool,
) -> anyhow::Result<usize> {
    if !(0.0..=1.0).contains(&min_similarity) {
        anyhow::bail!("--min-similarity must be between 0.0 and 1.0");
    }

    let mut warnings = 0;
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for doc in docs {
        if let Ok(similar) = db.find_similar_documents(&doc.id, min_similarity) {
            for (similar_id, similar_title, similarity) in similar {
                // Avoid duplicate pairs (A,B) and (B,A)
                let pair = normalize_pair(&doc.id, &similar_id);
                if seen_pairs.insert(pair) {
                    if is_table_format {
                        println!(
                            "  WARN: Potential duplicate: {} [{}] ↔ {} [{}] ({:.1}% similar)",
                            doc.title,
                            doc.id,
                            similar_title,
                            similar_id,
                            similarity * 100.0
                        );
                    }
                    warnings += 1;
                }
            }
        }
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_helpers::{make_test_doc, make_test_repo, test_db};

    fn test_doc(id: &str, title: &str) -> Document {
        Document {
            title: title.to_string(),
            content: format!("# {title}\n\nContent here."),
            ..make_test_doc(id)
        }
    }

    #[test]
    fn test_check_duplicates_invalid_similarity_too_low() {
        let (db, _tmp) = test_db();
        let docs: Vec<Document> = vec![];
        let result = check_duplicates(&docs, &db, -0.1, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--min-similarity must be between 0.0 and 1.0"));
    }

    #[test]
    fn test_check_duplicates_invalid_similarity_too_high() {
        let (db, _tmp) = test_db();
        let docs: Vec<Document> = vec![];
        let result = check_duplicates(&docs, &db, 1.1, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--min-similarity must be between 0.0 and 1.0"));
    }

    #[test]
    fn test_check_duplicates_valid_boundary_values() {
        let (db, _tmp) = test_db();
        let docs: Vec<Document> = vec![];
        // 0.0 should be valid
        let result = check_duplicates(&docs, &db, 0.0, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        // 1.0 should be valid
        let result = check_duplicates(&docs, &db, 1.0, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_check_duplicates_empty_docs() {
        let (db, _tmp) = test_db();
        let docs: Vec<Document> = vec![];
        let result = check_duplicates(&docs, &db, 0.95, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_check_duplicates_no_similar_found() {
        let (db, _tmp) = test_db();
        let repo = make_test_repo();
        db.upsert_repository(&repo).unwrap();

        // Create two docs with different embeddings
        let doc1 = test_doc("doc1", "Document One");
        let doc2 = test_doc("doc2", "Document Two");
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        // Create very different embeddings
        let emb1: Vec<f32> = vec![1.0; 1024];
        let emb2: Vec<f32> = vec![-1.0; 1024];
        db.upsert_embedding("doc1", &emb1).unwrap();
        db.upsert_embedding("doc2", &emb2).unwrap();

        let docs = vec![doc1, doc2];
        let result = check_duplicates(&docs, &db, 0.95, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // No duplicates at 95% threshold
    }

    #[test]
    fn test_check_duplicates_finds_similar() {
        let (db, _tmp) = test_db();
        let repo = make_test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc1 = test_doc("doc1", "Document One");
        let doc2 = test_doc("doc2", "Document Two");
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        // Create nearly identical embeddings
        let emb1: Vec<f32> = vec![0.5; 1024];
        let mut emb2: Vec<f32> = vec![0.5; 1024];
        emb2[0] = 0.501; // Very small difference

        db.upsert_embedding("doc1", &emb1).unwrap();
        db.upsert_embedding("doc2", &emb2).unwrap();

        let docs = vec![doc1, doc2];
        let result = check_duplicates(&docs, &db, 0.95, false);
        assert!(result.is_ok());
        // Should find 1 duplicate pair (not 2, due to deduplication)
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_check_duplicates_deduplicates_pairs() {
        let (db, _tmp) = test_db();
        let repo = make_test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc1 = test_doc("aaa111", "Document A");
        let doc2 = test_doc("bbb222", "Document B");
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        // Create identical embeddings
        let emb: Vec<f32> = vec![0.5; 1024];
        db.upsert_embedding("aaa111", &emb).unwrap();
        db.upsert_embedding("bbb222", &emb).unwrap();

        // Both docs in list - should only count pair once
        let docs = vec![doc1, doc2];
        let result = check_duplicates(&docs, &db, 0.95, false);
        assert!(result.is_ok());
        // (A,B) found when processing A, (B,A) skipped when processing B
        assert_eq!(result.unwrap(), 1);
    }
}
