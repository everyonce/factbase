//! Analyze command implementation.
//!
//! Detects reorganization opportunities: merge candidates, split candidates, misplaced documents.

use super::AnalyzeArgs;
use crate::commands::{
    find_repo_with_config, print_output, setup_embedding_with_timeout, OutputFormat,
};
use factbase::{
    assess_staleness, detect_duplicate_entries, detect_ghost_files, detect_merge_candidates,
    detect_misplaced, detect_split_candidates, DuplicateEntry, GhostFile, MergeCandidate,
    MisplacedCandidate, SplitCandidate, StaleDuplicate,
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
    /// Duplicate entity entries across documents
    pub duplicate_entries: Vec<DuplicateEntry>,
    /// Stale duplicate entries with newer versions elsewhere
    pub stale_entries: Vec<StaleDuplicate>,
    /// Ghost files (same ID or title in same directory)
    pub ghost_files: Vec<GhostFile>,
}

impl AnalysisResults {
    /// Total number of suggestions across all categories.
    pub fn total_count(&self) -> usize {
        self.merge_candidates.len()
            + self.split_candidates.len()
            + self.misplaced_candidates.len()
            + self.duplicate_entries.len()
            + self.stale_entries.len()
            + self.ghost_files.len()
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
    let progress = factbase::ProgressReporter::Cli { quiet: false };

    let repo_id = Some(repo.id.as_str());

    // Detect merge candidates (no embedding needed, uses existing embeddings)
    progress.phase("Analysis 1/5: Ghost files");
    let ghost_files = detect_ghost_files(&db, Some(repo.id.as_str()), &progress)?;

    progress.phase("Analysis 2/5: Merge candidates");
    let merge_candidates = detect_merge_candidates(&db, args.merge_threshold, repo_id, &progress)?;

    // Detect split candidates (needs embedding provider for section embeddings)
    progress.phase("Analysis 3/5: Split candidates");
    let embedding = setup_embedding_with_timeout(&config, args.timeout).await;
    let split_candidates =
        detect_split_candidates(&db, &embedding, args.split_threshold, repo_id, &progress).await?;

    // Detect misplaced documents (uses existing embeddings)
    progress.phase("Analysis 4/5: Misplaced documents");
    let misplaced_candidates = detect_misplaced(&db, repo_id, &progress)?;

    // Detect duplicate entity entries across documents
    progress.phase("Analysis 5/5: Duplicate entries");
    let duplicate_entries = detect_duplicate_entries(&db, &*embedding, repo_id, &progress).await?;

    // Assess staleness of duplicate entries
    let stale_entries = assess_staleness(&duplicate_entries, &db)?;

    let results = AnalysisResults {
        merge_candidates,
        split_candidates,
        misplaced_candidates,
        duplicate_entries,
        stale_entries,
        ghost_files,
    };

    print_output(format, &results, || print_table(&results, &repo.id))?;

    Ok(())
}

/// Print results in table format.
fn print_table(results: &AnalysisResults, repo_id: &str) {
    println!("Reorganization Analysis: {repo_id}");
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

    // Duplicate entity entries with inline staleness
    if !results.duplicate_entries.is_empty() {
        // Build staleness lookup: (entity_name, doc_id) -> "current" or "stale"
        let mut staleness_map = std::collections::HashMap::new();
        for s in &results.stale_entries {
            staleness_map.insert(
                (s.entity_name.as_str(), s.current.doc_id.as_str()),
                "current",
            );
            for e in &s.stale {
                staleness_map.insert((s.entity_name.as_str(), e.doc_id.as_str()), "stale");
            }
        }

        println!("\nDuplicate Entries ({}):", results.duplicate_entries.len());
        println!("{}", "-".repeat(40));
        for d in &results.duplicate_entries {
            println!(
                "  \"{}\" appears in {} documents:",
                d.entity_name,
                d.entries.len()
            );
            for e in &d.entries {
                let staleness = staleness_map
                    .get(&(d.entity_name.as_str(), e.doc_id.as_str()))
                    .copied()
                    .unwrap_or("");
                let tag = match staleness {
                    "current" => " [CURRENT]",
                    "stale" => " [STALE]",
                    _ => "",
                };
                let section = if e.section.is_empty() {
                    String::new()
                } else {
                    format!(" §{}", e.section)
                };
                println!(
                    "    - {} [{}]{} (line {}, {} facts){}",
                    e.doc_title,
                    e.doc_id,
                    section,
                    e.line_start,
                    e.facts.len(),
                    tag,
                );
            }
        }
    }

    // Ghost files (same ID or title in same directory)
    if !results.ghost_files.is_empty() {
        println!("\nGhost Files ({}):", results.ghost_files.len());
        println!("{}", "-".repeat(40));
        for g in &results.ghost_files {
            println!(
                "  [{}] {} [{}]",
                g.reason, g.title, g.doc_id
            );
            println!(
                "         tracked: {} ({} lines)",
                g.tracked_path, g.tracked_lines
            );
            println!(
                "         ghost:   {} ({} lines)",
                g.ghost_path, g.ghost_lines
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
            duplicate_entries: vec![],
            stale_entries: vec![],
            ghost_files: vec![],
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
            duplicate_entries: vec![],
            stale_entries: vec![],
            ghost_files: vec![],
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
            duplicate_entries: vec![],
            stale_entries: vec![],
            ghost_files: vec![],
        };
        assert_eq!(results.total_count(), 3);
    }

    #[test]
    fn test_print_table_duplicate_entries_with_staleness() {
        use factbase::EntryLocation;

        let loc_acme = EntryLocation {
            doc_id: "aaa111".to_string(),
            doc_title: "Acme Corp".to_string(),
            section: "Team".to_string(),
            line_start: 10,
            facts: vec!["VP Engineering".to_string()],
        };
        let loc_globex = EntryLocation {
            doc_id: "bbb222".to_string(),
            doc_title: "Globex Inc".to_string(),
            section: "Staff".to_string(),
            line_start: 20,
            facts: vec!["CTO".to_string()],
        };

        let results = AnalysisResults {
            merge_candidates: vec![],
            split_candidates: vec![],
            misplaced_candidates: vec![],
            duplicate_entries: vec![DuplicateEntry {
                entity_name: "Jane Smith".to_string(),
                entries: vec![loc_acme.clone(), loc_globex.clone()],
            }],
            stale_entries: vec![StaleDuplicate {
                entity_name: "Jane Smith".to_string(),
                current: loc_globex,
                stale: vec![loc_acme],
            }],
            ghost_files: vec![],
        };

        // Verify it doesn't panic and counts correctly
        print_table(&results, "test-repo");
        assert_eq!(results.total_count(), 2);
    }
}
