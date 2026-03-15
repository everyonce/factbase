//! Merge candidate detection.
//!
//! Identifies pairs of documents with high similarity that could be merged.

use super::collect_active_documents;
use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::normalize_pair;
use crate::organize::MergeCandidate;
use crate::ProgressReporter;
use std::collections::HashMap;
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
    progress: &ProgressReporter,
) -> Result<Vec<MergeCandidate>, FactbaseError> {
    let docs = collect_active_documents(db, repo_id)?;

    progress.phase("Detecting merge candidates");

    let mut candidates = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
    let total = docs.len();

    // Pre-fetch link counts for all docs in one batch (2 queries instead of 4 per candidate)
    let all_doc_ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let link_counts: HashMap<String, (usize, usize)> =
        db.get_link_counts_batch(&all_doc_ids).unwrap_or_default();

    for (i, doc) in docs.iter().enumerate() {
        progress.report(i + 1, total, &doc.title);
        let similar = db.find_similar_documents(&doc.id, threshold)?;

        for (similar_id, similar_title, similarity) in similar {
            // Create canonical pair key (smaller ID first) to avoid duplicates
            let pair_key = normalize_pair(&doc.id, &similar_id);

            if seen_pairs.contains(&pair_key) {
                continue;
            }
            seen_pairs.insert(pair_key);

            // Get the similar document for comparison
            let Some(similar_doc) = db.get_document(&similar_id)? else {
                continue;
            };

            // Determine which document to keep based on content length and links.
            // Use pre-fetched counts; fall back to (0,0) for docs outside the batch
            // (e.g. similar doc from a different repo when repo_id filter is active).
            let (doc_links_from, doc_links_to) =
                link_counts.get(&doc.id).copied().unwrap_or((0, 0));
            let (similar_links_from, similar_links_to) =
                link_counts.get(&similar_id).copied().unwrap_or_else(|| {
                    // similar_id not in pre-fetched set — fetch individually
                    db.get_link_counts_batch(&[similar_id.as_str()])
                        .ok()
                        .and_then(|mut m| m.remove(&similar_id))
                        .unwrap_or((0, 0))
                });

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

    #[test]
    fn test_detect_merge_candidates_empty_repo() {
        use crate::database::tests::{test_db, test_repo_in_db};
        use tempfile::TempDir;
        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        test_repo_in_db(&db, "test", repo_dir.path());
        let result =
            detect_merge_candidates(&db, 0.95, Some("test"), &crate::ProgressReporter::Silent)
                .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_detect_merge_candidates_no_similar_docs() {
        use crate::database::tests::{test_db, test_doc_with_repo, test_repo_in_db};
        use tempfile::TempDir;
        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        test_repo_in_db(&db, "test", repo_dir.path());
        // Insert docs but no embeddings — find_similar_documents returns empty
        db.upsert_document(&test_doc_with_repo("d1", "test", "Doc 1"))
            .unwrap();
        db.upsert_document(&test_doc_with_repo("d2", "test", "Doc 2"))
            .unwrap();
        let result =
            detect_merge_candidates(&db, 0.95, Some("test"), &crate::ProgressReporter::Silent)
                .unwrap();
        assert!(result.is_empty());
    }
}
