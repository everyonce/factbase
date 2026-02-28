//! Misplaced document detection.
//!
//! Identifies documents whose content doesn't match their folder-derived type
//! by comparing document embeddings to type centroids.

use super::{
    collect_active_documents, compute_centroid, cosine_similarity, get_document_embedding,
};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::MisplacedCandidate;
use crate::ProgressReporter;
use std::collections::HashMap;

/// Minimum documents per type to compute a meaningful centroid.
const MIN_DOCS_PER_TYPE: usize = 2;

/// Detect documents that may be misplaced based on embedding similarity to type centroids.
///
/// Computes centroid embedding per document type, then compares each document's
/// embedding to all centroids. If the closest centroid differs from the document's
/// current type, it's flagged as a misplaced candidate.
///
/// # Arguments
/// * `db` - Database connection
/// * `repo_id` - Optional repository filter
///
/// # Returns
/// Vector of misplaced candidates sorted by confidence descending.
pub fn detect_misplaced(
    db: &Database,
    repo_id: Option<&str>,
    progress: &ProgressReporter,
) -> Result<Vec<MisplacedCandidate>, FactbaseError> {
    // Get all documents with their types
    let docs = get_documents_with_types(db, repo_id)?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    progress.phase("Detecting misplaced documents");

    // Group documents by type
    let docs_by_type = group_by_type(&docs);

    // Compute centroid embedding per type (only for types with enough docs)
    let centroids = compute_type_centroids(db, &docs_by_type)?;
    if centroids.is_empty() {
        return Ok(Vec::new());
    }

    // Compare each document to all centroids
    let mut candidates = Vec::new();
    let total = docs.len();
    for (i, (doc_id, doc_title, current_type)) in docs.iter().enumerate() {
        progress.report(i + 1, total, doc_title);
        // Skip docs whose type doesn't have a centroid (too few docs)
        if !centroids.contains_key(current_type) {
            continue;
        }

        // Get document embedding
        let Some(embedding) = get_document_embedding(db, doc_id)? else {
            continue;
        };

        // Find closest centroid
        let (closest_type, closest_sim) = find_closest_centroid(&embedding, &centroids);

        // If closest type differs from current type, it's a candidate
        if closest_type != *current_type {
            // Calculate similarity to current type for confidence
            let current_sim = if let Some(current_centroid) = centroids.get(current_type) {
                cosine_similarity(&embedding, current_centroid)
            } else {
                0.0
            };

            // Confidence is how much closer the suggested type is
            let confidence = closest_sim - current_sim;

            // Only report if confidence is positive (suggested type is actually closer)
            if confidence > 0.0 {
                candidates.push(MisplacedCandidate {
                    doc_id: doc_id.clone(),
                    doc_title: doc_title.clone(),
                    current_type: current_type.clone(),
                    suggested_type: closest_type.clone(),
                    confidence,
                    rationale: format!(
                        "Similarity to '{closest_type}': {closest_sim:.2}, to '{current_type}': {current_sim:.2}"
                    ),
                });
            }
        }
    }

    // Sort by confidence descending
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(candidates)
}

/// Get all non-deleted documents with their types.
fn get_documents_with_types(
    db: &Database,
    repo_id: Option<&str>,
) -> Result<Vec<(String, String, String)>, FactbaseError> {
    Ok(collect_active_documents(db, repo_id)?
        .into_iter()
        .map(|d| (d.id, d.title, d.doc_type.unwrap_or_default()))
        .collect())
}

/// Group documents by their type.
fn group_by_type(docs: &[(String, String, String)]) -> HashMap<String, Vec<String>> {
    let mut by_type: HashMap<String, Vec<String>> = HashMap::new();
    for (doc_id, _, doc_type) in docs {
        if !doc_type.is_empty() {
            by_type
                .entry(doc_type.clone())
                .or_default()
                .push(doc_id.clone());
        }
    }
    by_type
}

/// Compute centroid embedding for each type with enough documents.
fn compute_type_centroids(
    db: &Database,
    docs_by_type: &HashMap<String, Vec<String>>,
) -> Result<HashMap<String, Vec<f32>>, FactbaseError> {
    let mut centroids = HashMap::new();

    for (doc_type, doc_ids) in docs_by_type {
        if doc_ids.len() < MIN_DOCS_PER_TYPE {
            continue;
        }

        // Collect embeddings for this type
        let mut embeddings = Vec::new();
        for doc_id in doc_ids {
            if let Some(emb) = get_document_embedding(db, doc_id)? {
                embeddings.push(emb);
            }
        }

        if embeddings.len() < MIN_DOCS_PER_TYPE {
            continue;
        }

        // Compute centroid (average of all embeddings)
        let centroid = compute_centroid(&embeddings);
        centroids.insert(doc_type.clone(), centroid);
    }

    Ok(centroids)
}

/// Find the type with the closest centroid to the given embedding.
fn find_closest_centroid(
    embedding: &[f32],
    centroids: &HashMap<String, Vec<f32>>,
) -> (String, f32) {
    let mut best_type = String::new();
    let mut best_sim = f32::NEG_INFINITY;

    for (doc_type, centroid) in centroids {
        let sim = cosine_similarity(embedding, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_type = doc_type.clone();
        }
    }

    (best_type, best_sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_closest_centroid() {
        let mut centroids = HashMap::new();
        centroids.insert("type_a".to_string(), vec![1.0, 0.0, 0.0]);
        centroids.insert("type_b".to_string(), vec![0.0, 1.0, 0.0]);

        let embedding = vec![0.9, 0.1, 0.0];
        let (closest, sim) = find_closest_centroid(&embedding, &centroids);
        assert_eq!(closest, "type_a");
        assert!(sim > 0.9);
    }

    #[test]
    fn test_group_by_type() {
        let docs = vec![
            (
                "doc1".to_string(),
                "Doc 1".to_string(),
                "person".to_string(),
            ),
            (
                "doc2".to_string(),
                "Doc 2".to_string(),
                "person".to_string(),
            ),
            (
                "doc3".to_string(),
                "Doc 3".to_string(),
                "project".to_string(),
            ),
            ("doc4".to_string(), "Doc 4".to_string(), "".to_string()), // Empty type ignored
        ];
        let by_type = group_by_type(&docs);
        assert_eq!(by_type.get("person").map(|v| v.len()), Some(2));
        assert_eq!(by_type.get("project").map(|v| v.len()), Some(1));
        assert!(!by_type.contains_key(""));
    }

    #[test]
    fn test_misplaced_candidate_struct() {
        let candidate = MisplacedCandidate {
            doc_id: "abc123".to_string(),
            doc_title: "John Smith".to_string(),
            current_type: "project".to_string(),
            suggested_type: "person".to_string(),
            confidence: 0.15,
            rationale: "Similarity to 'person': 0.85, to 'project': 0.70".to_string(),
        };
        assert_eq!(candidate.doc_id, "abc123");
        assert_eq!(candidate.current_type, "project");
        assert_eq!(candidate.suggested_type, "person");
        assert!(candidate.confidence > 0.0);
    }
}
