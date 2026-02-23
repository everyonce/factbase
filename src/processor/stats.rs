//! Fact statistics calculation.
//!
//! This module handles calculating statistics about facts in documents,
//! including temporal coverage and source attribution.

use crate::models::FactStats;
use crate::patterns::{FACT_LINE_REGEX, TEMPORAL_TAG_FULL_REGEX};

/// Count total facts (list items) in document content.
/// A fact is defined as a list item starting with `-`, `*`, or a number followed by `.` or `)`.
pub fn count_facts(content: &str) -> usize {
    content
        .lines()
        .filter(|line| FACT_LINE_REGEX.is_match(line))
        .count()
}

/// Count facts that have at least one temporal tag on the same line.
pub fn count_facts_with_temporal_tags(content: &str) -> usize {
    content
        .lines()
        .filter(|line| FACT_LINE_REGEX.is_match(line) && TEMPORAL_TAG_FULL_REGEX.is_match(line))
        .count()
}

/// Calculate fact statistics for document content.
/// Returns FactStats with total facts, facts with temporal tags, and coverage percentage.
pub fn calculate_fact_stats(content: &str) -> FactStats {
    let total_facts = count_facts(content);
    let facts_with_tags = count_facts_with_temporal_tags(content);
    let coverage = if total_facts > 0 {
        facts_with_tags as f32 / total_facts as f32
    } else {
        0.0
    };

    FactStats {
        total_facts,
        facts_with_tags,
        coverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_facts_dash_list() {
        let content = "# Title\n- Fact one\n- Fact two\n- Fact three";
        assert_eq!(count_facts(content), 3);
    }

    #[test]
    fn test_count_facts_asterisk_list() {
        let content = "# Title\n* Fact one\n* Fact two";
        assert_eq!(count_facts(content), 2);
    }

    #[test]
    fn test_count_facts_numbered_list() {
        let content = "# Title\n1. First\n2. Second\n3. Third";
        assert_eq!(count_facts(content), 3);
    }

    #[test]
    fn test_count_facts_numbered_paren() {
        let content = "# Title\n1) First\n2) Second";
        assert_eq!(count_facts(content), 2);
    }

    #[test]
    fn test_count_facts_mixed() {
        let content = "# Title\n- Dash item\n* Star item\n1. Numbered\n2) Paren";
        assert_eq!(count_facts(content), 4);
    }

    #[test]
    fn test_count_facts_indented() {
        let content = "# Title\n- Top level\n  - Nested\n    - Deep nested";
        assert_eq!(count_facts(content), 3);
    }

    #[test]
    fn test_count_facts_empty() {
        assert_eq!(count_facts(""), 0);
    }

    #[test]
    fn test_count_facts_no_lists() {
        let content = "# Title\nJust some text\nNo lists here";
        assert_eq!(count_facts(content), 0);
    }

    #[test]
    fn test_count_facts_with_temporal_tags_all_tagged() {
        let content = "- Fact one @t[2020]\n- Fact two @t[2021..2022]\n- Fact three @t[?]";
        assert_eq!(count_facts(content), 3);
        assert_eq!(count_facts_with_temporal_tags(content), 3);
    }

    #[test]
    fn test_count_facts_with_temporal_tags_partial() {
        let content = "- Tagged fact @t[2020]\n- Untagged fact\n- Another tagged @t[~2024-01]";
        assert_eq!(count_facts(content), 3);
        assert_eq!(count_facts_with_temporal_tags(content), 2);
    }

    #[test]
    fn test_count_facts_with_temporal_tags_none() {
        let content = "- Fact one\n- Fact two\n- Fact three";
        assert_eq!(count_facts(content), 3);
        assert_eq!(count_facts_with_temporal_tags(content), 0);
    }

    #[test]
    fn test_calculate_fact_stats_full_coverage() {
        let content = "- Fact @t[2020]\n- Fact @t[2021]";
        let stats = calculate_fact_stats(content);
        assert_eq!(stats.total_facts, 2);
        assert_eq!(stats.facts_with_tags, 2);
        assert!((stats.coverage - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_calculate_fact_stats_partial_coverage() {
        let content = "- Tagged @t[2020]\n- Untagged\n- Tagged @t[2021]\n- Untagged";
        let stats = calculate_fact_stats(content);
        assert_eq!(stats.total_facts, 4);
        assert_eq!(stats.facts_with_tags, 2);
        assert!((stats.coverage - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_calculate_fact_stats_no_facts() {
        let content = "# Title\nJust text, no lists";
        let stats = calculate_fact_stats(content);
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert!((stats.coverage - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_calculate_fact_stats_empty() {
        let stats = calculate_fact_stats("");
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert!((stats.coverage - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_malformed_tags_not_counted_as_temporal() {
        let content =
            "- Fact @t[traditional..modern]\n- Fact @t[static]\n- Valid @t[2024]\n- No tag";
        let stats = calculate_fact_stats(content);
        assert_eq!(stats.total_facts, 4);
        assert_eq!(stats.facts_with_tags, 1, "Only valid @t[2024] should count");
    }
}
