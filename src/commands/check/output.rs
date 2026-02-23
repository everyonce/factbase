//! Output formatting for lint results.
//!
//! Contains structs for lint output:
//! - [`CheckTemporalStats`] - Temporal tag statistics
//! - [`CheckSourceStats`] - Source footnote statistics
//! - [`CheckResult`] - Overall lint result
//! - [`ExportedQuestion`] - Single exported question
//! - [`ExportedDocQuestions`] - Document with exported questions

use crate::commands::OutputFormat;
use factbase::{format_json, format_yaml};
use serde::Serialize;
use std::collections::HashMap;

/// Temporal statistics for lint output
#[derive(Debug, Clone, Serialize, Default)]
pub struct CheckTemporalStats {
    pub total_facts: usize,
    pub facts_with_tags: usize,
    pub coverage_percent: f32,
    pub format_errors: usize,
    pub conflicts: usize,
    pub illogical_sequences: usize,
    pub by_type: HashMap<String, usize>,
}

/// Source statistics for lint output
#[derive(Debug, Clone, Serialize, Default)]
pub struct CheckSourceStats {
    pub total_facts: usize,
    pub facts_with_sources: usize,
    pub coverage_percent: f32,
    pub orphan_refs: usize,
    pub orphan_defs: usize,
    pub by_type: HashMap<String, usize>,
}

/// Exported question for --export-questions output
#[derive(Debug, Clone, Serialize)]
pub struct ExportedQuestion {
    #[serde(rename = "type")]
    pub question_type: String,
    pub line_ref: Option<usize>,
    pub description: String,
}

/// Document with exported questions for --export-questions output
#[derive(Debug, Clone, Serialize)]
pub struct ExportedDocQuestions {
    pub doc_id: String,
    pub doc_title: String,
    pub file_path: String,
    pub questions: Vec<ExportedQuestion>,
}

/// Lint result for JSON output
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub errors: usize,
    pub warnings: usize,
    pub fixed: usize,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub type_distribution: HashMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_stats: Option<CheckTemporalStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_stats: Option<CheckSourceStats>,
}

/// Print lint result in the specified format.
pub fn print_check_result(result: &CheckResult, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", format_json(result)?);
        }
        OutputFormat::Yaml => {
            println!("{}", format_yaml(result)?);
        }
        OutputFormat::Table => {
            println!();

            // Show temporal summary if --check-temporal was used
            if let Some(ref ts) = result.temporal_stats {
                println!(
                    "Temporal: {:.0}% coverage ({}/{} facts), {} format error(s), {} conflict(s)",
                    ts.coverage_percent,
                    ts.facts_with_tags,
                    ts.total_facts,
                    ts.format_errors,
                    ts.conflicts + ts.illogical_sequences
                );
                if !ts.by_type.is_empty() {
                    print!("  Tag types: ");
                    let mut types: Vec<_> = ts.by_type.iter().collect();
                    types.sort_by(|a, b| b.1.cmp(a.1));
                    let type_strs: Vec<_> =
                        types.iter().map(|(t, c)| format!("{t}: {c}")).collect();
                    println!("{}", type_strs.join(", "));
                }
                println!();
            }

            // Show source summary if --check-sources was used
            if let Some(ref ss) = result.source_stats {
                println!(
                    "Sources: {:.0}% coverage ({}/{} facts), {} orphan ref(s), {} orphan def(s)",
                    ss.coverage_percent,
                    ss.facts_with_sources,
                    ss.total_facts,
                    ss.orphan_refs,
                    ss.orphan_defs
                );
                if !ss.by_type.is_empty() {
                    print!("  Source types: ");
                    let mut types: Vec<_> = ss.by_type.iter().collect();
                    types.sort_by(|a, b| b.1.cmp(a.1));
                    let type_strs: Vec<_> =
                        types.iter().map(|(t, c)| format!("{t}: {c}")).collect();
                    println!("{}", type_strs.join(", "));
                }
                println!();
            }

            if result.errors == 0 && result.warnings == 0 && result.fixed == 0 {
                println!("✓ No issues found");
            } else if result.fixed > 0 {
                println!(
                    "Found {} error(s), {} warning(s), fixed {} issue(s)",
                    result.errors, result.warnings, result.fixed
                );
            } else {
                println!(
                    "Found {} error(s), {} warning(s)",
                    result.errors, result.warnings
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_temporal_stats_default() {
        let stats = CheckTemporalStats::default();
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert_eq!(stats.format_errors, 0);
        assert_eq!(stats.conflicts, 0);
        assert_eq!(stats.illogical_sequences, 0);
        assert!(stats.by_type.is_empty());
    }

    #[test]
    fn test_lint_source_stats_default() {
        let stats = CheckSourceStats::default();
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_sources, 0);
        assert_eq!(stats.coverage_percent, 0.0);
        assert_eq!(stats.orphan_refs, 0);
        assert_eq!(stats.orphan_defs, 0);
        assert!(stats.by_type.is_empty());
    }

    #[test]
    fn test_lint_result_json_serialization() {
        let result = CheckResult {
            errors: 2,
            warnings: 3,
            fixed: 1,
            type_distribution: HashMap::new(),
            temporal_stats: None,
            source_stats: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        // Empty type_distribution should be skipped
        assert!(!json.contains("type_distribution"));
        // None stats should be skipped
        assert!(!json.contains("temporal_stats"));
        assert!(!json.contains("source_stats"));
        // Core fields present
        assert!(json.contains("\"errors\":2"));
        assert!(json.contains("\"warnings\":3"));
        assert!(json.contains("\"fixed\":1"));
    }

    #[test]
    fn test_lint_result_with_type_distribution() {
        let mut type_dist = HashMap::new();
        type_dist.insert("person".to_string(), 5);
        let result = CheckResult {
            errors: 0,
            warnings: 0,
            fixed: 0,
            type_distribution: type_dist,
            temporal_stats: None,
            source_stats: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        // Non-empty type_distribution should be included
        assert!(json.contains("type_distribution"));
        assert!(json.contains("\"person\":5"));
    }

    #[test]
    fn test_lint_result_with_temporal_stats() {
        let mut by_type = HashMap::new();
        by_type.insert("range".to_string(), 10);
        let result = CheckResult {
            errors: 1,
            warnings: 2,
            fixed: 0,
            type_distribution: HashMap::new(),
            temporal_stats: Some(CheckTemporalStats {
                total_facts: 20,
                facts_with_tags: 15,
                coverage_percent: 75.0,
                format_errors: 1,
                conflicts: 2,
                illogical_sequences: 0,
                by_type,
            }),
            source_stats: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("temporal_stats"));
        assert!(json.contains("\"total_facts\":20"));
        assert!(json.contains("\"coverage_percent\":75.0"));
    }

    #[test]
    fn test_exported_question_serialization() {
        let question = ExportedQuestion {
            question_type: "temporal".to_string(),
            line_ref: Some(5),
            description: "When was this role held?".to_string(),
        };
        let json = serde_json::to_string(&question).unwrap();
        // type field should be renamed from question_type
        assert!(json.contains("\"type\":\"temporal\""));
        assert!(json.contains("\"line_ref\":5"));
        assert!(json.contains("\"description\":\"When was this role held?\""));
    }

    #[test]
    fn test_exported_doc_questions_serialization() {
        let doc = ExportedDocQuestions {
            doc_id: "abc123".to_string(),
            doc_title: "Test Doc".to_string(),
            file_path: "people/test.md".to_string(),
            questions: vec![ExportedQuestion {
                question_type: "missing".to_string(),
                line_ref: None,
                description: "Source needed".to_string(),
            }],
        };
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"doc_id\":\"abc123\""));
        assert!(json.contains("\"doc_title\":\"Test Doc\""));
        assert!(json.contains("\"file_path\":\"people/test.md\""));
        assert!(json.contains("\"questions\":["));
    }
}
