//! Links block parsing and manipulation.
//!
//! Handles the `Links:` block at the bottom of documents:
//! ```markdown
//! Links: [[abc123]] [[def456]] [[ghi789]]
//! ```

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Regex matching a Links: line with [[id]] references.
static LINKS_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Links:\s*(.+)$").expect("links line regex")
});

/// Regex extracting individual [[id]] from a Links: line.
static LINK_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([a-f0-9]{6})\]\]").expect("link id regex")
});

/// Parse the `Links:` block from document content.
/// Returns a list of target document IDs found in the block.
pub fn parse_links_block(content: &str) -> Vec<String> {
    let Some(cap) = LINKS_LINE_REGEX.captures(content) else {
        return Vec::new();
    };
    let rest = &cap[1];
    LINK_ID_REGEX
        .captures_iter(rest)
        .map(|c| c[1].to_string())
        .collect()
}

/// Append new link IDs to a document's `Links:` block.
/// Creates the block if it doesn't exist. Skips IDs already present.
/// Returns the modified content.
pub fn append_links_to_content(content: &str, new_ids: &[&str]) -> String {
    let existing: HashSet<String> = parse_links_block(content).into_iter().collect();
    let to_add: Vec<&&str> = new_ids
        .iter()
        .filter(|id| !existing.contains(**id))
        .collect();

    if to_add.is_empty() {
        return content.to_string();
    }

    // Build the full set of IDs for the new Links: line
    let existing_ids = parse_links_block(content);
    let mut all_ids: Vec<&str> = existing_ids.iter().map(String::as_str).collect();
    for id in &to_add {
        all_ids.push(id);
    }
    let links_line = format!(
        "Links: {}",
        all_ids
            .iter()
            .map(|id| format!("[[{id}]]"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    if LINKS_LINE_REGEX.is_match(content) {
        // Replace existing Links: line
        LINKS_LINE_REGEX
            .replace(content, links_line.as_str())
            .to_string()
    } else {
        // Append after footnotes or at end
        let trimmed = content.trim_end();
        format!("{trimmed}\n\n{links_line}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_links_block_empty() {
        assert!(parse_links_block("# Title\n\nSome content").is_empty());
    }

    #[test]
    fn test_parse_links_block_single() {
        let content = "# Title\n\nLinks: [[abc123]]";
        assert_eq!(parse_links_block(content), vec!["abc123"]);
    }

    #[test]
    fn test_parse_links_block_multiple() {
        let content = "# Title\n\nLinks: [[abc123]] [[def456]] [[aaa789]]";
        assert_eq!(
            parse_links_block(content),
            vec!["abc123", "def456", "aaa789"]
        );
    }

    #[test]
    fn test_parse_links_block_with_footnotes() {
        let content = "# Title\n\n- Fact [^1]\n\n---\n[^1]: Source\n\nLinks: [[abc123]] [[def456]]";
        assert_eq!(parse_links_block(content), vec!["abc123", "def456"]);
    }

    #[test]
    fn test_parse_links_block_ignores_inline_links() {
        // [[id]] in body text should NOT be parsed as Links: block
        let content = "# Title\n\nSee [[abc123]] for details.";
        assert!(parse_links_block(content).is_empty());
    }

    #[test]
    fn test_append_links_creates_block() {
        let content = "# Title\n\nSome content.";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        assert!(result.contains("Links: [[abc123]] [[def456]]"));
    }

    #[test]
    fn test_append_links_extends_existing() {
        let content = "# Title\n\nLinks: [[abc123]]";
        let result = append_links_to_content(content, &["def456"]);
        assert!(result.contains("Links: [[abc123]] [[def456]]"));
    }

    #[test]
    fn test_append_links_skips_duplicates() {
        let content = "# Title\n\nLinks: [[abc123]]";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        let ids = parse_links_block(&result);
        assert_eq!(ids, vec!["abc123", "def456"]);
    }

    #[test]
    fn test_append_links_no_change_when_all_exist() {
        let content = "# Title\n\nLinks: [[abc123]] [[def456]]";
        let result = append_links_to_content(content, &["abc123", "def456"]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_append_links_after_footnotes() {
        let content = "# Title\n\n- Fact [^1]\n\n---\n[^1]: Source";
        let result = append_links_to_content(content, &["abc123"]);
        assert!(result.contains("[^1]: Source"));
        assert!(result.contains("Links: [[abc123]]"));
        // Links should come after footnotes
        let footnote_pos = result.find("[^1]: Source").unwrap();
        let links_pos = result.find("Links:").unwrap();
        assert!(links_pos > footnote_pos);
    }

    #[test]
    fn test_roundtrip_parse_append() {
        let content = "# Title\n\nLinks: [[aaa111]] [[bbb222]]";
        let ids = parse_links_block(content);
        assert_eq!(ids.len(), 2);
        let result = append_links_to_content(content, &["ccc333"]);
        let new_ids = parse_links_block(&result);
        assert_eq!(new_ids, vec!["aaa111", "bbb222", "ccc333"]);
    }
}
