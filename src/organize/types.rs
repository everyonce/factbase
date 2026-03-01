//! Types for fact extraction and reorganization tracking.
//!
//! These types enable fact-level accounting during reorganization operations
//! to ensure no data is silently lost.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt;

/// A candidate pair of documents that could be merged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCandidate {
    /// First document ID
    pub doc1_id: String,
    /// First document title
    pub doc1_title: String,
    /// Second document ID
    pub doc2_id: String,
    /// Second document title
    pub doc2_title: String,
    /// Similarity score (0.0 to 1.0)
    pub similarity: f32,
    /// Suggested document to keep (usually the one with more content/links)
    pub suggested_keep: String,
    /// Rationale for the merge suggestion
    pub rationale: String,
}

/// A section of a document identified by header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SplitSection {
    /// Section title (from header, or "Introduction" for content before first header)
    pub title: String,
    /// Header level (1-6, or 0 for intro section)
    pub level: u8,
    /// Start line (1-indexed, inclusive)
    pub start_line: usize,
    /// End line (1-indexed, inclusive)
    pub end_line: usize,
    /// Section content (excluding the header line itself)
    pub content: String,
}

/// A document that is a candidate for splitting into multiple documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitCandidate {
    /// Document ID
    pub doc_id: String,
    /// Document title
    pub doc_title: String,
    /// Sections identified in the document
    pub sections: Vec<SplitSection>,
    /// Average similarity between sections (lower = more distinct topics)
    pub avg_similarity: f32,
    /// Minimum similarity between any two sections
    pub min_similarity: f32,
    /// Rationale for the split suggestion
    pub rationale: String,
}

/// A document that may be in the wrong folder based on content analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MisplacedCandidate {
    /// Document ID
    pub doc_id: String,
    /// Document title
    pub doc_title: String,
    /// Current type (derived from folder)
    pub current_type: String,
    /// Suggested type based on content similarity
    pub suggested_type: String,
    /// Confidence score (difference in similarity to suggested vs current type)
    pub confidence: f32,
    /// Rationale for the suggestion
    pub rationale: String,
}

/// A discrete fact extracted from a document for tracking through reorganization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackedFact {
    /// Unique identifier for this fact (hash of content + source)
    pub id: String,
    /// Source document ID
    pub source_doc: String,
    /// Line number in source document (1-indexed)
    pub source_line: usize,
    /// The fact content (trimmed)
    pub content: String,
    /// Temporal tag if present (raw text, e.g., "@t\[2020..2022\]")
    pub temporal: Option<String>,
    /// Footnote references (e.g., \["1", "2"\] for \[^1\], \[^2\])
    pub sources: Vec<String>,
}

impl TrackedFact {
    /// Create a new tracked fact with a generated ID.
    pub fn new(
        source_doc: &str,
        source_line: usize,
        content: &str,
        temporal: Option<String>,
        sources: Vec<String>,
    ) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        source_doc.hash(&mut hasher);
        source_line.hash(&mut hasher);
        content.hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());

        Self {
            id,
            source_doc: source_doc.to_string(),
            source_line,
            content: content.trim().to_string(),
            temporal,
            sources,
        }
    }
}

/// Where a fact ends up after reorganization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactDestination {
    /// Assigned to a specific document
    Document,
    /// Moved to orphan holding document
    Orphan,
    /// Identified as duplicate of another fact
    Duplicate,
    /// Explicitly deleted by user
    Deleted,
}

impl fmt::Display for FactDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FactDestination::Document => write!(f, "document"),
            FactDestination::Orphan => write!(f, "orphan"),
            FactDestination::Duplicate => write!(f, "duplicate"),
            FactDestination::Deleted => write!(f, "deleted"),
        }
    }
}

/// Assignment of a fact to its destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactAssignment {
    /// The fact being assigned
    pub fact_id: String,
    /// Where the fact goes
    pub destination: FactDestination,
    /// Target document ID (if destination is Document)
    pub target_doc: Option<String>,
    /// Reason for this assignment
    pub reason: Option<String>,
}

/// Location of an entity entry within a specific document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryLocation {
    /// Parent document ID.
    pub doc_id: String,
    /// Parent document title.
    pub doc_title: String,
    /// Section containing the entry (e.g., "Team").
    pub section: String,
    /// Start line (1-indexed, inclusive).
    pub line_start: usize,
    /// Child list items (fact text).
    pub facts: Vec<String>,
}

/// A named entity that appears as an entry in multiple documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateEntry {
    /// Canonical entity name.
    pub entity_name: String,
    /// All locations where this entity appears.
    pub entries: Vec<EntryLocation>,
}

/// Two files in the same directory that share a factbase document ID or title.
///
/// This happens when organize/merge creates one copy but the other persists on
/// disk. The database only tracks one file per doc ID, so the second file is a
/// "ghost" that accumulates stale content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostFile {
    /// The shared factbase document ID (if both files have the same header).
    pub doc_id: String,
    /// Document title.
    pub title: String,
    /// File path the database tracks (relative to repo root).
    pub tracked_path: String,
    /// File path of the ghost (relative to repo root).
    pub ghost_path: String,
    /// Line count of the tracked file.
    pub tracked_lines: usize,
    /// Line count of the ghost file.
    pub ghost_lines: usize,
    /// Why these were flagged: "same_id" or "same_title".
    pub reason: String,
}

/// Ledger tracking all facts through a reorganization operation.
///
/// The key invariant is: `source_facts.len() == assignments.len()`
/// Every source fact must have exactly one assignment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactLedger {
    /// All facts extracted from source documents
    pub source_facts: Vec<TrackedFact>,
    /// Assignments for each fact (keyed by fact ID)
    pub assignments: HashMap<String, FactAssignment>,
}

impl FactLedger {
    /// Create a new empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source fact to the ledger.
    pub fn add_fact(&mut self, fact: TrackedFact) {
        self.source_facts.push(fact);
    }

    /// Assign a fact to a destination.
    pub fn assign(
        &mut self,
        fact_id: &str,
        destination: FactDestination,
        target_doc: Option<String>,
        reason: Option<String>,
    ) {
        self.assignments.insert(
            fact_id.to_string(),
            FactAssignment {
                fact_id: fact_id.to_string(),
                destination,
                target_doc,
                reason,
            },
        );
    }

    /// Check if all facts have been assigned (books balance).
    pub fn is_balanced(&self) -> bool {
        self.source_facts
            .iter()
            .all(|f| self.assignments.contains_key(&f.id))
    }

    /// Get facts that haven't been assigned yet.
    pub fn unaccounted_facts(&self) -> Vec<&TrackedFact> {
        self.source_facts
            .iter()
            .filter(|f| !self.assignments.contains_key(&f.id))
            .collect()
    }

    /// Count facts assigned to orphan destination.
    pub fn orphan_count(&self) -> usize {
        self.assignments
            .values()
            .filter(|a| a.destination == FactDestination::Orphan)
            .count()
    }

    /// Count facts assigned to each destination type.
    pub fn destination_counts(&self) -> HashMap<FactDestination, usize> {
        let mut counts = HashMap::new();
        for assignment in self.assignments.values() {
            *counts.entry(assignment.destination).or_insert(0) += 1;
        }
        counts
    }

    /// Get all facts assigned to a specific document.
    pub fn facts_for_document(&self, doc_id: &str) -> Vec<&TrackedFact> {
        self.source_facts
            .iter()
            .filter(|f| {
                self.assignments.get(&f.id).is_some_and(|a| {
                    a.destination == FactDestination::Document
                        && a.target_doc.as_deref() == Some(doc_id)
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_fact_new() {
        let fact = TrackedFact::new(
            "abc123",
            5,
            "  CTO at Acme Corp  ",
            Some("@t[2020..2022]".to_string()),
            vec!["1".to_string()],
        );

        assert!(!fact.id.is_empty());
        assert_eq!(fact.source_doc, "abc123");
        assert_eq!(fact.source_line, 5);
        assert_eq!(fact.content, "CTO at Acme Corp"); // trimmed
        assert_eq!(fact.temporal, Some("@t[2020..2022]".to_string()));
        assert_eq!(fact.sources, vec!["1".to_string()]);
    }

    #[test]
    fn test_tracked_fact_unique_ids() {
        let fact1 = TrackedFact::new("doc1", 1, "content", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "content", None, vec![]);
        let fact3 = TrackedFact::new("doc2", 1, "content", None, vec![]);

        // Different line numbers or docs should produce different IDs
        assert_ne!(fact1.id, fact2.id);
        assert_ne!(fact1.id, fact3.id);
    }

    #[test]
    fn test_fact_destination_display() {
        assert_eq!(FactDestination::Document.to_string(), "document");
        assert_eq!(FactDestination::Orphan.to_string(), "orphan");
        assert_eq!(FactDestination::Duplicate.to_string(), "duplicate");
        assert_eq!(FactDestination::Deleted.to_string(), "deleted");
    }

    #[test]
    fn test_ledger_empty_is_balanced() {
        let ledger = FactLedger::new();
        assert!(ledger.is_balanced());
        assert!(ledger.unaccounted_facts().is_empty());
        assert_eq!(ledger.orphan_count(), 0);
    }

    #[test]
    fn test_ledger_unbalanced() {
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "fact content", None, vec![]);
        ledger.add_fact(fact);

        assert!(!ledger.is_balanced());
        assert_eq!(ledger.unaccounted_facts().len(), 1);
    }

    #[test]
    fn test_ledger_balanced_after_assignment() {
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "fact content", None, vec![]);
        let fact_id = fact.id.clone();
        ledger.add_fact(fact);

        ledger.assign(
            &fact_id,
            FactDestination::Document,
            Some("doc2".to_string()),
            Some("merged".to_string()),
        );

        assert!(ledger.is_balanced());
        assert!(ledger.unaccounted_facts().is_empty());
    }

    #[test]
    fn test_ledger_orphan_count() {
        let mut ledger = FactLedger::new();

        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "fact 2", None, vec![]);
        let fact3 = TrackedFact::new("doc1", 3, "fact 3", None, vec![]);

        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        let id3 = fact3.id.clone();

        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.add_fact(fact3);

        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc2".to_string()),
            None,
        );
        ledger.assign(&id2, FactDestination::Orphan, None, None);
        ledger.assign(&id3, FactDestination::Orphan, None, None);

        assert!(ledger.is_balanced());
        assert_eq!(ledger.orphan_count(), 2);
    }

    #[test]
    fn test_ledger_destination_counts() {
        let mut ledger = FactLedger::new();

        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "fact 2", None, vec![]);
        let fact3 = TrackedFact::new("doc1", 3, "fact 3", None, vec![]);
        let fact4 = TrackedFact::new("doc1", 4, "fact 4", None, vec![]);

        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        let id3 = fact3.id.clone();
        let id4 = fact4.id.clone();

        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.add_fact(fact3);
        ledger.add_fact(fact4);

        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc2".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("doc2".to_string()),
            None,
        );
        ledger.assign(&id3, FactDestination::Orphan, None, None);
        ledger.assign(&id4, FactDestination::Duplicate, None, None);

        let counts = ledger.destination_counts();
        assert_eq!(counts.get(&FactDestination::Document), Some(&2));
        assert_eq!(counts.get(&FactDestination::Orphan), Some(&1));
        assert_eq!(counts.get(&FactDestination::Duplicate), Some(&1));
        assert_eq!(counts.get(&FactDestination::Deleted), None);
    }

    #[test]
    fn test_ledger_facts_for_document() {
        let mut ledger = FactLedger::new();

        let fact1 = TrackedFact::new("doc1", 1, "fact 1", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "fact 2", None, vec![]);
        let fact3 = TrackedFact::new("doc1", 3, "fact 3", None, vec![]);

        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        let id3 = fact3.id.clone();

        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.add_fact(fact3);

        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("target1".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("target2".to_string()),
            None,
        );
        ledger.assign(
            &id3,
            FactDestination::Document,
            Some("target1".to_string()),
            None,
        );

        let target1_facts = ledger.facts_for_document("target1");
        assert_eq!(target1_facts.len(), 2);

        let target2_facts = ledger.facts_for_document("target2");
        assert_eq!(target2_facts.len(), 1);

        let unknown_facts = ledger.facts_for_document("unknown");
        assert!(unknown_facts.is_empty());
    }

    #[test]
    fn test_split_section_struct() {
        let section = SplitSection {
            title: "Career".to_string(),
            level: 2,
            start_line: 5,
            end_line: 10,
            content: "- CTO at Acme\n- VP at BigCo".to_string(),
        };

        assert_eq!(section.title, "Career");
        assert_eq!(section.level, 2);
        assert_eq!(section.start_line, 5);
        assert_eq!(section.end_line, 10);
    }

    #[test]
    fn test_split_candidate_struct() {
        let candidate = SplitCandidate {
            doc_id: "abc123".to_string(),
            doc_title: "Person Name".to_string(),
            sections: vec![
                SplitSection {
                    title: "Career".to_string(),
                    level: 2,
                    start_line: 5,
                    end_line: 10,
                    content: "Career content".to_string(),
                },
                SplitSection {
                    title: "Hobbies".to_string(),
                    level: 2,
                    start_line: 12,
                    end_line: 15,
                    content: "Hobbies content".to_string(),
                },
            ],
            avg_similarity: 0.35,
            min_similarity: 0.28,
            rationale: "Sections cover distinct topics".to_string(),
        };

        assert_eq!(candidate.doc_id, "abc123");
        assert_eq!(candidate.sections.len(), 2);
        assert!(candidate.avg_similarity < 0.5);
        assert!(candidate.min_similarity < candidate.avg_similarity);
    }
}
