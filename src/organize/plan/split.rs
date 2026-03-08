//! Split planning for document reorganization.
//!
//! Creates a plan for splitting a document into multiple documents based on
//! sections, with fact-level accounting to ensure no data is lost.

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::{
    extract_facts, FactDestination, FactLedger, SplitSection, TemporalIssue, TrackedFact,
};

/// A plan for splitting a document into multiple documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitPlan {
    /// Source document ID being split
    pub source_id: String,
    /// Proposed new documents with their content
    pub new_documents: Vec<ProposedDocument>,
    /// Fact ledger tracking all facts through the split
    pub ledger: FactLedger,
    /// Temporal consistency issues detected during planning
    pub temporal_issues: Vec<TemporalIssue>,
}

/// A proposed new document from a split operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedDocument {
    /// Proposed title for the new document
    pub title: String,
    /// Section this document is based on
    pub section_title: String,
    /// Content for the new document
    pub content: String,
}

impl SplitPlan {
    /// Check if the plan is valid (ledger is balanced).
    pub fn is_valid(&self) -> bool {
        self.ledger.is_balanced()
    }

    /// Get count of facts that will be orphaned.
    pub fn orphan_count(&self) -> usize {
        self.ledger.orphan_count()
    }

    /// Get count of new documents to be created.
    pub fn document_count(&self) -> usize {
        self.new_documents.len()
    }
}

/// Create a split plan for dividing a document into multiple documents.
///
/// Extracts facts from the source document and assigns each fact to the
/// section whose line range contains the fact's source line.
///
/// # Arguments
/// * `doc_id` - ID of the document to split
/// * `sections` - Sections identified in the document (from detect_split_candidates)
/// * `db` - Database connection
///
/// # Returns
/// A `SplitPlan` with fact assignments and proposed documents.
pub async fn plan_split(
    doc_id: &str,
    sections: &[SplitSection],
    db: &Database,
) -> Result<SplitPlan, FactbaseError> {
    // Get the source document
    let doc = db.require_document(doc_id)?;

    // Extract facts from the document
    let facts = extract_facts(&doc.content, doc_id);
    let mut ledger = FactLedger::new();
    for fact in &facts {
        ledger.add_fact(fact.clone());
    }

    // Assign facts to sections based on line numbers
    for fact in &facts {
        let mut assigned = false;
        for (idx, section) in sections.iter().enumerate() {
            if fact.source_line >= section.start_line && fact.source_line <= section.end_line {
                ledger.assign(
                    &fact.id,
                    FactDestination::Document,
                    Some(format!("section_{idx}")),
                    Some(format!("line {} in section \"{}\"", fact.source_line, section.title)),
                );
                assigned = true;
                break;
            }
        }
        if !assigned {
            ledger.assign(
                &fact.id,
                FactDestination::Orphan,
                None,
                Some("line not in any section range".to_string()),
            );
        }
    }

    // Use section titles as proposed document titles
    let titles: Vec<String> = sections.iter().map(|s| s.title.clone()).collect();

    // Build proposed documents from sections and assigned facts
    let new_documents = build_proposed_documents(sections, &titles, &ledger, &facts);

    Ok(SplitPlan {
        source_id: doc_id.to_string(),
        new_documents,
        ledger,
        temporal_issues: Vec::new(),
    })
}

/// Build proposed documents from sections and assigned facts.
fn build_proposed_documents(
    sections: &[SplitSection],
    titles: &[String],
    ledger: &FactLedger,
    facts: &[TrackedFact],
) -> Vec<ProposedDocument> {
    let mut documents = Vec::new();

    for (i, section) in sections.iter().enumerate() {
        let title = titles
            .get(i)
            .cloned()
            .unwrap_or_else(|| section.title.clone());
        let target_id = format!("section_{i}");

        // Get facts assigned to this section
        let section_facts: Vec<&TrackedFact> = facts
            .iter()
            .filter(|f| {
                ledger.assignments.get(&f.id).is_some_and(|a| {
                    a.destination == FactDestination::Document
                        && a.target_doc.as_deref() == Some(&target_id)
                })
            })
            .collect();

        // Build content from section content plus any additional facts
        let mut content = format!("# {title}\n\n");

        // Add facts as list items
        for fact in section_facts {
            if fact.content.starts_with('-') || fact.content.starts_with('*') {
                content.push_str(&fact.content);
            } else {
                content.push_str("- ");
                content.push_str(&fact.content);
            }
            content.push('\n');
        }

        documents.push(ProposedDocument {
            title,
            section_title: section.title.clone(),
            content,
        });
    }

    documents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_plan_is_valid() {
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "test fact", None, vec![]);
        let fact_id = fact.id.clone();
        ledger.add_fact(fact);
        ledger.assign(
            &fact_id,
            FactDestination::Document,
            Some("section_0".to_string()),
            None,
        );

        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![ProposedDocument {
                title: "New Doc".to_string(),
                section_title: "Section".to_string(),
                content: "# New Doc\n\n- test fact\n".to_string(),
            }],
            ledger,
            temporal_issues: vec![],
        };

        assert!(plan.is_valid());
    }

    #[test]
    fn test_split_plan_orphan_count() {
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "fact 2", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("section_0".to_string()),
            None,
        );
        ledger.assign(&id2, FactDestination::Orphan, None, None);

        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![],
            ledger,
            temporal_issues: vec![],
        };

        assert_eq!(plan.orphan_count(), 1);
    }

    #[test]
    fn test_split_plan_document_count() {
        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![
                ProposedDocument {
                    title: "Doc 1".to_string(),
                    section_title: "Section 1".to_string(),
                    content: "content".to_string(),
                },
                ProposedDocument {
                    title: "Doc 2".to_string(),
                    section_title: "Section 2".to_string(),
                    content: "content".to_string(),
                },
            ],
            ledger: FactLedger::new(),
            temporal_issues: vec![],
        };

        assert_eq!(plan.document_count(), 2);
    }
}
