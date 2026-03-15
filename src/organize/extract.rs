//! Fact extraction from markdown documents.
//!
//! Parses document content into discrete facts for tracking through
//! reorganization operations.

use crate::organize::TrackedFact;
use crate::patterns::{FACT_LINE_REGEX, SOURCE_REF_CAPTURE_REGEX, TEMPORAL_TAG_CONTENT_REGEX};

/// Extract discrete facts from markdown content.
///
/// Facts are identified as:
/// - List items (`- text`, `* text`, `1. text`)
/// - Non-empty paragraphs (text blocks separated by blank lines)
/// - Headers are treated as context, not facts themselves
///
/// Each fact preserves:
/// - Temporal tags (`@t[...]`)
/// - Source footnote references (`[^n]`)
pub fn extract_facts(content: &str, doc_id: &str) -> Vec<TrackedFact> {
    let mut facts = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Skip YAML frontmatter
    let fm_lines = crate::patterns::frontmatter_line_count(content);
    let mut i = fm_lines;

    while i < lines.len() {
        let line = lines[i];
        let line_num = i + 1; // 1-indexed

        // Skip empty lines, headers, factbase ID, and footnote definitions
        if line.trim().is_empty()
            || line.starts_with('#')
            || line.starts_with("---")
            || line.starts_with("[^")
        {
            i += 1;
            continue;
        }

        // Check if this is a list item (fact line)
        if FACT_LINE_REGEX.is_match(line) {
            let (temporal, sources) = extract_metadata(line);
            facts.push(TrackedFact::new(
                doc_id,
                line_num,
                line.trim(),
                temporal,
                sources,
            ));
            i += 1;
            continue;
        }

        // Otherwise, treat as paragraph - collect until blank line or list/header
        let mut para_lines = vec![line];
        let para_start = line_num;
        i += 1;

        while i < lines.len() {
            let next = lines[i];
            if next.trim().is_empty()
                || next.starts_with('#')
                || FACT_LINE_REGEX.is_match(next)
                || next.starts_with("[^")
                || next.starts_with("---")
            {
                break;
            }
            para_lines.push(next);
            i += 1;
        }

        let para_content = para_lines.join(" ");
        if !para_content.trim().is_empty() {
            let (temporal, sources) = extract_metadata(&para_content);
            facts.push(TrackedFact::new(
                doc_id,
                para_start,
                &para_content,
                temporal,
                sources,
            ));
        }
    }

    facts
}

/// Extract temporal tag and source references from text.
fn extract_metadata(text: &str) -> (Option<String>, Vec<String>) {
    // Extract temporal tag
    let temporal = TEMPORAL_TAG_CONTENT_REGEX
        .captures(text)
        .map(|c| format!("@t[{}]", c.get(1).map_or("", |m| m.as_str())));

    // Extract source references
    let sources: Vec<String> = SOURCE_REF_CAPTURE_REGEX
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    (temporal, sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_list() {
        let content = "# Title\n\n- Fact one\n- Fact two\n- Fact three";
        let facts = extract_facts(content, "abc123");

        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].content, "- Fact one");
        assert_eq!(facts[0].source_line, 3);
        assert_eq!(facts[1].content, "- Fact two");
        assert_eq!(facts[2].content, "- Fact three");
    }

    #[test]
    fn test_extract_with_temporal_tags() {
        let content = "- CTO at Acme @t[2020..2022]\n- VP at BigCo @t[2022..]";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].temporal, Some("@t[2020..2022]".to_string()));
        assert_eq!(facts[1].temporal, Some("@t[2022..]".to_string()));
    }

    #[test]
    fn test_extract_with_footnotes() {
        let content = "- Founded company [^1]\n- Sold company [^2]\n\n---\n[^1]: LinkedIn\n[^2]: Press release";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].sources, vec!["1".to_string()]);
        assert_eq!(facts[1].sources, vec!["2".to_string()]);
    }

    #[test]
    fn test_extract_with_multiple_footnotes() {
        let content = "- Complex fact [^1] [^2]";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].sources, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn test_extract_paragraph() {
        let content = "# Title\n\nThis is a paragraph\nspanning multiple lines.\n\n- List item";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 2);
        assert_eq!(
            facts[0].content,
            "This is a paragraph spanning multiple lines."
        );
        assert_eq!(facts[0].source_line, 3);
        assert_eq!(facts[1].content, "- List item");
    }

    #[test]
    fn test_extract_skips_headers() {
        let content = "# Main Title\n\n## Section\n\n- Fact";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "- Fact");
    }

    #[test]
    fn test_extract_skips_factbase_header() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n\n- Fact";
        let facts = extract_facts(content, "abc123");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "- Fact");
    }

    #[test]
    fn test_extract_skips_footnote_definitions() {
        let content = "- Fact [^1]\n\n[^1]: Source info";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "- Fact [^1]");
    }

    #[test]
    fn test_extract_numbered_list() {
        let content = "1. First item\n2. Second item\n3) Third item";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].content, "1. First item");
        assert_eq!(facts[1].content, "2. Second item");
        assert_eq!(facts[2].content, "3) Third item");
    }

    #[test]
    fn test_extract_asterisk_list() {
        let content = "* Item one\n* Item two";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].content, "* Item one");
        assert_eq!(facts[1].content, "* Item two");
    }

    #[test]
    fn test_extract_mixed_content() {
        let content = r#"---\nfactbase_id: abc123\n---
# Person Name

Software engineer with 10 years experience.

## Career

- CTO at Acme @t[2020..2022] [^1]
- VP Engineering at BigCo @t[2022..] [^2]

## Education

- PhD Computer Science

---
[^1]: LinkedIn profile
[^2]: Press release"#;

        let facts = extract_facts(content, "abc123");

        assert_eq!(facts.len(), 4);
        assert_eq!(
            facts[0].content,
            "Software engineer with 10 years experience."
        );
        assert_eq!(facts[1].content, "- CTO at Acme @t[2020..2022] [^1]");
        assert_eq!(facts[1].temporal, Some("@t[2020..2022]".to_string()));
        assert_eq!(facts[1].sources, vec!["1".to_string()]);
        assert_eq!(
            facts[2].content,
            "- VP Engineering at BigCo @t[2022..] [^2]"
        );
        assert_eq!(facts[3].content, "- PhD Computer Science");
    }

    #[test]
    fn test_extract_empty_content() {
        let facts = extract_facts("", "doc1");
        assert!(facts.is_empty());
    }

    #[test]
    fn test_extract_only_headers() {
        let content = "# Title\n\n## Section\n\n### Subsection";
        let facts = extract_facts(content, "doc1");
        assert!(facts.is_empty());
    }

    #[test]
    fn test_extract_preserves_doc_id() {
        let content = "- Fact";
        let facts = extract_facts(content, "xyz789");

        assert_eq!(facts[0].source_doc, "xyz789");
    }

    #[test]
    fn test_extract_indented_list() {
        let content = "  - Indented fact\n    - Nested fact";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].content, "- Indented fact");
        assert_eq!(facts[1].content, "- Nested fact");
    }

    #[test]
    fn test_extract_temporal_unknown() {
        let content = "- Unverified fact @t[?]";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].temporal, Some("@t[?]".to_string()));
    }

    #[test]
    fn test_extract_temporal_point_in_time() {
        let content = "- Founded company @t[=2019-06]";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts[0].temporal, Some("@t[=2019-06]".to_string()));
    }

    #[test]
    fn test_extract_temporal_last_known() {
        let content = "- Lives in Austin @t[~2024-01]";
        let facts = extract_facts(content, "doc1");

        assert_eq!(facts[0].temporal, Some("@t[~2024-01]".to_string()));
    }
}
