//! Source footnote checking for lint.
//!
//! Validates source references and definitions.

use crate::commands::check::output::CheckSourceStats;
use factbase::{
    calculate_fact_stats, count_facts_with_sources, parse_source_definitions,
    parse_source_references, Document,
};
use std::collections::HashSet;

/// Check source references for a document and update stats.
pub fn check_source_refs(
    doc: &Document,
    source_stats: &mut Option<CheckSourceStats>,
    is_table_format: bool,
) -> (usize, usize) {
    let mut warnings = 0;
    let mut errors = 0;

    let refs = parse_source_references(&doc.content);
    let defs = parse_source_definitions(&doc.content);
    let defined_numbers: HashSet<_> = defs.iter().map(|d| d.number).collect();
    let referenced_numbers: HashSet<_> = refs.iter().map(|r| r.number).collect();

    // Track fact coverage
    let fact_stats = calculate_fact_stats(&doc.content);
    if let Some(ref mut ss) = source_stats {
        ss.total_facts += fact_stats.total_facts;
        let facts_with_sources = count_facts_with_sources(&doc.content);
        ss.facts_with_sources += facts_with_sources;
    }

    // Check for orphan references
    let mut orphan_ref_count = 0;
    for r in &refs {
        if !defined_numbers.contains(&r.number) {
            if is_table_format {
                println!(
                    "  ERROR: Orphan reference [^{}] at line {} (no definition): {} [{}]",
                    r.number, r.line_number, doc.title, doc.id
                );
            }
            errors += 1;
            orphan_ref_count += 1;
        }
    }
    if let Some(ref mut ss) = source_stats {
        ss.orphan_refs += orphan_ref_count;
    }

    // Check for orphan definitions and non-standard source types
    // Also track source type distribution (consuming defs to avoid clone)
    let mut orphan_def_count = 0;
    for d in defs {
        if !referenced_numbers.contains(&d.number) {
            if is_table_format {
                println!(
                    "  WARN: Orphan definition [^{}] at line {} (never referenced): {} [{}]",
                    d.number, d.line_number, doc.title, doc.id
                );
            }
            warnings += 1;
            orphan_def_count += 1;
        }

        if d.source_type == "Unknown" {
            if is_table_format {
                println!(
                    "  WARN: Non-standard source type in [^{}] at line {}: {} [{}]",
                    d.number, d.line_number, doc.title, doc.id
                );
                println!(
                    "        Standard types: LinkedIn, Website, Press release, News, Filing, Direct, Email, Event, Inferred, Unverified"
                );
            }
            warnings += 1;
        }

        // Track source type distribution (takes ownership, no clone needed)
        if let Some(ref mut ss) = source_stats {
            *ss.by_type.entry(d.source_type).or_insert(0) += 1;
        }
    }
    if let Some(ref mut ss) = source_stats {
        ss.orphan_defs += orphan_def_count;
    }

    (warnings, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::check::execute::test_helpers::make_test_doc;

    #[test]
    fn test_check_source_refs_orphan_reference() {
        // Reference [^1] without definition
        let doc = make_test_doc("# Person\n\n- CEO at Acme [^1]\n");
        let mut stats = Some(CheckSourceStats::default());
        let (warnings, errors) = check_source_refs(&doc, &mut stats, false);
        assert_eq!(errors, 1); // orphan reference
        assert_eq!(warnings, 0);
        assert_eq!(stats.as_ref().unwrap().orphan_refs, 1);
    }

    #[test]
    fn test_check_source_refs_orphan_definition() {
        // Definition [^1] without reference
        let doc = make_test_doc("# Person\n\n- CEO at Acme\n\n---\n[^1]: LinkedIn, 2024-01-15");
        let mut stats = Some(CheckSourceStats::default());
        let (warnings, errors) = check_source_refs(&doc, &mut stats, false);
        assert_eq!(errors, 0);
        assert_eq!(warnings, 1); // orphan definition
        assert_eq!(stats.as_ref().unwrap().orphan_defs, 1);
    }

    #[test]
    fn test_check_source_refs_valid_sources() {
        // Matching reference and definition
        let doc =
            make_test_doc("# Person\n\n- CEO at Acme [^1]\n\n---\n[^1]: LinkedIn, 2024-01-15");
        let mut stats = Some(CheckSourceStats::default());
        let (warnings, errors) = check_source_refs(&doc, &mut stats, false);
        assert_eq!(errors, 0);
        assert_eq!(warnings, 0);
        assert_eq!(stats.as_ref().unwrap().orphan_refs, 0);
        assert_eq!(stats.as_ref().unwrap().orphan_defs, 0);
    }

    #[test]
    fn test_check_source_refs_stats_accumulation() {
        let doc = make_test_doc(
            "# Person\n\n- CEO at Acme [^1]\n- CTO at BigCo [^2]\n\n---\n[^1]: LinkedIn, 2024-01-15\n[^2]: Website, 2024-02-01",
        );
        let mut stats = Some(CheckSourceStats::default());
        let (warnings, errors) = check_source_refs(&doc, &mut stats, false);
        assert_eq!(errors, 0);
        assert_eq!(warnings, 0);
        let s = stats.as_ref().unwrap();
        assert_eq!(s.facts_with_sources, 2);
        assert!(s.by_type.get("LinkedIn").unwrap_or(&0) >= &1);
        assert!(s.by_type.get("Website").unwrap_or(&0) >= &1);
    }
}
