//! Detection of reorganization opportunities.
//!
//! This module identifies documents that could benefit from reorganization:
//! - Merge candidates: highly similar documents that may be duplicates
//! - Split candidates: documents covering multiple distinct topics
//! - Misplaced candidates: documents in wrong folders

mod merge;
mod misplaced;
mod split;

pub use merge::detect_merge_candidates;
pub use misplaced::detect_misplaced;
pub use split::{detect_split_candidates, extract_sections};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::Document;

// Cosine similarity between two embedding vectors.
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

// Compute centroid (element-wise average) of embedding vectors.
pub(crate) fn compute_centroid(embeddings: &[Vec<f32>]) -> Vec<f32> {
    if embeddings.is_empty() {
        return Vec::new();
    }

    let dim = embeddings[0].len();
    let mut centroid = vec![0.0f32; dim];
    let count = embeddings.len() as f32;

    for emb in embeddings {
        for (i, &val) in emb.iter().enumerate() {
            centroid[i] += val;
        }
    }

    for val in &mut centroid {
        *val /= count;
    }

    centroid
}

// Get embedding for a document (first chunk).
pub(crate) fn get_document_embedding(
    db: &Database,
    doc_id: &str,
) -> Result<Option<Vec<f32>>, FactbaseError> {
    let conn = db.get_conn()?;
    let chunk_id = format!("{}_0", doc_id);

    let result: Result<Vec<u8>, _> = conn.query_row(
        "SELECT embedding FROM document_embeddings WHERE id = ?1",
        [&chunk_id],
        |r| r.get(0),
    );

    match result {
        Ok(bytes) => {
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            Ok(Some(floats))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// Collects all non-deleted documents, optionally filtered by repository.
pub(crate) fn collect_active_documents(
    db: &Database,
    repo_id: Option<&str>,
) -> Result<Vec<Document>, FactbaseError> {
    match repo_id {
        Some(rid) => Ok(db
            .get_documents_for_repo(rid)?
            .into_values()
            .filter(|d| !d.is_deleted)
            .collect()),
        None => {
            let mut all_docs = Vec::new();
            for repo in db.list_repositories()? {
                let map = db.get_documents_for_repo(&repo.id)?;
                all_docs.extend(map.into_values().filter(|d| !d.is_deleted));
            }
            Ok(all_docs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_centroid() {
        let embeddings = vec![vec![1.0, 2.0, 3.0], vec![3.0, 4.0, 5.0]];
        let centroid = compute_centroid(&embeddings);
        assert_eq!(centroid, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_compute_centroid_empty() {
        let embeddings: Vec<Vec<f32>> = vec![];
        let centroid = compute_centroid(&embeddings);
        assert!(centroid.is_empty());
    }

    #[test]
    fn test_compute_centroid_single() {
        let embeddings = vec![vec![1.0, 2.0, 3.0]];
        let centroid = compute_centroid(&embeddings);
        assert_eq!(centroid, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }
}
