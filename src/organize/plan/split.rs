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

/// Type alias for split response parsing result: (assignments, titles, temporal_issues)
/// Each assignment is (fact_id, section_index, section_name)
type SplitParseResult = (Vec<(String, usize, String)>, Vec<String>, Vec<TemporalIssue>);

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

/// Build the LLM prompt for split analysis (no longer used).
fn build_split_prompt(_doc_title: &str, _facts: &[TrackedFact], _sections: &[SplitSection]) -> String {
    String::new()
}

/// Parse the LLM response into fact assignments and proposed titles.
fn parse_split_response(
    response: &str,
    facts: &[TrackedFact],
    sections: &[SplitSection],
) -> Result<SplitParseResult, FactbaseError> {
    let mut assignments = Vec::new();
    let mut titles: Vec<String> = sections.iter().map(|s| s.title.clone()).collect();
    let mut temporal_issues = Vec::new();

    // Try to parse JSON from response
    let json_start = response.find('{');
    let json_end = response.rfind('}');

    if let (Some(start), Some(end)) = (json_start, json_end) {
        let json_str = &response[start..=end];
        if let Ok(parsed) = serde_json::from_str::<SplitResponse>(json_str) {
            // Process assignments
            for assignment in parsed.assignments {
                if let Some(fact) = find_fact_by_ref(&assignment.fact, facts) {
                    let section_idx = parse_section_ref(&assignment.section, sections.len());
                    assignments.push((fact.id.clone(), section_idx, assignment.reason));
                }
            }

            // Process titles
            for title_entry in parsed.titles {
                if let Some(idx) = parse_section_index(&title_entry.section) {
                    if idx < titles.len() {
                        titles[idx] = title_entry.title;
                    }
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

    // Any unassigned facts default to orphan (safety)
    for fact in facts {
        if !assignments.iter().any(|(id, _, _)| id == &fact.id) {
            assignments.push((
                fact.id.clone(),
                usize::MAX, // Orphan marker
                "LLM did not assign this fact".to_string(),
            ));
        }
    }

    Ok((assignments, titles, temporal_issues))
}

/// Find a fact by its reference string (e.g., "F0").
fn find_fact_by_ref<'a>(reference: &str, facts: &'a [TrackedFact]) -> Option<&'a TrackedFact> {
    if !reference.starts_with('F') {
        return None;
    }
    let index_str = &reference[1..];
    if let Ok(index) = index_str.parse::<usize>() {
        facts.get(index)
    } else {
        None
    }
}

/// Parse a section reference (e.g., "S0" or "ORPHAN") into an index.
fn parse_section_ref(reference: &str, _section_count: usize) -> usize {
    if reference.eq_ignore_ascii_case("ORPHAN") {
        return usize::MAX;
    }
    parse_section_index(reference).unwrap_or(usize::MAX)
}

/// Parse a section index from a reference string (e.g., "S0" -> 0).
fn parse_section_index(reference: &str) -> Option<usize> {
    if !reference.starts_with('S') {
        return None;
    }
    reference[1..].parse().ok()
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

#[derive(Deserialize)]
struct SplitResponse {
    assignments: Vec<SplitAssignment>,
    #[serde(default)]
    titles: Vec<SplitTitle>,
    #[serde(default)]
    temporal_issues: Vec<RawTemporalIssue>,
}

#[derive(Deserialize)]
struct SplitAssignment {
    fact: String,
    section: String,
    reason: String,
}

#[derive(Deserialize)]
struct SplitTitle {
    section: String,
    title: String,
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

    #[test]
    fn test_find_fact_by_ref() {
        let facts = vec![
            TrackedFact::new("doc1", 1, "fact 0", None, vec![]),
            TrackedFact::new("doc1", 2, "fact 1", None, vec![]),
        ];

        let found = find_fact_by_ref("F0", &facts);
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "fact 0");

        let found = find_fact_by_ref("F1", &facts);
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "fact 1");

        assert!(find_fact_by_ref("F5", &facts).is_none());
        assert!(find_fact_by_ref("S0", &facts).is_none());
    }

    #[test]
    fn test_parse_section_ref() {
        assert_eq!(parse_section_ref("S0", 3), 0);
        assert_eq!(parse_section_ref("S2", 3), 2);
        assert_eq!(parse_section_ref("ORPHAN", 3), usize::MAX);
        assert_eq!(parse_section_ref("orphan", 3), usize::MAX);
        assert_eq!(parse_section_ref("invalid", 3), usize::MAX);
    }

    #[test]
    fn test_parse_section_index() {
        assert_eq!(parse_section_index("S0"), Some(0));
        assert_eq!(parse_section_index("S5"), Some(5));
        assert_eq!(parse_section_index("F0"), None);
        assert_eq!(parse_section_index("invalid"), None);
    }

    #[test]
    fn test_build_split_prompt() {
        let facts = vec![TrackedFact::new("doc1", 5, "- CTO at Acme", None, vec![])];
        let sections = vec![
            SplitSection {
                title: "Career".to_string(),
                level: 2,
                start_line: 3,
                end_line: 10,
                content: "Career content".to_string(),
            },
        ];

        let prompt = build_split_prompt("Person Name", &facts, &sections);
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_parse_split_response_valid_json() {
        let facts = vec![
            TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![]),
            TrackedFact::new("doc1", 2, "- PhD from MIT", None, vec![]),
        ];
        let sections = vec![
            SplitSection {
                title: "Career".to_string(),
                level: 2,
                start_line: 1,
                end_line: 5,
                content: "content".to_string(),
            },
            SplitSection {
                title: "Education".to_string(),
                level: 2,
                start_line: 6,
                end_line: 10,
                content: "content".to_string(),
            },
        ];

        let response = r#"{
            "assignments": [
                {"fact": "F0", "section": "S0", "reason": "career fact"},
                {"fact": "F1", "section": "S1", "reason": "education fact"}
            ],
            "titles": [
                {"section": "S0", "title": "Career History"},
                {"section": "S1", "title": "Academic Background"}
            ]
        }"#;

        let (assignments, titles, temporal_issues) =
            parse_split_response(response, &facts, &sections).unwrap();

        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].1, 0); // Section 0
        assert_eq!(assignments[1].1, 1); // Section 1
        assert_eq!(titles[0], "Career History");
        assert_eq!(titles[1], "Academic Background");
        assert!(temporal_issues.is_empty());
    }

    #[test]
    fn test_parse_split_response_orphan() {
        let facts = vec![TrackedFact::new("doc1", 1, "- Random fact", None, vec![])];
        let sections = vec![SplitSection {
            title: "Career".to_string(),
            level: 2,
            start_line: 1,
            end_line: 5,
            content: "content".to_string(),
        }];

        let response = r#"{
            "assignments": [
                {"fact": "F0", "section": "ORPHAN", "reason": "doesn't fit"}
            ],
            "titles": []
        }"#;

        let (assignments, _, _) = parse_split_response(response, &facts, &sections).unwrap();

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].1, usize::MAX); // Orphan
    }

    #[test]
    fn test_parse_split_response_unassigned_defaults_to_orphan() {
        let facts = vec![TrackedFact::new(
            "doc1",
            1,
            "- Unassigned fact",
            None,
            vec![],
        )];
        let sections = vec![SplitSection {
            title: "Career".to_string(),
            level: 2,
            start_line: 1,
            end_line: 5,
            content: "content".to_string(),
        }];

        // Empty assignments - LLM didn't assign the fact
        let response = r#"{"assignments": [], "titles": []}"#;

        let (assignments, _, _) = parse_split_response(response, &facts, &sections).unwrap();

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].1, usize::MAX); // Defaults to orphan
    }

    #[test]
    fn test_build_proposed_documents() {
        let sections = vec![SplitSection {
            title: "Career".to_string(),
            level: 2,
            start_line: 1,
            end_line: 5,
            content: "content".to_string(),
        }];
        let titles = vec!["Career History".to_string()];

        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 3, "- CTO at Acme", None, vec![]);
        let fact_id = fact.id.clone();
        ledger.add_fact(fact.clone());
        ledger.assign(
            &fact_id,
            FactDestination::Document,
            Some("section_0".to_string()),
            None,
        );

        let facts = vec![fact];
        let docs = build_proposed_documents(&sections, &titles, &ledger, &facts);

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Career History");
        assert_eq!(docs[0].section_title, "Career");
        assert!(docs[0].content.contains("# Career History"));
        assert!(docs[0].content.contains("- CTO at Acme"));
    }

    #[test]
    fn test_parse_split_response_with_temporal_issues() {
        let facts = vec![TrackedFact::new("doc1", 1, "- Fact A", None, vec![])];
        let sections = vec![SplitSection {
            title: "Section".to_string(),
            level: 2,
            start_line: 1,
            end_line: 5,
            content: "content".to_string(),
        }];

        let response = r#"{
            "assignments": [
                {"fact": "F0", "section": "S0", "reason": "fits section"}
            ],
            "titles": [],
            "temporal_issues": [
                {"line_ref": 3, "description": "Date range ends before it starts"},
                {"line_ref": 7, "description": "Missing date makes sequence unclear"}
            ]
        }"#;

        let (_, _, temporal_issues) =
            parse_split_response(response, &facts, &sections).unwrap();

        assert_eq!(temporal_issues.len(), 2);
        assert_eq!(temporal_issues[0].line_ref, 3);
        assert_eq!(
            temporal_issues[0].description,
            "Date range ends before it starts"
        );
        assert_eq!(temporal_issues[1].line_ref, 7);
    }

    #[test]
    fn test_parse_split_response_no_temporal_issues_field() {
        let facts = vec![TrackedFact::new("doc1", 1, "- Fact A", None, vec![])];
        let sections = vec![SplitSection {
            title: "Section".to_string(),
            level: 2,
            start_line: 1,
            end_line: 5,
            content: "content".to_string(),
        }];

        // Response without temporal_issues field at all
        let response = r#"{
            "assignments": [{"fact": "F0", "section": "S0", "reason": "fits"}],
            "titles": []
        }"#;

        let (_, _, temporal_issues) =
            parse_split_response(response, &facts, &sections).unwrap();

        assert!(temporal_issues.is_empty());
    }

    #[test]
    fn test_parse_split_response_history_domain() {
        let facts = vec![
            TrackedFact::new("doc1", 1, "- Battle began @t[=480 BCE]", None, vec![]),
            TrackedFact::new("doc1", 2, "- Greek forces numbered 7000", None, vec![]),
        ];
        let sections = vec![
            SplitSection {
                title: "Timeline".to_string(),
                level: 2,
                start_line: 1,
                end_line: 5,
                content: "content".to_string(),
            },
        ];

        let response = r#"{
            "assignments": [
                {"fact": "F0", "section": "S0", "reason": "temporal event"},
                {"fact": "F1", "section": "S0", "reason": "battle detail"}
            ],
            "titles": [{"section": "S0", "title": "Battle of Thermopylae Timeline"}],
            "temporal_issues": [{"line_ref": 1, "description": "BCE date may need verification"}]
        }"#;

        let (assignments, titles, temporal_issues) =
            parse_split_response(response, &facts, &sections).unwrap();

        assert_eq!(assignments.len(), 2);
        assert_eq!(titles[0], "Battle of Thermopylae Timeline");
        assert_eq!(temporal_issues.len(), 1);
    }
}
