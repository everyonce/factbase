//! Entity discovery: detect frequently-mentioned names without their own document.
//!
//! Scans documents via LLM to find proper nouns and named concepts that appear
//! across multiple documents but don't have a dedicated entity document yet.

use crate::error::FactbaseError;
use crate::models::{Document, Perspective};
use crate::progress::ProgressReporter;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A suggested entity discovered across multiple documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedEntity {
    /// The candidate entity name.
    pub name: String,
    /// Suggested document type (from perspective classification), if available.
    pub suggested_type: Option<String>,
    /// Document IDs where this entity was mentioned.
    pub mentioned_in: Vec<String>,
    /// Confidence level: "high" or "medium".
    pub confidence: String,
}

/// Discover entities mentioned across documents that lack their own document.
///
/// Currently a no-op — returns empty results. Entity discovery previously
/// required an LLM provider; callers should implement their own discovery logic.
pub async fn discover_entities(
    docs: &[Document],
    _existing_titles: &[String],
    _perspective: Option<&Perspective>,
    _progress: &ProgressReporter,
    _doc_offset: usize,
    _deadline: Option<Instant>,
) -> Result<(Vec<SuggestedEntity>, usize), FactbaseError> {
    Ok((Vec::new(), docs.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::ProgressReporter;

    #[tokio::test]
    async fn test_discover_entities_returns_empty() {
        let progress = ProgressReporter::Silent;
        let (results, processed) = discover_entities(&[], &[], None, &progress, 0, None)
            .await
            .unwrap();
        assert!(results.is_empty());
        assert_eq!(processed, 0);

        // With docs, processed should equal docs.len()
        let doc = crate::models::Document::test_default();
        let docs = vec![doc];
        let (results, processed) = discover_entities(&docs, &[], None, &progress, 0, None)
            .await
            .unwrap();
        assert!(results.is_empty());
        assert_eq!(processed, 1);
    }
}
