//! Source reference parsing.
//!
//! This module handles parsing `[^N]` source footnotes and their definitions
//! from document content.

use crate::models::{SourceDefinition, SourceReference};
use crate::patterns::{
    DATE_EXTRACT_REGEX, FACT_LINE_REGEX, SOURCE_DEF_REGEX, SOURCE_REF_CAPTURE_REGEX,
    SOURCE_REF_DETECT_REGEX,
};

/// Parse all inline source references `[^N]` from document content.
/// Returns a Vec of SourceReference with line numbers (1-indexed), sorted by line number.
/// Ignores references inside code blocks (fenced with ``` or indented 4+ spaces).
pub fn parse_source_references(content: &str) -> Vec<SourceReference> {
    let mut refs = Vec::new();
    let mut in_code_block = false;

    for (line_idx, line) in content.lines().enumerate() {
        let line_number = line_idx + 1; // 1-indexed

        // Track fenced code blocks
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        // Skip lines inside code blocks
        if in_code_block {
            continue;
        }

        // Skip indented code blocks (4+ spaces or tab)
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }

        // Skip footnote definitions (they start with [^N]:)
        if line.trim_start().starts_with("[^") && line.contains("]:") {
            continue;
        }

        // Find all source references on this line
        for cap in SOURCE_REF_CAPTURE_REGEX.captures_iter(line) {
            if let Some(num_match) = cap.get(1) {
                if let Ok(number) = num_match.as_str().parse::<u32>() {
                    refs.push(SourceReference {
                        number,
                        line_number,
                    });
                }
            }
        }
    }

    // Sort by line number for consistent output
    refs.sort_by_key(|r| (r.line_number, r.number));
    refs
}

/// Parse all footnote definitions `[^N]: ...` from document content.
/// Returns a Vec of SourceDefinition with parsed components, sorted by footnote number.
/// Handles multi-line definitions (continuation lines starting with whitespace).
pub fn parse_source_definitions(content: &str) -> Vec<SourceDefinition> {
    let mut defs = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let line_number = i + 1; // 1-indexed

        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let number: u32 = cap[1].parse().unwrap_or(0);
            let mut full_text = cap[2].to_string();

            // Collect continuation lines (indented lines following the definition)
            let mut j = i + 1;
            while j < lines.len() {
                let next_line = lines[j];
                // Continuation lines start with whitespace but aren't empty
                if (next_line.starts_with("  ") || next_line.starts_with('\t'))
                    && !next_line.trim().is_empty()
                    && !SOURCE_DEF_REGEX.is_match(next_line)
                {
                    full_text.push(' ');
                    full_text.push_str(next_line.trim());
                    j += 1;
                } else {
                    break;
                }
            }

            let (source_type, context) = parse_source_type(&full_text);
            let date = extract_source_date(&full_text);

            defs.push(SourceDefinition {
                number,
                source_type,
                context,
                date,
                line_number,
            });

            i = j; // Skip past continuation lines
        } else {
            i += 1;
        }
    }

    // Sort by footnote number for consistent output
    defs.sort_by_key(|d| d.number);
    defs
}

/// Extract source type from definition text.
/// Returns (source_type, remaining_context).
fn parse_source_type(text: &str) -> (String, String) {
    // Standard source types from fact-document-format.md
    let type_patterns = [
        ("LinkedIn profile", "LinkedIn"),
        ("Company website", "Website"),
        ("Press release", "Press release"),
        ("News article", "News"),
        ("Public filing", "Filing"),
        ("Author knowledge", "Author knowledge"),
        ("Direct conversation", "Direct"),
        ("Email from", "Email"),
        ("Conference bio", "Event"),
        ("Speaker bio", "Event"),
        ("Slack #", "Slack"),
        ("Slack message", "Slack"),
        ("Inferred from", "Inferred"),
        ("Unverified", "Unverified"),
        ("Public records", "Filing"),
    ];

    let text_lower = text.to_lowercase();

    for (pattern, source_type) in type_patterns {
        if text_lower.starts_with(&pattern.to_lowercase()) {
            return (source_type.to_string(), text.to_string());
        }
    }

    // Try to extract type from comma-separated format: "Type, context"
    if let Some(comma_pos) = text.find(',') {
        let potential_type = text[..comma_pos].trim();
        // Only use as type if it's reasonably short (not a full sentence)
        if potential_type.len() <= 50 && !potential_type.contains(' ')
            || potential_type.split_whitespace().count() <= 3
        {
            return (potential_type.to_string(), text.to_string());
        }
    }

    // Default: use "Unknown" as type, full text as context
    ("Unknown".to_string(), text.to_string())
}

/// Extract date from source definition text.
/// Looks for dates in formats: YYYY-MM-DD, YYYY-MM, YYYY
/// Returns the most specific (longest) date found.
pub fn extract_source_date(text: &str) -> Option<String> {
    let mut best_date: Option<String> = None;

    for cap in DATE_EXTRACT_REGEX.captures_iter(text) {
        let date = cap[1].to_string();
        // Prefer more specific dates (longer = more specific)
        if best_date.as_ref().is_none_or(|d| date.len() > d.len()) {
            best_date = Some(date);
        }
    }

    best_date
}

/// Count facts that have at least one source reference on the same line.
pub fn count_facts_with_sources(content: &str) -> usize {
    content
        .lines()
        .filter(|line| FACT_LINE_REGEX.is_match(line) && SOURCE_REF_DETECT_REGEX.is_match(line))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Source Reference Parsing Tests ====================

    #[test]
    fn test_source_ref_single() {
        let content = "- Fact here [^1]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 1);
        assert_eq!(refs[0].line_number, 1);
    }

    #[test]
    fn test_source_ref_multiple_per_line() {
        let content = "- Fact with multiple sources [^1][^2]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 1);
        assert_eq!(refs[1].number, 2);
        assert_eq!(refs[0].line_number, 1);
        assert_eq!(refs[1].line_number, 1);
    }

    #[test]
    fn test_source_ref_multiple_lines() {
        let content = "- First fact [^1]\n- Second fact [^2]\n- Third fact [^3]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].line_number, 1);
        assert_eq!(refs[1].line_number, 2);
        assert_eq!(refs[2].line_number, 3);
    }

    #[test]
    fn test_source_ref_skip_fenced_code_block() {
        let content = "- Fact [^1]\n```\ncode [^2]\n```\n- Another [^3]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 1);
        assert_eq!(refs[1].number, 3);
    }

    #[test]
    fn test_source_ref_skip_indented_code_block() {
        let content = "- Fact [^1]\n    code [^2]\n- Another [^3]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 1);
        assert_eq!(refs[1].number, 3);
    }

    #[test]
    fn test_source_ref_skip_definitions() {
        let content = "- Fact [^1]\n\n[^1]: Source definition";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 1);
        assert_eq!(refs[0].line_number, 1);
    }

    #[test]
    fn test_source_ref_empty_content() {
        let refs = parse_source_references("");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_source_ref_no_refs() {
        let content = "Plain text without any references.";
        let refs = parse_source_references(content);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_source_ref_large_numbers() {
        let content = "- Fact [^99] and [^100]";
        let refs = parse_source_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 99);
        assert_eq!(refs[1].number, 100);
    }

    // ==================== Source Definition Parsing Tests ====================

    #[test]
    fn test_source_def_simple() {
        let content = "[^1]: LinkedIn profile, scraped 2024-01-15";
        let defs = parse_source_definitions(content);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].number, 1);
        assert_eq!(defs[0].source_type, "LinkedIn");
        assert_eq!(defs[0].date, Some("2024-01-15".to_string()));
        assert_eq!(defs[0].line_number, 1);
    }

    #[test]
    fn test_source_def_all_standard_types() {
        let content = r#"[^1]: LinkedIn profile, scraped 2024-01
[^2]: Company website, accessed 2024-02
[^3]: Press release, 2024-03
[^4]: News article, 2024-04
[^5]: Public filing, 2024-05
[^6]: Direct conversation, 2024-06
[^7]: Email from John, 2024-07
[^8]: Conference bio, 2024-08
[^9]: Inferred from context
[^10]: Unverified claim"#;
        let defs = parse_source_definitions(content);
        assert_eq!(defs.len(), 10);
        assert_eq!(defs[0].source_type, "LinkedIn");
        assert_eq!(defs[1].source_type, "Website");
        assert_eq!(defs[2].source_type, "Press release");
        assert_eq!(defs[3].source_type, "News");
        assert_eq!(defs[4].source_type, "Filing");
        assert_eq!(defs[5].source_type, "Direct");
        assert_eq!(defs[6].source_type, "Email");
        assert_eq!(defs[7].source_type, "Event");
        assert_eq!(defs[8].source_type, "Inferred");
        assert_eq!(defs[9].source_type, "Unverified");
    }

    #[test]
    fn test_source_def_date_extraction_formats() {
        // Full date
        let defs = parse_source_definitions("[^1]: Source, 2024-01-15");
        assert_eq!(defs[0].date, Some("2024-01-15".to_string()));

        // Year-month
        let defs = parse_source_definitions("[^1]: Source, 2024-01");
        assert_eq!(defs[0].date, Some("2024-01".to_string()));

        // Year only
        let defs = parse_source_definitions("[^1]: Source, 2024");
        assert_eq!(defs[0].date, Some("2024".to_string()));
    }

    #[test]
    fn test_source_def_date_prefers_specific() {
        // When multiple dates present, prefer most specific
        let defs = parse_source_definitions("[^1]: Source from 2024, scraped 2024-01-15");
        assert_eq!(defs[0].date, Some("2024-01-15".to_string()));
    }

    #[test]
    fn test_source_def_no_date() {
        let defs = parse_source_definitions("[^1]: Unverified claim");
        assert_eq!(defs[0].date, None);
    }

    #[test]
    fn test_source_def_multiline() {
        let content =
            "[^1]: LinkedIn profile, scraped 2024-01-15\n  Additional context on continuation line";
        let defs = parse_source_definitions(content);
        assert_eq!(defs.len(), 1);
        assert!(defs[0].context.contains("Additional context"));
    }

    #[test]
    fn test_source_def_multiple() {
        let content = "[^1]: First source\n[^2]: Second source\n[^3]: Third source";
        let defs = parse_source_definitions(content);
        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].number, 1);
        assert_eq!(defs[1].number, 2);
        assert_eq!(defs[2].number, 3);
    }

    #[test]
    fn test_source_def_sorted_by_number() {
        // Definitions out of order should be sorted
        let content = "[^3]: Third\n[^1]: First\n[^2]: Second";
        let defs = parse_source_definitions(content);
        assert_eq!(defs[0].number, 1);
        assert_eq!(defs[1].number, 2);
        assert_eq!(defs[2].number, 3);
    }

    #[test]
    fn test_source_def_empty_content() {
        let defs = parse_source_definitions("");
        assert!(defs.is_empty());
    }

    #[test]
    fn test_source_def_no_definitions() {
        let content = "Plain text without any definitions.";
        let defs = parse_source_definitions(content);
        assert!(defs.is_empty());
    }

    #[test]
    fn test_source_def_unknown_type() {
        let defs = parse_source_definitions("[^1]: Some random text without known type");
        assert_eq!(defs[0].source_type, "Unknown");
    }

    #[test]
    fn test_source_def_comma_separated_type() {
        let defs = parse_source_definitions("[^1]: Blog, posted 2024-01-15");
        assert_eq!(defs[0].source_type, "Blog");
    }

    #[test]
    fn test_source_def_speaker_bio() {
        let defs = parse_source_definitions("[^1]: Speaker bio at TechConf 2024");
        assert_eq!(defs[0].source_type, "Event");
    }

    #[test]
    fn test_source_def_public_records() {
        let defs = parse_source_definitions("[^1]: Public records, 2024-01");
        assert_eq!(defs[0].source_type, "Filing");
    }

    // ==================== Orphan Detection Tests ====================

    #[test]
    fn test_orphan_reference_detection() {
        // Reference without definition
        let content = "- Fact [^1] and [^2]\n\n[^1]: Only first defined";
        let refs = parse_source_references(content);
        let defs = parse_source_definitions(content);

        let defined_numbers: std::collections::HashSet<_> = defs.iter().map(|d| d.number).collect();
        let orphan_refs: Vec<_> = refs
            .iter()
            .filter(|r| !defined_numbers.contains(&r.number))
            .collect();

        assert_eq!(orphan_refs.len(), 1);
        assert_eq!(orphan_refs[0].number, 2);
    }

    #[test]
    fn test_orphan_definition_detection() {
        // Definition without reference
        let content = "- Fact [^1]\n\n[^1]: Used\n[^2]: Unused";
        let refs = parse_source_references(content);
        let defs = parse_source_definitions(content);

        let referenced_numbers: std::collections::HashSet<_> =
            refs.iter().map(|r| r.number).collect();
        let orphan_defs: Vec<_> = defs
            .iter()
            .filter(|d| !referenced_numbers.contains(&d.number))
            .collect();

        assert_eq!(orphan_defs.len(), 1);
        assert_eq!(orphan_defs[0].number, 2);
    }

    #[test]
    fn test_no_orphans() {
        let content = "- Fact [^1] and [^2]\n\n[^1]: First\n[^2]: Second";
        let refs = parse_source_references(content);
        let defs = parse_source_definitions(content);

        let defined_numbers: std::collections::HashSet<_> = defs.iter().map(|d| d.number).collect();
        let referenced_numbers: std::collections::HashSet<_> =
            refs.iter().map(|r| r.number).collect();

        let orphan_refs: Vec<_> = refs
            .iter()
            .filter(|r| !defined_numbers.contains(&r.number))
            .collect();
        let orphan_defs: Vec<_> = defs
            .iter()
            .filter(|d| !referenced_numbers.contains(&d.number))
            .collect();

        assert!(orphan_refs.is_empty());
        assert!(orphan_defs.is_empty());
    }

    // ==================== Source Coverage Tests ====================

    #[test]
    fn test_count_facts_with_sources_all_sourced() {
        let content = "- Fact one [^1]\n- Fact two [^2]\n- Fact three [^3]";
        assert_eq!(count_facts_with_sources(content), 3);
    }

    #[test]
    fn test_count_facts_with_sources_partial() {
        let content = "- Sourced fact [^1]\n- Unsourced fact\n- Another sourced [^2]";
        assert_eq!(count_facts_with_sources(content), 2);
    }

    #[test]
    fn test_count_facts_with_sources_none() {
        let content = "- Fact one\n- Fact two\n- Fact three";
        assert_eq!(count_facts_with_sources(content), 0);
    }

    #[test]
    fn test_count_facts_with_sources_multiple_refs() {
        let content = "- Fact with multiple sources [^1][^2]";
        assert_eq!(count_facts_with_sources(content), 1);
    }

    #[test]
    fn test_count_facts_with_sources_empty() {
        assert_eq!(count_facts_with_sources(""), 0);
    }
}
