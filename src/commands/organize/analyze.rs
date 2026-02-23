//! Analyze command implementation.
//!
//! Detects reorganization opportunities: merge candidates, split candidates, misplaced documents.

use super::AnalyzeArgs;
use crate::commands::{
    find_repo_with_config, print_output, setup_embedding_with_timeout, OutputFormat,
};
use factbase::{
    detect_merge_candidates, detect_misplaced, detect_split_candidates, MergeCandidate,
    MisplacedCandidate, SplitCandidate,
};
use serde::Serialize;

/// Combined analysis results for all reorganization opportunities.
#[derive(Debug, Serialize)]
pub struct AnalysisResults {
    /// Merge candidates (highly similar documents)
    pub merge_candidates: Vec<MergeCandidate>,
    /// Split candidates (multi-topic documents)
    pub split_candidates: Vec<SplitCandidate>,
    /// Misplaced candidates (documents in wrong folders)
    pub misplaced_candidates: Vec<MisplacedCandidate>,
}

impl AnalysisResults {
    /// Total number of suggestions across all categories.
    pub fn total_count(&self) -> usize {
        self.merge_candidates.len() + self.split_candidates.len() + self.misplaced_candidates.len()
    }

    /// Check if there are any suggestions.
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

/// Run the analyze command.
pub async fn run(args: AnalyzeArgs) -> anyhow::Result<()> {
    let (config, db, repo) = find_repo_with_config(args.repo.as_deref())?;
    let format = OutputFormat::resolve(args.json, args.format);

    let repo_id = Some(repo.id.as_str());

    // Detect merge candidates (no embedding needed, uses existing embeddings)
    let merge_candidates = detect_merge_candidates(&db, args.merge_threshold, repo_id)?;

    // Detect split candidates (needs embedding provider for section embeddings)
    let embedding = setup_embedding_with_timeout(&config, args.timeout).await;
    let split_candidates =
        detect_split_candidates(&db, &embedding, args.split_threshold, repo_id).await?;

    // Detect misplaced documents (uses existing embeddings)
    let misplaced_candidates = detect_misplaced(&db, repo_id)?;

    let results = AnalysisResults {
        merge_candidates,
        split_candidates,
        misplaced_candidates,
    };

    print_output(format, &results, || print_table(&results, &repo.id))?;

    Ok(())
}

/// Print results in table format.
fn print_table(results: &AnalysisResults, repo_id: &str) {
    println!("Reorganization Analysis: {}", repo_id);
    println!("{}", "=".repeat(40));

    if results.is_empty() {
        println!("\nNo reorganization opportunities found.");
        return;
    }

    // Merge candidates
    if !results.merge_candidates.is_empty() {
        println!("\nMerge Candidates ({}):", results.merge_candidates.len());
        println!("{}", "-".repeat(40));
        for c in &results.merge_candidates {
            println!(
                "  [{:.0}%] {} + {}",
                c.similarity * 100.0,
                c.doc1_id,
                c.doc2_id
            );
            println!("         {} + {}", c.doc1_title, c.doc2_title);
            println!("         Suggested: keep {}", c.suggested_keep);
        }
    }

    // Split candidates
    if !results.split_candidates.is_empty() {
        println!("\nSplit Candidates ({}):", results.split_candidates.len());
        println!("{}", "-".repeat(40));
        for c in &results.split_candidates {
            let section_names: Vec<_> = c.sections.iter().map(|s| s.title.as_str()).collect();
            println!(
                "  [{:.0}% avg] {} [{}]",
                c.avg_similarity * 100.0,
                c.doc_id,
                c.doc_title
            );
            println!("         Sections: {}", section_names.join(", "));
        }
    }

    // Misplaced candidates
    if !results.misplaced_candidates.is_empty() {
        println!(
            "\nMisplaced Candidates ({}):",
            results.misplaced_candidates.len()
        );
        println!("{}", "-".repeat(40));
        for c in &results.misplaced_candidates {
            println!("  [{:.2}] {} [{}]", c.confidence, c.doc_id, c.doc_title);
            println!(
                "         {} → {} (suggested)",
                c.current_type, c.suggested_type
            );
        }
    }

    println!("\nTotal: {} suggestion(s)", results.total_count());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_results_empty() {
        let results = AnalysisResults {
            merge_candidates: vec![],
            split_candidates: vec![],
            misplaced_candidates: vec![],
        };
        assert!(results.is_empty());
        assert_eq!(results.total_count(), 0);
    }

    #[test]
    fn test_analysis_results_with_merge() {
        let results = AnalysisResults {
            merge_candidates: vec![MergeCandidate {
                doc1_id: "a".to_string(),
                doc1_title: "A".to_string(),
                doc2_id: "b".to_string(),
                doc2_title: "B".to_string(),
                similarity: 0.95,
                suggested_keep: "a".to_string(),
                rationale: "test".to_string(),
            }],
            split_candidates: vec![],
            misplaced_candidates: vec![],
        };
        assert!(!results.is_empty());
        assert_eq!(results.total_count(), 1);
    }

    #[test]
    fn test_analysis_results_total_count() {
        use factbase::{SplitCandidate, SplitSection};

        let results = AnalysisResults {
            merge_candidates: vec![MergeCandidate {
                doc1_id: "a".to_string(),
                doc1_title: "A".to_string(),
                doc2_id: "b".to_string(),
                doc2_title: "B".to_string(),
                similarity: 0.95,
                suggested_keep: "a".to_string(),
                rationale: "test".to_string(),
            }],
            split_candidates: vec![SplitCandidate {
                doc_id: "c".to_string(),
                doc_title: "C".to_string(),
                sections: vec![
                    SplitSection {
                        title: "S1".to_string(),
                        level: 2,
                        start_line: 1,
                        end_line: 5,
                        content: "content".to_string(),
                    },
                    SplitSection {
                        title: "S2".to_string(),
                        level: 2,
                        start_line: 6,
                        end_line: 10,
                        content: "content".to_string(),
                    },
                ],
                avg_similarity: 0.3,
                min_similarity: 0.2,
                rationale: "test".to_string(),
            }],
            misplaced_candidates: vec![MisplacedCandidate {
                doc_id: "d".to_string(),
                doc_title: "D".to_string(),
                current_type: "person".to_string(),
                suggested_type: "project".to_string(),
                confidence: 0.15,
                rationale: "test".to_string(),
            }],
        };
        assert_eq!(results.total_count(), 3);
    }
}
