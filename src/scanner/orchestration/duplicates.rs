//! Duplicate detection for scan orchestration

use std::collections::HashSet;

use crate::{models::normalize_pair, models::DuplicatePair, Database};

/// Threshold for considering documents as duplicates (95% similarity)
pub(super) const DUPLICATE_THRESHOLD: f32 = 0.95;

/// Check for duplicate documents among changed documents
pub(super) fn check_duplicates(
    db: &Database,
    changed_ids: &HashSet<String>,
) -> anyhow::Result<Vec<DuplicatePair>> {
    let mut duplicates = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for doc_id in changed_ids {
        if let Ok(similar) = db.find_similar_documents(doc_id, DUPLICATE_THRESHOLD) {
            for (similar_id, similar_title, similarity) in similar {
                // Avoid duplicate pairs (A,B) and (B,A)
                let pair = normalize_pair(doc_id, &similar_id);
                if seen_pairs.insert(pair) {
                    let doc_title = db
                        .get_document(doc_id)?
                        .map(|d| d.title)
                        .unwrap_or_else(|| doc_id.clone());
                    duplicates.push(DuplicatePair {
                        doc1_id: doc_id.clone(),
                        doc1_title: doc_title,
                        doc2_id: similar_id,
                        doc2_title: similar_title,
                        similarity,
                    });
                }
            }
        }
    }

    Ok(duplicates)
}
