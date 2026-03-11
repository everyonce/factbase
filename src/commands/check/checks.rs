//! Lint check functions.
//!
//! Contains individual lint check functions for:
//! - Stub detection
//! - Stale document detection
//! - Type validation
//! - Temporal tag validation
//! - Source footnote validation
//!
//! Note: Orphan and broken link detection remain in mod.rs as they require
//! database access (links_from/links_to queries).

use chrono::{Duration, Utc};
use factbase::models::TemporalTagType;
use factbase::processor::{
    calculate_fact_stats,
    count_facts_with_sources,
    detect_illogical_sequences,
    detect_temporal_conflicts,
    parse_source_definitions,
    parse_source_references,
    parse_temporal_tags,
    validate_temporal_tags,
};
use std::collections::{HashMap, HashSet};

/// Result of linting a single document (for parallel processing)
#[derive(Debug, Default)]
pub struct DocCheckResult {
    pub errors: usize,
    pub warnings: usize,
    pub messages: Vec<String>,
    // Temporal stats
    pub temporal_total_facts: usize,
    pub temporal_facts_with_tags: usize,
    pub temporal_format_errors: usize,
    pub temporal_conflicts: usize,
    pub temporal_illogical_sequences: usize,
    pub temporal_by_type: HashMap<String, usize>,
    // Source stats
    pub source_total_facts: usize,
    pub source_facts_with_sources: usize,
    pub source_orphan_refs: usize,
    pub source_orphan_defs: usize,
    pub source_by_type: HashMap<String, usize>,
    // Basic checks (stub, stale, unknown type)
    pub is_stub: bool,
    pub is_stale: bool,
    pub stale_days: Option<i64>,
    pub is_unknown_type: bool,
}

/// Options for parallel lint checks
#[derive(Clone)]
pub struct ParallelCheckOptions {
    pub check_temporal: bool,
    pub check_sources: bool,
    pub min_length: usize,
    pub max_age_days: Option<i64>,
    pub allowed_types: Option<Vec<String>>,
}

/// Perform CPU-bound lint checks on a document (can be parallelized)
pub fn check_document_content(
    doc: &factbase::models::Document,
    opts: &ParallelCheckOptions,
) -> DocCheckResult {
    let mut result = DocCheckResult::default();
    let content = &doc.content;
    let doc_id = &doc.id;
    let doc_title = &doc.title;
    let doc_type = doc.doc_type.as_deref();
    let file_modified_at = doc.file_modified_at;
    let indexed_at = doc.indexed_at;

    // Check for stub documents (content too short)
    let content_len = content.len();
    if content_len < opts.min_length {
        result.is_stub = true;
        result.messages.push(format!(
            "  WARN: Stub document ({content_len} chars): {doc_title} [{doc_id}]"
        ));
        result.warnings += 1;
    }

    // Check for unknown document types
    if let Some(ref allowed) = opts.allowed_types {
        let doc_type_str = doc_type.unwrap_or("");
        if !allowed.iter().any(|t| t.to_lowercase() == doc_type_str) {
            result.is_unknown_type = true;
            result.messages.push(format!(
                "  WARN: Unknown type '{doc_type_str}': {doc_title} [{doc_id}]"
            ));
            result.warnings += 1;
        }
    }

    // Check for stale documents
    if let Some(max_age_days) = opts.max_age_days {
        let cutoff = Utc::now() - Duration::days(max_age_days);
        let doc_date = file_modified_at.unwrap_or(indexed_at);
        if doc_date < cutoff {
            let age_days = (Utc::now() - doc_date).num_days();
            result.is_stale = true;
            result.stale_days = Some(age_days);
            result.messages.push(format!(
                "  WARN: Stale document ({age_days} days old): {doc_title} [{doc_id}]"
            ));
            result.warnings += 1;
        }
    }

    // Check temporal tag validity and collect stats
    if opts.check_temporal {
        let fact_stats = calculate_fact_stats(content);
        result.temporal_total_facts = fact_stats.total_facts;
        result.temporal_facts_with_tags = fact_stats.facts_with_tags;

        // Show per-document coverage
        if fact_stats.total_facts > 0 {
            let coverage_pct = fact_stats.coverage * 100.0;
            if coverage_pct < 100.0 {
                result.messages.push(format!(
                    "  INFO: Temporal coverage: {}/{} facts ({:.0}%): {} [{}]",
                    fact_stats.facts_with_tags,
                    fact_stats.total_facts,
                    coverage_pct,
                    doc_title,
                    doc_id
                ));
            }
        }

        // Collect tag type distribution
        let tags = parse_temporal_tags(content);
        for tag in &tags {
            let type_name = match tag.tag_type {
                TemporalTagType::PointInTime => "PointInTime",
                TemporalTagType::LastSeen => "LastSeen",
                TemporalTagType::Range => "Range",
                TemporalTagType::Ongoing => "Ongoing",
                TemporalTagType::Historical => "Historical",
                TemporalTagType::Unknown => "Unknown",
            };
            *result
                .temporal_by_type
                .entry(type_name.to_string())
                .or_insert(0) += 1;
        }

        // Validate temporal tags
        let validation_errors = validate_temporal_tags(content);
        result.temporal_format_errors = validation_errors.len();
        for err in validation_errors {
            result.messages.push(format!(
                "  ERROR: Invalid temporal tag at line {}: {} - {} [{}]",
                err.line_number, err.raw_text, err.message, doc_id
            ));
            result.errors += 1;
        }

        // Check for conflicting temporal tags
        let conflicts = detect_temporal_conflicts(content);
        result.temporal_conflicts = conflicts.len();
        for conflict in conflicts {
            result.messages.push(format!(
                "  WARN: Temporal conflict at line {}: {} vs {} - {} [{}]",
                conflict.line_number, conflict.tag1, conflict.tag2, conflict.message, doc_id
            ));
            result.warnings += 1;
        }

        // Check for illogical sequences
        let sequence_errors = detect_illogical_sequences(content);
        result.temporal_illogical_sequences = sequence_errors.len();
        for err in sequence_errors {
            result.messages.push(format!(
                "  WARN: Illogical temporal sequence at line {}: {} - {} [{}]",
                err.line_number, err.raw_text, err.message, doc_id
            ));
            result.warnings += 1;
        }
    }

    // Check source footnotes
    if opts.check_sources {
        let refs = parse_source_references(content);
        let defs = parse_source_definitions(content);
        let defined_numbers: HashSet<_> = defs.iter().map(|d| d.number).collect();
        let referenced_numbers: HashSet<_> = refs.iter().map(|r| r.number).collect();

        let fact_stats = calculate_fact_stats(content);
        result.source_total_facts = fact_stats.total_facts;
        result.source_facts_with_sources = count_facts_with_sources(content);

        for d in &defs {
            *result
                .source_by_type
                .entry(d.source_type.clone())
                .or_insert(0) += 1;
        }

        // Check orphan references
        for r in &refs {
            if !defined_numbers.contains(&r.number) {
                result.messages.push(format!(
                    "  ERROR: Orphan reference [^{}] at line {} (no definition): {} [{}]",
                    r.number, r.line_number, doc_title, doc_id
                ));
                result.errors += 1;
                result.source_orphan_refs += 1;
            }
        }

        // Check orphan definitions
        for d in &defs {
            if !referenced_numbers.contains(&d.number) {
                result.messages.push(format!(
                    "  WARN: Orphan definition [^{}] at line {} (never referenced): {} [{}]",
                    d.number, d.line_number, doc_title, doc_id
                ));
                result.warnings += 1;
                result.source_orphan_defs += 1;
            }
        }

        // Check non-standard source types
        for d in &defs {
            if d.source_type == "Unknown" {
                result.messages.push(format!(
                    "  WARN: Non-standard source type in [^{}] at line {}: {} [{}]",
                    d.number, d.line_number, doc_title, doc_id
                ));
                result.messages.push(
                    "        Standard types: LinkedIn, Website, Press release, News, Filing, Direct, Email, Event, Inferred, Unverified".to_string()
                );
                result.warnings += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    /// Helper to create default ParallelCheckOptions for tests
    fn test_opts(check_temporal: bool, check_sources: bool) -> ParallelCheckOptions {
        ParallelCheckOptions {
            check_temporal,
            check_sources,
            min_length: 0, // Disable stub check by default in tests
            max_age_days: None,
            allowed_types: None,
        }
    }

    fn make_doc(content: &str) -> factbase::models::Document {
        make_doc_with(content, None, None, Utc::now())
    }

    fn make_doc_with(
        content: &str,
        doc_type: Option<&str>,
        modified: Option<DateTime<Utc>>,
        indexed: DateTime<Utc>,
    ) -> factbase::models::Document {
        factbase::models::Document {
            id: "abc123".into(),
            repo_id: "test".into(),
            file_path: "test.md".into(),
            file_hash: "hash".into(),
            title: "Test".into(),
            doc_type: doc_type.map(|s| s.to_string()),
            content: content.to_string(),
            file_modified_at: modified,
            indexed_at: indexed,
            is_deleted: false,
        }
    }

    #[test]
    fn test_check_document_content_no_checks() {
        let doc = make_doc("# Test Document\n\n- Some fact\n- Another fact");
        let result = check_document_content(&doc, &test_opts(false, false));
        assert_eq!(result.errors, 0);
        assert_eq!(result.warnings, 0);
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_check_document_content_temporal_coverage() {
        let doc = make_doc("# Test\n\n- Fact without tag\n- Fact with tag @t[2024]");
        let result = check_document_content(&doc, &test_opts(true, false));
        assert_eq!(result.temporal_total_facts, 2);
        assert_eq!(result.temporal_facts_with_tags, 1);
        assert!(result.messages.iter().any(|m| m.contains("Temporal coverage")));
    }

    #[test]
    fn test_check_document_content_temporal_full_coverage() {
        let doc = make_doc("# Test\n\n- Fact @t[2024]\n- Another @t[2023..2024]");
        let result = check_document_content(&doc, &test_opts(true, false));
        assert_eq!(result.temporal_total_facts, 2);
        assert_eq!(result.temporal_facts_with_tags, 2);
        assert!(!result.messages.iter().any(|m| m.contains("Temporal coverage")));
    }

    #[test]
    fn test_check_document_content_temporal_invalid_tag() {
        let doc = make_doc("# Test\n\n- Fact @t[2024-13]");
        let result = check_document_content(&doc, &test_opts(true, false));
        assert_eq!(result.temporal_format_errors, 1);
        assert_eq!(result.errors, 1);
        assert!(result.messages.iter().any(|m| m.contains("Invalid temporal tag")));
    }

    #[test]
    fn test_check_document_content_temporal_conflict() {
        let doc = make_doc("# Test\n\n- Role @t[2020..2022] @t[2021..]");
        let result = check_document_content(&doc, &test_opts(true, false));
        assert_eq!(result.temporal_conflicts, 1);
        assert_eq!(result.warnings, 1);
        assert!(result.messages.iter().any(|m| m.contains("Temporal conflict")));
    }

    #[test]
    fn test_check_document_content_source_orphan_ref() {
        let doc = make_doc("# Test\n\n- Fact [^1]\n\n---\n[^2]: LinkedIn profile, 2024-01-01");
        let result = check_document_content(&doc, &test_opts(false, true));
        assert_eq!(result.source_orphan_refs, 1);
        assert_eq!(result.errors, 1);
        assert!(result.messages.iter().any(|m| m.contains("Orphan reference")));
    }

    #[test]
    fn test_check_document_content_source_orphan_def() {
        let doc = make_doc("# Test\n\n- Fact [^1]\n\n---\n[^1]: LinkedIn profile, 2024-01-01\n[^2]: News article, 2024-01-01");
        let result = check_document_content(&doc, &test_opts(false, true));
        assert_eq!(result.source_orphan_defs, 1);
        assert_eq!(result.warnings, 1);
        assert!(result.messages.iter().any(|m| m.contains("Orphan definition")));
    }

    #[test]
    fn test_check_document_content_source_coverage() {
        let doc = make_doc("# Test\n\n- Fact with source [^1]\n- Fact without source\n\n---\n[^1]: LinkedIn profile, 2024-01-01");
        let result = check_document_content(&doc, &test_opts(false, true));
        assert_eq!(result.source_total_facts, 2);
        assert_eq!(result.source_facts_with_sources, 1);
    }

    #[test]
    fn test_check_document_content_both_checks() {
        let doc = make_doc("# Test\n\n- Fact @t[2024] [^1]\n\n---\n[^1]: LinkedIn profile, 2024-01-01");
        let result = check_document_content(&doc, &test_opts(true, true));
        assert_eq!(result.temporal_total_facts, 1);
        assert_eq!(result.temporal_facts_with_tags, 1);
        assert_eq!(result.source_total_facts, 1);
        assert_eq!(result.source_facts_with_sources, 1);
        assert_eq!(result.errors, 0);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_check_document_content_stub_check() {
        let doc = make_doc("# Test\n\nShort");
        let mut opts = test_opts(false, false);
        opts.min_length = 100;
        let result = check_document_content(&doc, &opts);
        assert!(result.is_stub);
        assert_eq!(result.warnings, 1);
        assert!(result.messages.iter().any(|m| m.contains("Stub document")));
    }

    #[test]
    fn test_stub_check_exactly_at_threshold() {
        let doc = make_doc(&"x".repeat(50));
        let mut opts = test_opts(false, false);
        opts.min_length = 50;
        let result = check_document_content(&doc, &opts);
        assert!(!result.is_stub);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_stub_check_one_below_threshold() {
        let doc = make_doc(&"x".repeat(49));
        let mut opts = test_opts(false, false);
        opts.min_length = 50;
        let result = check_document_content(&doc, &opts);
        assert!(result.is_stub);
        assert_eq!(result.warnings, 1);
        assert!(result.messages[0].contains("49 chars"));
    }

    #[test]
    fn test_stub_check_one_above_threshold() {
        let doc = make_doc(&"x".repeat(51));
        let mut opts = test_opts(false, false);
        opts.min_length = 50;
        let result = check_document_content(&doc, &opts);
        assert!(!result.is_stub);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_stub_check_disabled_with_zero() {
        let doc = make_doc("");
        let mut opts = test_opts(false, false);
        opts.min_length = 0;
        let result = check_document_content(&doc, &opts);
        assert!(!result.is_stub);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_check_document_content_unknown_type_check() {
        let doc = make_doc_with("# Test\n\nSome content that is long enough.", Some("unknown"), None, Utc::now());
        let mut opts = test_opts(false, false);
        opts.allowed_types = Some(vec!["person".to_string(), "project".to_string()]);
        let result = check_document_content(&doc, &opts);
        assert!(result.is_unknown_type);
        assert_eq!(result.warnings, 1);
        assert!(result.messages.iter().any(|m| m.contains("Unknown type")));
    }

    #[test]
    fn test_check_document_content_stale_check() {
        let old_date = Utc::now() - Duration::days(60);
        let doc = make_doc_with("# Test\n\nSome content that is long enough.", None, Some(old_date), Utc::now());
        let mut opts = test_opts(false, false);
        opts.max_age_days = Some(30);
        let result = check_document_content(&doc, &opts);
        assert!(result.is_stale);
        assert!(result.stale_days.unwrap() >= 59);
        assert_eq!(result.warnings, 1);
        assert!(result.messages.iter().any(|m| m.contains("Stale document")));
    }
}
