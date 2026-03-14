//! Fact extraction for cross-document validation.
//!
//! Extracts ALL list items from markdown content (any indentation level),
//! not just temporally-tagged ones. Used by cross-validation to search
//! each fact against the rest of the factbase.

use crate::patterns::{
    extract_frontmatter_reviewed_date, extract_reviewed_date, FACT_LINE_REGEX,
    SOURCE_REF_CAPTURE_REGEX,
};

/// A single fact line extracted from a document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FactLine {
    /// 1-indexed line number in the source document.
    pub line_number: usize,
    /// Cleaned fact text (bullets, checkboxes, and leading whitespace removed).
    /// NOT truncated — full text is needed for embedding generation.
    pub text: String,
    /// The `## H2` section heading this fact appears under, if any.
    pub section: Option<String>,
    /// Footnote reference numbers (`[^N]`) found on this line.
    pub source_refs: Vec<u32>,
}

/// Extract all fact lines (list items at any indentation level) from markdown content.
///
/// Tracks `## H2` section headings so each fact knows which section it belongs to.
/// Reuses `FACT_LINE_REGEX` for list-item detection and strips markdown bullets,
/// numbered markers, and checkbox markers (`[ ]`, `[x]`, `[X]`).
///
/// Skips lines with recent `<!-- reviewed:YYYY-MM-DD -->` markers (within 180 days)
/// to avoid re-validating facts that have already been reviewed.
pub(crate) fn extract_all_facts(content: &str) -> Vec<FactLine> {
    let mut facts = Vec::new();
    let mut current_section: Option<String> = None;
    let today = chrono::Local::now().date_naive();
    const REVIEWED_SKIP_DAYS: i64 = 180;

    // Check frontmatter for document-level reviewed date (obsidian format)
    let fm_reviewed = extract_frontmatter_reviewed_date(content)
        .filter(|d| (today - *d).num_days() <= REVIEWED_SKIP_DAYS);

    // Stop before the review queue section; skip YAML frontmatter lines
    let end = crate::patterns::body_end_offset(content);
    let fm_lines = crate::patterns::frontmatter_line_count(content);

    for (line_idx, line) in content[..end].lines().enumerate() {
        // Track section headings
        if line.starts_with("## ") {
            current_section = Some(line.trim_start_matches('#').trim().to_string());
            continue;
        }

        // Skip YAML frontmatter (metadata, not facts)
        if line_idx < fm_lines {
            continue;
        }

        if !FACT_LINE_REGEX.is_match(line) {
            continue;
        }

        // Skip facts with a recent reviewed marker (inline or frontmatter)
        if fm_reviewed.is_some() {
            continue;
        }
        if extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        let text = clean_fact_text(line);
        if text.is_empty() {
            continue;
        }

        let source_refs: Vec<u32> = SOURCE_REF_CAPTURE_REGEX
            .captures_iter(line)
            .filter_map(|c| c[1].parse().ok())
            .collect();

        facts.push(FactLine {
            line_number: line_idx + 1,
            text,
            section: current_section.clone(),
            source_refs,
        });
    }

    facts
}

/// Clean a list-item line by removing the bullet/number marker and any checkbox prefix.
/// Does NOT truncate — returns the full text for embedding use.
fn clean_fact_text(line: &str) -> String {
    let trimmed = line.trim();

    // Remove list markers: -, *, 1., 1)
    let text = if let Some(rest) = trimmed.strip_prefix("- ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("* ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(rest) = rest.strip_prefix(". ") {
            rest
        } else if let Some(rest) = rest.strip_prefix(") ") {
            rest
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    // Remove checkbox markers: [ ], [x], [X]
    let text = text
        .strip_prefix("[ ] ")
        .or_else(|| text.strip_prefix("[x] "))
        .or_else(|| text.strip_prefix("[X] "))
        .unwrap_or(text);

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- clean_fact_text tests ---

    #[test]
    fn test_clean_fact_text_formats() {
        // All list marker formats
        assert_eq!(clean_fact_text("- Simple fact"), "Simple fact");
        assert_eq!(clean_fact_text("* Another fact"), "Another fact");
        assert_eq!(clean_fact_text("1. Numbered fact"), "Numbered fact");
        assert_eq!(clean_fact_text("2) Paren fact"), "Paren fact");
        assert_eq!(clean_fact_text("- [ ] Todo item"), "Todo item");
        assert_eq!(clean_fact_text("- [x] Done item"), "Done item");
        assert_eq!(clean_fact_text("- [X] Done item"), "Done item");
        assert_eq!(clean_fact_text("  - Indented fact"), "Indented fact");
        // Does not truncate long text
        let long = "- ".to_string() + &"x".repeat(200);
        let result = clean_fact_text(&long);
        assert_eq!(result.len(), 200);
        assert!(!result.ends_with("..."));
    }

    // --- extract_all_facts tests ---

    #[test]
    fn test_extract_plain_list_items() {
        let content = "# Title\n\n- Fact one\n- Fact two";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].text, "Fact one");
        assert_eq!(facts[0].line_number, 3);
        assert_eq!(facts[0].section, None);
        assert_eq!(facts[1].text, "Fact two");
        assert_eq!(facts[1].line_number, 4);
    }

    #[test]
    fn test_extract_nested_items() {
        let content = "- Top level\n  - Nested level\n    - Deep nested";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].text, "Top level");
        assert_eq!(facts[1].text, "Nested level");
        assert_eq!(facts[2].text, "Deep nested");
    }

    #[test]
    fn test_extract_with_temporal_tags() {
        let content = "- VP Engineering @t[2020..]\n- Based in Seattle";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].text, "VP Engineering @t[2020..]");
        assert_eq!(facts[1].text, "Based in Seattle");
    }

    #[test]
    fn test_extract_without_temporal_tags() {
        let content = "- No tags here\n- Also no tags";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn test_extract_section_tracking() {
        let content = "## Career\n\n- Job one\n\n## Education\n\n- Degree one";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].section, Some("Career".to_string()));
        assert_eq!(facts[0].text, "Job one");
        assert_eq!(facts[1].section, Some("Education".to_string()));
        assert_eq!(facts[1].text, "Degree one");
    }

    #[test]
    fn test_extract_non_list_lines_excluded() {
        let content = "# Title\n\nParagraph text here.\n\n- Only fact\n\nMore paragraph.";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Only fact");
    }

    #[test]
    fn test_extract_mixed_markers() {
        let content = "- Dash item\n* Star item\n1. Numbered item";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].text, "Dash item");
        assert_eq!(facts[1].text, "Star item");
        assert_eq!(facts[2].text, "Numbered item");
    }

    #[test]
    fn test_extract_section_persists_until_next() {
        let content = "## Section A\n\n- Fact A1\n- Fact A2\n\n## Section B\n\n- Fact B1";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].section, Some("Section A".to_string()));
        assert_eq!(facts[1].section, Some("Section A".to_string()));
        assert_eq!(facts[2].section, Some("Section B".to_string()));
    }

    #[test]
    fn test_extract_no_section_before_first_h2() {
        let content = "# Title\n\n- Orphan fact\n\n## Section\n\n- Sectioned fact";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].section, None);
        assert_eq!(facts[1].section, Some("Section".to_string()));
    }

    #[test]
    fn test_extract_checkboxes() {
        let content = "- [ ] Unchecked\n- [x] Checked\n- [X] Also checked";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].text, "Unchecked");
        assert_eq!(facts[1].text, "Checked");
        assert_eq!(facts[2].text, "Also checked");
    }

    #[test]
    fn test_extract_empty_content() {
        let facts = extract_all_facts("");
        assert!(facts.is_empty());
    }

    #[test]
    fn test_extract_no_list_items() {
        let content = "# Title\n\nJust paragraphs.\n\nNo lists here.";
        let facts = extract_all_facts(content);
        assert!(facts.is_empty());
    }

    // --- source_refs extraction tests ---

    #[test]
    fn test_extract_source_refs_single() {
        let content = "- VP Engineering at Acme [^1]";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source_refs, vec![1]);
    }

    #[test]
    fn test_extract_source_refs_multiple() {
        let content = "- VP Engineering at Acme [^1] [^3]";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source_refs, vec![1, 3]);
    }

    #[test]
    fn test_extract_source_refs_none() {
        let content = "- No footnotes here";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 1);
        assert!(facts[0].source_refs.is_empty());
    }

    #[test]
    fn test_extract_source_refs_with_temporal() {
        let content = "- VP Engineering @t[2020..] [^2]";
        let facts = extract_all_facts(content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source_refs, vec![2]);
    }

    // --- reviewed marker skip tests ---

    #[test]
    fn test_extract_skips_recently_reviewed_lines() {
        let today = chrono::Local::now().format("%Y-%m-%d");
        let content = format!("- Reviewed fact <!-- reviewed:{today} -->\n- Unreviewed fact");
        let facts = extract_all_facts(&content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Unreviewed fact");
    }

    #[test]
    fn test_extract_includes_old_reviewed_lines() {
        // 200 days ago exceeds the 180-day skip window
        let old_date =
            (chrono::Local::now().date_naive() - chrono::Duration::days(200)).format("%Y-%m-%d");
        let content =
            format!("- Old reviewed fact <!-- reviewed:{old_date} -->\n- Unreviewed fact");
        let facts = extract_all_facts(&content);
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn test_extract_skips_all_facts_with_recent_frontmatter_reviewed() {
        let today = chrono::Local::now().format("%Y-%m-%d");
        let content = format!(
            "---\nfactbase_id: abc123\nreviewed: {today}\n---\n# Title\n\n- Fact one\n- Fact two\n"
        );
        let facts = extract_all_facts(&content);
        assert_eq!(
            facts.len(),
            0,
            "All facts should be skipped when frontmatter reviewed date is recent"
        );
    }

    #[test]
    fn test_extract_includes_facts_with_old_frontmatter_reviewed() {
        let old_date =
            (chrono::Local::now().date_naive() - chrono::Duration::days(200)).format("%Y-%m-%d");
        let content = format!(
            "---\nfactbase_id: abc123\nreviewed: {old_date}\n---\n# Title\n\n- Fact one\n- Fact two\n"
        );
        let facts = extract_all_facts(&content);
        assert_eq!(
            facts.len(),
            2,
            "Facts should not be skipped when frontmatter reviewed date is old"
        );
    }
}
