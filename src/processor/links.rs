//! Links block parsing and manipulation.
//!
//! Supports directional link blocks:
//! ```markdown
//! References: [[abc123]] [[def456]]
//! Referenced by: [[ghi789]] [[jkl012]]
//! ```
//!
//! Backward compatible: legacy `Links:` format is treated as `References:`.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Regex matching a References: line (outbound links).
static REFERENCES_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^References:\s*(.+)$").expect("references line regex")
});

/// Regex matching a Referenced by: line (inbound links).
static REFERENCED_BY_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Referenced by:\s*(.+)$").expect("referenced by line regex")
});

/// Regex matching a legacy Links: line (treated as References:).
static LINKS_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Links:\s*(.+)$").expect("links line regex")
});

/// Regex extracting individual [[id]] from a line.
static LINK_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([a-f0-9]{6})\]\]").expect("link id regex")
});

fn extract_ids(regex: &Regex, content: &str) -> Vec<String> {
    let Some(cap) = regex.captures(content) else {
        return Vec::new();
    };
    LINK_ID_REGEX
        .captures_iter(&cap[1])
        .map(|c| c[1].to_string())
        .collect()
}

/// Parse the `References:` block (or legacy `Links:` block) from document content.
/// Returns outbound target document IDs.
pub fn parse_links_block(content: &str) -> Vec<String> {
    let refs = extract_ids(&REFERENCES_LINE_REGEX, content);
    if !refs.is_empty() {
        return refs;
    }
    // Fallback to legacy Links: format
    extract_ids(&LINKS_LINE_REGEX, content)
}

/// Parse the `Referenced by:` block from document content.
/// Returns inbound source document IDs.
pub fn parse_referenced_by_block(content: &str) -> Vec<String> {
    extract_ids(&REFERENCED_BY_LINE_REGEX, content)
}

/// Build a formatted line: `{label}: [[id1]] [[id2]]`
fn format_ids_line(label: &str, ids: &[&str]) -> String {
    format!(
        "{label}: {}",
        ids.iter()
            .map(|id| format!("[[{id}]]"))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Replace or append a line matching `regex` with `new_line` in content.
/// If `legacy_regex` is provided, also replaces that pattern (migration).
fn replace_or_append_line(
    content: &str,
    regex: &Regex,
    legacy_regex: Option<&Regex>,
    new_line: &str,
) -> String {
    if regex.is_match(content) {
        return regex.replace(content, new_line).to_string();
    }
    if let Some(legacy) = legacy_regex {
        if legacy.is_match(content) {
            return legacy.replace(content, new_line).to_string();
        }
    }
    let trimmed = content.trim_end();
    format!("{trimmed}\n\n{new_line}\n")
}

/// Append new link IDs to a document's `References:` block (or migrate legacy `Links:`).
/// Creates the block if it doesn't exist. Skips IDs already present.
pub fn append_links_to_content(content: &str, new_ids: &[&str]) -> String {
    let existing: HashSet<String> = parse_links_block(content).into_iter().collect();
    let to_add: Vec<&&str> = new_ids
        .iter()
        .filter(|id| !existing.contains(**id))
        .collect();

    if to_add.is_empty() {
        return content.to_string();
    }

    let existing_ids = parse_links_block(content);
    let mut all_ids: Vec<&str> = existing_ids.iter().map(String::as_str).collect();
    for id in &to_add {
        all_ids.push(id);
    }
    let new_line = format_ids_line("References", &all_ids);

    replace_or_append_line(
        content,
        &REFERENCES_LINE_REGEX,
        Some(&LINKS_LINE_REGEX),
        &new_line,
    )
}

/// Append new IDs to a document's `Referenced by:` block.
/// Creates the block if it doesn't exist. Skips IDs already present.
pub fn append_referenced_by_to_content(content: &str, new_ids: &[&str]) -> String {
    let existing: HashSet<String> = parse_referenced_by_block(content).into_iter().collect();
    let to_add: Vec<&&str> = new_ids
        .iter()
        .filter(|id| !existing.contains(**id))
        .collect();

    if to_add.is_empty() {
        return content.to_string();
    }

    let existing_ids = parse_referenced_by_block(content);
    let mut all_ids: Vec<&str> = existing_ids.iter().map(String::as_str).collect();
    for id in &to_add {
        all_ids.push(id);
    }
    let new_line = format_ids_line("Referenced by", &all_ids);

    replace_or_append_line(content, &REFERENCED_BY_LINE_REGEX, None, &new_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_links_block (References: / Links: backward compat) ---

    #[test]
    fn test_parse_links_block_empty() {
        assert!(parse_links_block("# Title\n\nSome content").is_empty());
    }

    #[test]
    fn test_parse_links_block_legacy_single() {
        let content = "# Title\n\nLinks: [[abc123]]";
        assert_eq!(parse_links_block(content), vec!["abc123"]);
    }

    #[test]
    fn test_parse_links_block_legacy_multiple() {
        let content = "# Title\n\nLinks: [[abc123]] [[def456]] [[aaa789]]";
        assert_eq!(
            parse_links_block(content),
            vec!["abc123", "def456", "aaa789"]
        );
    }

    #[test]
    fn test_parse_links_block_references_format() {
        let content = "# Title\n\nReferences: [[abc123]] [[def456]]";
        assert_eq!(parse_links_block(content), vec!["abc123", "def456"]);
    }

    #[test]
    fn test_parse_links_block_references_preferred_over_legacy() {
        let content = "# Title\n\nReferences: [[aaa111]]\n\nLinks: [[bbb222]]";
        assert_eq!(parse_links_block(content), vec!["aaa111"]);
    }

    #[test]
    fn test_parse_links_block_with_footnotes() {
        let content = "# Title\n\n- Fact [^1]\n\n---\n[^1]: Source\n\nReferences: [[abc123]] [[def456]]";
        assert_eq!(parse_links_block(content), vec!["abc123", "def456"]);
    }

    #[test]
    fn test_parse_links_block_ignores_inline_links() {
        let content = "# Title\n\nSee [[abc123]] for details.";
        assert!(parse_links_block(content).is_empty());
    }

    // --- parse_referenced_by_block ---

    #[test]
    fn test_parse_referenced_by_empty() {
        assert!(parse_referenced_by_block("# Title\n\nSome content").is_empty());
    }

    #[test]
    fn test_parse_referenced_by_single() {
        let content = "# Title\n\nReferenced by: [[abc123]]";
        assert_eq!(parse_referenced_by_block(content), vec!["abc123"]);
    }

    #[test]
    fn test_parse_referenced_by_multiple() {
        let content = "# Title\n\nReferenced by: [[abc123]] [[def456]]";
        assert_eq!(
            parse_referenced_by_block(content),
            vec!["abc123", "def456"]
        );
    }

    #[test]
    fn test_parse_both_blocks() {
        let content =
            "# Title\n\nReferences: [[aaa111]]\nReferenced by: [[bbb222]]";
        assert_eq!(parse_links_block(content), vec!["aaa111"]);
        assert_eq!(parse_referenced_by_block(content), vec!["bbb222"]);
    }

    // --- append_links_to_content (now writes References:) ---

    #[test]
    fn test_append_links_creates_references_block() {
        let content = "# Title\n\nSome content.";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        assert!(result.contains("References: [[abc123]] [[def456]]"));
        assert!(!result.contains("Links:"));
    }

    #[test]
    fn test_append_links_extends_existing_references() {
        let content = "# Title\n\nReferences: [[abc123]]";
        let result = append_links_to_content(content, &["def456"]);
        assert!(result.contains("References: [[abc123]] [[def456]]"));
    }

    #[test]
    fn test_append_links_migrates_legacy_to_references() {
        let content = "# Title\n\nLinks: [[abc123]]";
        let result = append_links_to_content(content, &["def456"]);
        assert!(result.contains("References: [[abc123]] [[def456]]"));
        assert!(!result.contains("Links:"));
    }

    #[test]
    fn test_append_links_skips_duplicates() {
        let content = "# Title\n\nReferences: [[abc123]]";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        let ids = parse_links_block(&result);
        assert_eq!(ids, vec!["abc123", "def456"]);
    }

    #[test]
    fn test_append_links_no_change_when_all_exist() {
        let content = "# Title\n\nReferences: [[abc123]] [[def456]]";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_append_links_after_footnotes() {
        let content = "# Title\n\n- Fact [^1]\n\n---\n[^1]: Source";
        let result = append_links_to_content(content, &["abc123"]);
        assert!(result.contains("[^1]: Source"));
        assert!(result.contains("References: [[abc123]]"));
        let footnote_pos = result.find("[^1]: Source").unwrap();
        let refs_pos = result.find("References:").unwrap();
        assert!(refs_pos > footnote_pos);
    }

    // --- append_referenced_by_to_content ---

    #[test]
    fn test_append_referenced_by_creates_block() {
        let content = "# Title\n\nSome content.";
        let result = append_referenced_by_to_content(content, &["abc123"]);
        assert!(result.contains("Referenced by: [[abc123]]"));
    }

    #[test]
    fn test_append_referenced_by_extends_existing() {
        let content = "# Title\n\nReferenced by: [[abc123]]";
        let result = append_referenced_by_to_content(content, &["def456"]);
        assert!(result.contains("Referenced by: [[abc123]] [[def456]]"));
    }

    #[test]
    fn test_append_referenced_by_skips_duplicates() {
        let content = "# Title\n\nReferenced by: [[abc123]]";
        let result = append_referenced_by_to_content(content, &["abc123", "def456"]);
        let ids = parse_referenced_by_block(&result);
        assert_eq!(ids, vec!["abc123", "def456"]);
    }

    #[test]
    fn test_append_referenced_by_no_change_when_all_exist() {
        let content = "# Title\n\nReferenced by: [[abc123]]";
        let result = append_referenced_by_to_content(content, &["abc123"]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_both_blocks_coexist() {
        let content = "# Title\n\nReferences: [[aaa111]]";
        let result = append_referenced_by_to_content(content, &["bbb222"]);
        assert!(result.contains("References: [[aaa111]]"));
        assert!(result.contains("Referenced by: [[bbb222]]"));
    }

    // --- roundtrip ---

    #[test]
    fn test_roundtrip_parse_append() {
        let content = "# Title\n\nReferences: [[aaa111]] [[bbb222]]";
        let ids = parse_links_block(content);
        assert_eq!(ids.len(), 2);
        let result = append_links_to_content(content, &["ccc333"]);
        let new_ids = parse_links_block(&result);
        assert_eq!(new_ids, vec!["aaa111", "bbb222", "ccc333"]);
    }

    #[test]
    fn test_roundtrip_referenced_by() {
        let content = "# Title\n\nReferenced by: [[aaa111]]";
        let result = append_referenced_by_to_content(content, &["bbb222"]);
        let ids = parse_referenced_by_block(&result);
        assert_eq!(ids, vec!["aaa111", "bbb222"]);
    }
}
