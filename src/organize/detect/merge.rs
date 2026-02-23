//! Merge candidate detection.
//!
//! Identifies pairs of documents with high similarity that could be merged.

use super::collect_active_documents;
use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::normalize_pair;
use crate::organize::MergeCandidate;
use std::collections::HashSet;

/// Detects documents that are candidates for merging based on embedding similarity.
///
/// Uses the existing `find_similar_documents` infrastructure to identify pairs
/// of documents above the similarity threshold. For each pair, suggests which
/// document to keep based on content length and link count.
///
/// # Arguments
/// * `db` - Database connection
/// * `threshold` - Minimum similarity score (0.0 to 1.0, default 0.95)
/// * `repo_id` - Optional repository filter
///
/// # Returns
/// Vector of merge candidates, deduplicated (each pair appears once).
pub fn detect_merge_candidates(
    db: &Database,
    threshold: f32,
    repo_id: Option<&str>,
) -> Result<Vec<MergeCandidate>, FactbaseError> {
    let docs = collect_active_documents(db, repo_id)?;

    let mut candidates = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for doc in &docs {
        let similar = db.find_similar_documents(&doc.id, threshold)?;

        for (similar_id, similar_title, similarity) in similar {
            // Create canonical pair key (smaller ID first) to avoid duplicates
            let pair_key = normalize_pair(&doc.id, &similar_id);

            if seen_pairs.contains(&pair_key) {
                continue;
            }
            seen_pairs.insert(pair_key);

            // Get the similar document for comparison
            let similar_doc = match db.get_document(&similar_id)? {
                Some(d) => d,
                None => continue,
            };

            // Determine which document to keep based on content length and links
            let doc_links_from = db.get_links_from(&doc.id)?.len();
            let doc_links_to = db.get_links_to(&doc.id)?.len();
            let similar_links_from = db.get_links_from(&similar_id)?.len();
            let similar_links_to = db.get_links_to(&similar_id)?.len();

            let doc_score = doc.content.len() + (doc_links_from + doc_links_to) * 100;
            let similar_score =
                similar_doc.content.len() + (similar_links_from + similar_links_to) * 100;

            let (suggested_keep, rationale) = if doc_score >= similar_score {
                (
                    doc.id.clone(),
                    format!(
                        "Keep '{}': {} chars, {} links vs '{}': {} chars, {} links",
                        doc.title,
                        doc.content.len(),
                        doc_links_from + doc_links_to,
                        similar_title,
                        similar_doc.content.len(),
                        similar_links_from + similar_links_to
                    ),
                )
            } else {
                (
                    similar_id.clone(),
                    format!(
                        "Keep '{}': {} chars, {} links vs '{}': {} chars, {} links",
                        similar_title,
                        similar_doc.content.len(),
                        similar_links_from + similar_links_to,
                        doc.title,
                        doc.content.len(),
                        doc_links_from + doc_links_to
                    ),
                )
            };

            candidates.push(MergeCandidate {
                doc1_id: doc.id.clone(),
                doc1_title: doc.title.clone(),
                doc2_id: similar_id,
                doc2_title: similar_title,
                similarity,
                suggested_keep,
                rationale,
            });
        }
    }

    // Sort by similarity descending
    candidates.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .expect("non-NaN similarity")
    });

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_candidate_struct() {
        let candidate = MergeCandidate {
            doc1_id: "abc123".to_string(),
            doc1_title: "Document A".to_string(),
            doc2_id: "def456".to_string(),
            doc2_title: "Document B".to_string(),
            similarity: 0.97,
            suggested_keep: "abc123".to_string(),
            rationale: "Keep 'Document A': more content".to_string(),
        };

        assert_eq!(candidate.doc1_id, "abc123");
        assert_eq!(candidate.similarity, 0.97);
        assert_eq!(candidate.suggested_keep, "abc123");
    }

    // Integration tests with real database require Ollama for embeddings
    // and are marked with #[ignore]. Run with: cargo test -- --ignored
}
