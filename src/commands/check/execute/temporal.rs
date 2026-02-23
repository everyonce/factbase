//! Temporal tag checking for lint.
//!
//! Validates temporal tags and collects statistics.

use crate::commands::check::output::CheckTemporalStats;
use factbase::{
    calculate_fact_stats, detect_illogical_sequences, detect_temporal_conflicts,
    parse_temporal_tags, validate_temporal_tags, Document, TemporalTagType,
};

/// Check temporal tags for a document and update stats.
pub fn check_temporal_tags(
    doc: &Document,
    temporal_stats: &mut Option<CheckTemporalStats>,
    is_table_format: bool,
) -> (usize, usize) {
    let mut warnings = 0;
    let mut errors = 0;

    // Collect fact stats for this document
    let fact_stats = calculate_fact_stats(&doc.content);
    if let Some(ref mut ts) = temporal_stats {
        ts.total_facts += fact_stats.total_facts;
        ts.facts_with_tags += fact_stats.facts_with_tags;
    }

    // Show per-document coverage in non-JSON mode
    if is_table_format && fact_stats.total_facts > 0 {
        let coverage_pct = fact_stats.coverage * 100.0;
        if coverage_pct < 100.0 {
            println!(
                "  INFO: Temporal coverage: {}/{} facts ({:.0}%): {} [{}]",
                fact_stats.facts_with_tags, fact_stats.total_facts, coverage_pct, doc.title, doc.id
            );
        }
    }

    // Collect tag type distribution
    let tags = parse_temporal_tags(&doc.content);
    if let Some(ref mut ts) = temporal_stats {
        for tag in &tags {
            let type_name = match tag.tag_type {
                TemporalTagType::PointInTime => "PointInTime",
                TemporalTagType::LastSeen => "LastSeen",
                TemporalTagType::Range => "Range",
                TemporalTagType::Ongoing => "Ongoing",
                TemporalTagType::Historical => "Historical",
                TemporalTagType::Unknown => "Unknown",
            };
            *ts.by_type.entry(type_name.to_string()).or_insert(0) += 1;
        }
    }

    // Validate temporal tags
    let validation_errors = validate_temporal_tags(&doc.content);
    let error_count = validation_errors.len();
    if let Some(ref mut ts) = temporal_stats {
        ts.format_errors += error_count;
    }
    for err in validation_errors {
        if is_table_format {
            println!(
                "  ERROR: Invalid temporal tag at line {}: {} - {} [{}]",
                err.line_number, err.raw_text, err.message, doc.id
            );
        }
        errors += 1;
    }

    // Check for conflicting temporal tags on same line
    let conflicts = detect_temporal_conflicts(&doc.content);
    let conflict_count = conflicts.len();
    if let Some(ref mut ts) = temporal_stats {
        ts.conflicts += conflict_count;
    }
    for conflict in conflicts {
        if is_table_format {
            println!(
                "  WARN: Temporal conflict at line {}: {} vs {} - {} [{}]",
                conflict.line_number, conflict.tag1, conflict.tag2, conflict.message, doc.id
            );
        }
        warnings += 1;
    }

    // Check for illogical sequences (end before start, far future dates)
    let sequence_errors = detect_illogical_sequences(&doc.content);
    let seq_error_count = sequence_errors.len();
    if let Some(ref mut ts) = temporal_stats {
        ts.illogical_sequences += seq_error_count;
    }
    for err in sequence_errors {
        if is_table_format {
            println!(
                "  WARN: Illogical temporal sequence at line {}: {} - {} [{}]",
                err.line_number, err.raw_text, err.message, doc.id
            );
        }
        warnings += 1;
    }

    (warnings, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::check::execute::test_helpers::make_test_doc;

    #[test]
    fn test_check_temporal_tags_no_facts() {
        let doc = make_test_doc("# Title\n\nJust some text without facts.");
        let mut stats = Some(CheckTemporalStats::default());
        let (warnings, errors) = check_temporal_tags(&doc, &mut stats, false);
        assert_eq!(warnings, 0);
        assert_eq!(errors, 0);
        assert_eq!(stats.as_ref().unwrap().total_facts, 0);
    }

    #[test]
    fn test_check_temporal_tags_all_tagged() {
        let doc =
            make_test_doc("# Person\n\n- CEO at Acme @t[2020..2022]\n- CTO at BigCo @t[2022..]");
        let mut stats = Some(CheckTemporalStats::default());
        let (warnings, errors) = check_temporal_tags(&doc, &mut stats, false);
        assert_eq!(warnings, 0);
        assert_eq!(errors, 0);
        let s = stats.as_ref().unwrap();
        assert_eq!(s.total_facts, 2);
        assert_eq!(s.facts_with_tags, 2);
    }

    #[test]
    fn test_check_temporal_tags_partial_coverage() {
        let doc =
            make_test_doc("# Person\n\n- CEO at Acme @t[2020..2022]\n- Lives in Austin\n- Has PhD");
        let mut stats = Some(CheckTemporalStats::default());
        let (warnings, errors) = check_temporal_tags(&doc, &mut stats, false);
        assert_eq!(warnings, 0);
        assert_eq!(errors, 0);
        let s = stats.as_ref().unwrap();
        assert_eq!(s.total_facts, 3);
        assert_eq!(s.facts_with_tags, 1);
    }

    #[test]
    fn test_check_temporal_tags_invalid_date() {
        // Invalid date (month 13) should produce an error
        let doc = make_test_doc("# Person\n\n- CEO at Acme @t[2020-13..2021]");
        let mut stats = Some(CheckTemporalStats::default());
        let (_warnings, errors) = check_temporal_tags(&doc, &mut stats, false);
        assert_eq!(errors, 1);
        assert_eq!(stats.as_ref().unwrap().format_errors, 1);
    }

    #[test]
    fn test_check_temporal_tags_type_distribution() {
        let doc = make_test_doc(
            "# Person\n\n- Founded company @t[=2019]\n- Lives in Austin @t[~2024-01]\n- CTO @t[2020..2022]",
        );
        let mut stats = Some(CheckTemporalStats::default());
        let (_warnings, _errors) = check_temporal_tags(&doc, &mut stats, false);
        let s = stats.as_ref().unwrap();
        assert!(s.by_type.get("PointInTime").unwrap_or(&0) >= &1);
        assert!(s.by_type.get("LastSeen").unwrap_or(&0) >= &1);
        assert!(s.by_type.get("Range").unwrap_or(&0) >= &1);
    }

    #[test]
    fn test_check_temporal_tags_none_stats() {
        // Should work even when stats is None
        let doc = make_test_doc("# Person\n\n- CEO at Acme @t[2020..2022]");
        let mut stats: Option<CheckTemporalStats> = None;
        let (warnings, errors) = check_temporal_tags(&doc, &mut stats, false);
        assert_eq!(warnings, 0);
        assert_eq!(errors, 0);
        assert!(stats.is_none());
    }
}
