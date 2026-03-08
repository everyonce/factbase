//! Merge planning for document reorganization.
//!
//! Creates a plan for merging multiple documents into one, with fact-level
//! accounting to ensure no data is lost.

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;

use crate::organize::{extract_facts, FactDestination, FactLedger, TemporalIssue, TrackedFact};

/// A plan for merging multiple documents into one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    /// Document ID to keep (target)
    pub keep_id: String,
    /// Document IDs being merged into the target
    pub merge_ids: Vec<String>,
    /// Fact ledger tracking all facts through the merge
    pub ledger: FactLedger,
    /// Combined content for the kept document
    pub combined_content: String,
    /// Temporal consistency issues detected during planning
    pub temporal_issues: Vec<TemporalIssue>,
}

impl MergePlan {
    /// Check if the plan is valid (ledger is balanced).
    pub fn is_valid(&self) -> bool {
        self.ledger.is_balanced()
    }

    /// Get count of facts that will be orphaned.
    pub fn orphan_count(&self) -> usize {
        self.ledger.orphan_count()
    }

    /// Get count of facts identified as duplicates.
    pub fn duplicate_count(&self) -> usize {
        self.ledger
            .assignments
            .values()
            .filter(|a| a.destination == FactDestination::Duplicate)
            .count()
    }
}

/// Create a merge plan for combining documents.
///
/// Extracts facts from all source documents and uses a simple heuristic
/// to assign all facts to the target document (keep everything).
///
/// # Arguments
/// * `keep_id` - ID of the document to keep (target)
/// * `merge_ids` - IDs of documents to merge into the target
/// * `db` - Database connection
///
/// # Returns
/// A `MergePlan` with fact assignments and combined content.
pub async fn plan_merge(
    keep_id: &str,
    merge_ids: &[&str],
    db: &Database,
) -> Result<MergePlan, FactbaseError> {
    // Get the target document
    let keep_doc = db.require_document(keep_id)?;

    // Get all documents to merge
    let mut merge_docs = Vec::new();
    for id in merge_ids {
        let doc = db.require_document(id)?;
        merge_docs.push(doc);
    }

    // Extract facts from all documents
    let mut ledger = FactLedger::new();
    let keep_facts = extract_facts(&keep_doc.content, keep_id);
    for fact in &keep_facts {
        ledger.add_fact(fact.clone());
    }

    let mut all_merge_facts: Vec<(String, Vec<TrackedFact>)> = Vec::new();
    for doc in &merge_docs {
        let facts = extract_facts(&doc.content, &doc.id);
        for fact in &facts {
            ledger.add_fact(fact.clone());
        }
        all_merge_facts.push((doc.id.clone(), facts));
    }

    // Assign all facts to the target document (let the agent decide what to prune)
    for fact in &keep_facts {
        ledger.assign(
            &fact.id,
            FactDestination::Document,
            Some(keep_id.to_string()),
            Some("target document fact".to_string()),
        );
    }
    for (_, facts) in &all_merge_facts {
        for fact in facts {
            ledger.assign(
                &fact.id,
                FactDestination::Document,
                Some(keep_id.to_string()),
                Some("merged fact".to_string()),
            );
        }
    }

    // Build combined content from kept facts
    let combined_content = build_combined_content(&keep_doc, &ledger);

    Ok(MergePlan {
        keep_id: keep_id.to_string(),
        merge_ids: merge_ids.iter().map(ToString::to_string).collect(),
        ledger,
        combined_content,
        temporal_issues: Vec::new(),
    })
}

/// Build combined content from the kept document and assigned facts.
fn build_combined_content(keep_doc: &crate::models::Document, ledger: &FactLedger) -> String {
    let mut content = keep_doc.content.clone();

    // Get facts assigned to this document that aren't from the original
    let new_facts: Vec<&TrackedFact> = ledger
        .source_facts
        .iter()
        .filter(|f| {
            f.source_doc != keep_doc.id
                && ledger
                    .assignments
                    .get(&f.id)
                    .is_some_and(|a| a.destination == FactDestination::Document)
        })
        .collect();

    if !new_facts.is_empty() {
        // Append new facts at the end
        content.push_str("\n\n## Merged Content\n\n");
        for fact in new_facts {
            content.push_str(&fact.content);
            content.push('\n');
        }
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_plan_is_valid() {
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "test fact", None, vec![]);
        let fact_id = fact.id.clone();
        ledger.add_fact(fact);
        ledger.assign(
            &fact_id,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content: "test".to_string(),
            temporal_issues: vec![],
        };

        assert!(plan.is_valid());
    }

    #[test]
    fn test_merge_plan_orphan_count() {
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc2", 1, "fact 2", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(&id2, FactDestination::Orphan, None, None);

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content: "test".to_string(),
            temporal_issues: vec![],
        };

        assert_eq!(plan.orphan_count(), 1);
    }

    #[test]
    fn test_merge_plan_duplicate_count() {
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc2", 1, "fact 1", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(&id2, FactDestination::Duplicate, None, None);

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content: "test".to_string(),
            temporal_issues: vec![],
        };

        assert_eq!(plan.duplicate_count(), 1);
    }
}
