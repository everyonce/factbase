//! Merge planning for document reorganization.
//!
//! Creates a plan for merging multiple documents into one, with fact-level
//! accounting to ensure no data is lost.

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
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
/// Extracts facts from all source documents and uses the LLM to determine
/// which facts to keep, which are duplicates, and which should be orphaned.
///
/// # Arguments
/// * `keep_id` - ID of the document to keep (target)
/// * `merge_ids` - IDs of documents to merge into the target
/// * `db` - Database connection
/// * `llm` - LLM provider for fact analysis
///
/// # Returns
/// A `MergePlan` with fact assignments and combined content.
pub async fn plan_merge(
    keep_id: &str,
    merge_ids: &[&str],
    db: &Database,
    llm: &dyn LlmProvider,
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

    // Build prompt for LLM to analyze facts
    let prompt = build_merge_prompt(&keep_doc.title, &keep_facts, &all_merge_facts);
    let response = llm.complete(&prompt).await?;

    // Parse LLM response and assign facts
    let (assignments, temporal_issues) =
        parse_merge_response(&response, &keep_facts, &all_merge_facts)?;

    // Apply assignments to ledger
    for (fact_id, destination, reason) in assignments {
        let target_doc = if destination == FactDestination::Document {
            Some(keep_id.to_string())
        } else {
            None
        };
        ledger.assign(&fact_id, destination, target_doc, Some(reason));
    }

    // Build combined content from kept facts
    let combined_content = build_combined_content(&keep_doc, &ledger);

    Ok(MergePlan {
        keep_id: keep_id.to_string(),
        merge_ids: merge_ids.iter().map(ToString::to_string).collect(),
        ledger,
        combined_content,
        temporal_issues,
    })
}

/// Default template for the organize merge prompt.
const DEFAULT_ORGANIZE_MERGE_PROMPT: &str = r#"You are analyzing facts from multiple documents to merge them into one.

TARGET DOCUMENT: "{doc_title}"

FACTS FROM TARGET (to keep):
{keep_facts}
FACTS FROM DOCUMENTS TO MERGE:
{merge_facts}
For each fact from the merge documents, decide:
- KEEP: Add to target (new information)
- DUPLICATE: Same as a fact in target (specify which K# it duplicates)
- ORPHAN: Doesn't fit the target document's topic

TEMPORAL AUDIT: Also identify any timeline or timing inconsistencies across all documents that could affect merge decisions. Flag:
- Contradictory dates for the same event across documents
- Overlapping date ranges that conflict when combined
- Missing dates that make the combined timeline unclear
- Any temporal feature that could be an issue when merging

Respond in JSON format:
{
  "decisions": [
    {"fact": "M<doc_id>_<index>", "action": "KEEP|DUPLICATE|ORPHAN", "reason": "brief explanation", "duplicates": "K#" (if DUPLICATE)}
  ],
  "temporal_issues": [
    {"line_ref": 5, "description": "description of the temporal issue"}
  ]
}

Only include facts from merge documents (M*), not target facts (K*).
Return an empty temporal_issues array if no issues are found.
"#;

/// Build the LLM prompt for merge analysis.
fn build_merge_prompt(
    keep_title: &str,
    keep_facts: &[TrackedFact],
    merge_facts: &[(String, Vec<TrackedFact>)],
) -> String {
    let mut keep_str = String::new();
    for (i, fact) in keep_facts.iter().enumerate() {
        writeln_str!(keep_str, "K{}: {}", i, fact.content);
    }

    let mut merge_str = String::new();
    for (doc_id, facts) in merge_facts {
        writeln_str!(merge_str, "\nFrom document {}:", doc_id);
        for (i, fact) in facts.iter().enumerate() {
            writeln_str!(merge_str, "M{}_{}: {}", doc_id, i, fact.content);
        }
    }

    let prompts = crate::Config::load(None).unwrap_or_default().prompts;
    crate::config::prompts::resolve_prompt(
        &prompts,
        "organize_merge",
        DEFAULT_ORGANIZE_MERGE_PROMPT,
        &[
            ("doc_title", keep_title),
            ("keep_facts", &keep_str),
            ("merge_facts", &merge_str),
        ],
    )
}

/// Parse the LLM response into fact assignments.
fn parse_merge_response(
    response: &str,
    keep_facts: &[TrackedFact],
    merge_facts: &[(String, Vec<TrackedFact>)],
) -> Result<(Vec<(String, FactDestination, String)>, Vec<TemporalIssue>), FactbaseError> {
    // All keep_facts automatically go to Document destination
    let mut assignments: Vec<(String, FactDestination, String)> = keep_facts
        .iter()
        .map(|f| {
            (
                f.id.clone(),
                FactDestination::Document,
                "target document fact".to_string(),
            )
        })
        .collect();

    let mut temporal_issues = Vec::new();

    // Try to parse JSON from response
    let json_start = response.find('{');
    let json_end = response.rfind('}');

    if let (Some(start), Some(end)) = (json_start, json_end) {
        let json_str = &response[start..=end];
        if let Ok(parsed) = serde_json::from_str::<MergeResponse>(json_str) {
            for decision in parsed.decisions {
                // Parse fact reference like "Mabc123_0"
                if let Some(fact) = find_fact_by_ref(&decision.fact, merge_facts) {
                    let destination = match decision.action.to_uppercase().as_str() {
                        "KEEP" => FactDestination::Document,
                        "DUPLICATE" => FactDestination::Duplicate,
                        // "ORPHAN" and any unrecognized action default to orphan for safety
                        _ => FactDestination::Orphan,
                    };
                    assignments.push((fact.id.clone(), destination, decision.reason));
                }
            }

            // Process temporal issues
            for issue in parsed.temporal_issues {
                temporal_issues.push(TemporalIssue {
                    line_ref: issue.line_ref,
                    description: issue.description,
                });
            }
        }
    }

    // Any unassigned merge facts default to orphan (safety)
    for (_, facts) in merge_facts {
        for fact in facts {
            if !assignments.iter().any(|(id, _, _)| id == &fact.id) {
                assignments.push((
                    fact.id.clone(),
                    FactDestination::Orphan,
                    "LLM did not assign this fact".to_string(),
                ));
            }
        }
    }

    Ok((assignments, temporal_issues))
}

/// Find a fact by its reference string (e.g., "Mabc123_0").
fn find_fact_by_ref<'a>(
    reference: &str,
    merge_facts: &'a [(String, Vec<TrackedFact>)],
) -> Option<&'a TrackedFact> {
    // Parse "M<doc_id>_<index>"
    if !reference.starts_with('M') {
        return None;
    }

    let rest = &reference[1..];
    if let Some(underscore_pos) = rest.rfind('_') {
        let doc_id = &rest[..underscore_pos];
        let index_str = &rest[underscore_pos + 1..];
        if let Ok(index) = index_str.parse::<usize>() {
            for (id, facts) in merge_facts {
                if id == doc_id && index < facts.len() {
                    return Some(&facts[index]);
                }
            }
        }
    }

    None
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

#[derive(Deserialize)]
struct MergeResponse {
    decisions: Vec<MergeDecision>,
    #[serde(default)]
    temporal_issues: Vec<RawTemporalIssue>,
}

#[derive(Deserialize)]
struct MergeDecision {
    fact: String,
    action: String,
    reason: String,
    #[serde(default)]
    #[allow(dead_code)] // Used for context in duplicate decisions
    duplicates: Option<String>,
}

#[derive(Deserialize)]
struct RawTemporalIssue {
    line_ref: usize,
    description: String,
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

    #[test]
    fn test_find_fact_by_ref() {
        let facts = vec![(
            "abc123".to_string(),
            vec![
                TrackedFact::new("abc123", 1, "fact 0", None, vec![]),
                TrackedFact::new("abc123", 2, "fact 1", None, vec![]),
            ],
        )];

        let found = find_fact_by_ref("Mabc123_0", &facts);
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "fact 0");

        let found = find_fact_by_ref("Mabc123_1", &facts);
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "fact 1");

        let not_found = find_fact_by_ref("Mabc123_5", &facts);
        assert!(not_found.is_none());

        let invalid = find_fact_by_ref("Kabc123_0", &facts);
        assert!(invalid.is_none());
    }

    #[test]
    fn test_build_merge_prompt() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- VP at BigCo", None, vec![])],
        )];

        let prompt = build_merge_prompt("Person Name", &keep_facts, &merge_facts);

        assert!(prompt.contains("TARGET DOCUMENT: \"Person Name\""));
        assert!(prompt.contains("K0: - CTO at Acme"));
        assert!(prompt.contains("M0: - VP at BigCo") || prompt.contains("Mdoc2_0: - VP at BigCo"));
    }

    #[test]
    fn test_parse_merge_response_valid_json() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- VP at BigCo", None, vec![])],
        )];

        let response =
            r#"{"decisions": [{"fact": "Mdoc2_0", "action": "KEEP", "reason": "new info"}]}"#;

        let (assignments, temporal_issues) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        // Should have 2 assignments: 1 for keep_fact, 1 for merge_fact
        assert_eq!(assignments.len(), 2);

        // First is the keep_fact (auto-assigned to Document)
        assert_eq!(assignments[0].1, FactDestination::Document);

        // Second is the merge_fact (KEEP -> Document)
        assert_eq!(assignments[1].1, FactDestination::Document);
        assert!(temporal_issues.is_empty());
    }

    #[test]
    fn test_parse_merge_response_duplicate() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- CTO at Acme", None, vec![])],
        )];

        let response = r#"{"decisions": [{"fact": "Mdoc2_0", "action": "DUPLICATE", "reason": "same as K0", "duplicates": "K0"}]}"#;

        let (assignments, _) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        // Merge fact should be marked as duplicate
        let merge_assignment = assignments
            .iter()
            .find(|(id, _, _)| id == &merge_facts[0].1[0].id);
        assert!(merge_assignment.is_some());
        assert_eq!(merge_assignment.unwrap().1, FactDestination::Duplicate);
    }

    #[test]
    fn test_parse_merge_response_unassigned_defaults_to_orphan() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- VP at BigCo", None, vec![])],
        )];

        // Empty decisions - LLM didn't assign the merge fact
        let response = r#"{"decisions": []}"#;

        let (assignments, _) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        // Merge fact should default to orphan
        let merge_assignment = assignments
            .iter()
            .find(|(id, _, _)| id == &merge_facts[0].1[0].id);
        assert!(merge_assignment.is_some());
        assert_eq!(merge_assignment.unwrap().1, FactDestination::Orphan);
    }

    #[test]
    fn test_parse_merge_response_with_temporal_issues() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- Fact A", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- Fact B", None, vec![])],
        )];

        let response = r#"{
            "decisions": [{"fact": "Mdoc2_0", "action": "KEEP", "reason": "new info"}],
            "temporal_issues": [
                {"line_ref": 2, "description": "Contradictory dates across documents"},
                {"line_ref": 5, "description": "Overlapping date ranges conflict when combined"}
            ]
        }"#;

        let (_, temporal_issues) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        assert_eq!(temporal_issues.len(), 2);
        assert_eq!(temporal_issues[0].line_ref, 2);
        assert_eq!(
            temporal_issues[0].description,
            "Contradictory dates across documents"
        );
        assert_eq!(temporal_issues[1].line_ref, 5);
    }

    #[test]
    fn test_parse_merge_response_no_temporal_issues_field() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- Fact A", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- Fact B", None, vec![])],
        )];

        // Response without temporal_issues field
        let response =
            r#"{"decisions": [{"fact": "Mdoc2_0", "action": "KEEP", "reason": "new info"}]}"#;

        let (_, temporal_issues) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        assert!(temporal_issues.is_empty());
    }

    #[test]
    fn test_merge_prompt_is_domain_agnostic() {
        let prompt = DEFAULT_ORGANIZE_MERGE_PROMPT;
        for term in &["employee", "company", "person", "promotion", "career", "hired", "job", "role", "staff"] {
            assert!(!prompt.to_lowercase().contains(term),
                "Merge prompt should not contain domain-specific term: {term}");
        }
    }

    #[test]
    fn test_build_merge_prompt_botany_domain() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- Native to North America", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- Fruiting season: autumn", None, vec![])],
        )];

        let prompt = build_merge_prompt("Chanterelle", &keep_facts, &merge_facts);

        assert!(prompt.contains("TARGET DOCUMENT: \"Chanterelle\""));
        assert!(prompt.contains("Native to North America"));
        assert!(prompt.contains("Fruiting season: autumn"));
    }

    #[test]
    fn test_parse_merge_response_history_domain() {
        let keep_facts = vec![TrackedFact::new("doc1", 1, "- Founded 753 BCE", None, vec![])];
        let merge_facts = vec![(
            "doc2".to_string(),
            vec![TrackedFact::new("doc2", 1, "- Republic established 509 BCE", None, vec![])],
        )];

        let response = r#"{
            "decisions": [{"fact": "Mdoc2_0", "action": "KEEP", "reason": "new temporal info"}],
            "temporal_issues": [{"line_ref": 1, "description": "BCE dates span centuries"}]
        }"#;

        let (assignments, temporal_issues) =
            parse_merge_response(response, &keep_facts, &merge_facts).unwrap();

        assert_eq!(assignments.len(), 2);
        assert_eq!(temporal_issues.len(), 1);
    }
}
