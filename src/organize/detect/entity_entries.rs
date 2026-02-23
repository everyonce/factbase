//! Entity entry extraction from markdown documents.
//!
//! Identifies named entity blocks within documents — e.g., person entries
//! under a `## Team` section in a company doc. An entity entry is a heading
//! (H3+) or bold-name list item followed by child facts.

use crate::patterns::FACT_LINE_REGEX;
use regex::Regex;
use std::sync::LazyLock;

/// Regex for bold-name list items: `- **Name** ...` or `- **Name**: ...`
static BOLD_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*[-*]\s+\*\*([^*]+)\*\*").expect("bold name regex should be valid")
});

/// A named entity block within a document (e.g., a person listed under a team section).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityEntry {
    /// The entity name (heading text or bold name).
    pub name: String,
    /// Parent document ID.
    pub doc_id: String,
    /// Parent H2 section name (empty if no parent section).
    pub section: String,
    /// Child list items (fact text).
    pub facts: Vec<String>,
    /// Start line (1-indexed, inclusive).
    pub line_start: usize,
    /// End line (1-indexed, inclusive).
    pub line_end: usize,
}

/// Extract entity entries from markdown content.
///
/// Identifies two patterns:
/// 1. H3+ headings under H2 sections, with child list items as facts
/// 2. Bold-name list items (`- **Name** - description`) as standalone entries
///
/// Returns entries sorted by line_start.
pub fn extract_entity_entries(content: &str, doc_id: &str) -> Vec<EntityEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut entries = Vec::new();
    let mut current_section = String::new();
    let mut current_entry: Option<EntityEntry> = None;

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        // Skip factbase header
        if line.starts_with("<!-- factbase:") {
            continue;
        }

        // Track H2 sections
        if line.starts_with("## ") && !line.starts_with("### ") {
            // Finalize any open entry
            if let Some(entry) = finalize_entry(current_entry.take(), line_num) {
                entries.push(entry);
            }
            current_section = line.trim_start_matches('#').trim().to_string();
            continue;
        }

        // H1 resets section context (document title, not a grouping section)
        if line.starts_with("# ") && !line.starts_with("## ") {
            if let Some(entry) = finalize_entry(current_entry.take(), line_num) {
                entries.push(entry);
            }
            current_section.clear();
            continue;
        }

        // Check for H3+ heading (entity entry via heading)
        if let Some(name) = parse_sub_heading(line) {
            if let Some(entry) = finalize_entry(current_entry.take(), line_num) {
                entries.push(entry);
            }
            current_entry = Some(EntityEntry {
                name,
                doc_id: doc_id.to_string(),
                section: current_section.clone(),
                facts: Vec::new(),
                line_start: line_num,
                line_end: line_num,
            });
            continue;
        }

        // Check for bold-name list item (entity entry via bold name)
        if let Some(name) = extract_bold_name(line) {
            if let Some(entry) = finalize_entry(current_entry.take(), line_num) {
                entries.push(entry);
            }
            current_entry = Some(EntityEntry {
                name,
                doc_id: doc_id.to_string(),
                section: current_section.clone(),
                facts: vec![line.trim().to_string()],
                line_start: line_num,
                line_end: line_num,
            });
            continue;
        }

        // Collect child facts for current entry
        if let Some(ref mut entry) = current_entry {
            if FACT_LINE_REGEX.is_match(line) {
                entry.facts.push(line.trim().to_string());
                entry.line_end = line_num;
            } else if !line.trim().is_empty() && !line.starts_with("  ") {
                // Non-empty, non-indented, non-list line ends the entry
                if let Some(entry) = finalize_entry(current_entry.take(), line_num) {
                    entries.push(entry);
                }
            }
            // Blank lines within an entry are tolerated
        }
    }

    // Finalize last entry
    let total_lines = lines.len() + 1;
    if let Some(entry) = finalize_entry(current_entry.take(), total_lines) {
        entries.push(entry);
    }

    entries
}

/// Finalize an entry: only keep it if it has at least one fact.
fn finalize_entry(entry: Option<EntityEntry>, next_line: usize) -> Option<EntityEntry> {
    entry.and_then(|mut e| {
        if e.facts.is_empty() {
            return None;
        }
        // Adjust end line to just before the next element
        if next_line > 1 {
            e.line_end = e.line_end.max(e.line_start);
        }
        Some(e)
    })
}

/// Parse an H3+ heading, returning the heading text.
fn parse_sub_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("###") {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if !(3..=6).contains(&level) {
        return None;
    }
    let title = trimmed[level..].trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(title)
}

/// Extract a bold name from a list item like `- **Jane Smith** - VP Engineering`.
fn extract_bold_name(line: &str) -> Option<String> {
    BOLD_NAME_REGEX
        .captures(line)
        .map(|caps| caps[1].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_entries_h3_under_h2() {
        let content = "\
<!-- factbase:abc123 -->
# Acme Corp

## Team

### Jane Smith
- VP Engineering @t[2022..]
- Previously at BigCo

### Bob Jones
- CTO @t[2020..]
- Founded startup X

## Products

Some product info.
";
        let entries = extract_entity_entries(content, "abc123");
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].name, "Jane Smith");
        assert_eq!(entries[0].doc_id, "abc123");
        assert_eq!(entries[0].section, "Team");
        assert_eq!(entries[0].facts.len(), 2);
        assert!(entries[0].facts[0].contains("VP Engineering"));

        assert_eq!(entries[1].name, "Bob Jones");
        assert_eq!(entries[1].section, "Team");
        assert_eq!(entries[1].facts.len(), 2);
    }

    #[test]
    fn test_extract_entries_bold_name_list() {
        let content = "\
# Globex Team

## Members

- **Jane Smith** - VP Engineering
- **Bob Jones** - CTO
";
        let entries = extract_entity_entries(content, "def456");
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].name, "Jane Smith");
        assert_eq!(entries[0].section, "Members");
        assert_eq!(entries[0].facts.len(), 1);
        assert!(entries[0].facts[0].contains("Jane Smith"));

        assert_eq!(entries[1].name, "Bob Jones");
    }

    #[test]
    fn test_extract_entries_empty_doc() {
        let content = "# Simple Doc\n\nJust some text, no sub-entries.\n";
        let entries = extract_entity_entries(content, "aaa111");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_extract_entries_no_facts_skipped() {
        let content = "\
# Doc

## Section

### Empty Person

## Next Section
";
        let entries = extract_entity_entries(content, "bbb222");
        assert!(
            entries.is_empty(),
            "Entries without facts should be skipped"
        );
    }

    #[test]
    fn test_extract_entries_line_numbers() {
        let content = "\
# Title
## Team
### Alice
- Fact 1
- Fact 2
### Bob
- Fact 3
";
        let entries = extract_entity_entries(content, "ccc333");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line_start, 3);
        assert_eq!(entries[1].line_start, 6);
    }

    #[test]
    fn test_parse_sub_heading() {
        assert_eq!(parse_sub_heading("### Person"), Some("Person".to_string()));
        assert_eq!(
            parse_sub_heading("#### Deep Heading"),
            Some("Deep Heading".to_string())
        );
        assert_eq!(parse_sub_heading("## Not Sub"), None);
        assert_eq!(parse_sub_heading("# Title"), None);
        assert_eq!(parse_sub_heading("###"), None);
        assert_eq!(parse_sub_heading("Not a heading"), None);
    }

    #[test]
    fn test_extract_bold_name() {
        assert_eq!(
            extract_bold_name("- **Jane Smith** - VP"),
            Some("Jane Smith".to_string())
        );
        assert_eq!(
            extract_bold_name("* **Bob Jones**: CTO"),
            Some("Bob Jones".to_string())
        );
        assert_eq!(extract_bold_name("- Regular item"), None);
        assert_eq!(extract_bold_name("Not a list"), None);
    }

    #[test]
    fn test_extract_entries_mixed_h3_and_bold() {
        let content = "\
# Company

## Leadership

### CEO
- John Doe @t[2020..]

## Advisors

- **Alice Wang** - Technical advisor
- **Charlie Brown** - Legal advisor
";
        let entries = extract_entity_entries(content, "ddd444");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "CEO");
        assert_eq!(entries[0].section, "Leadership");
        assert_eq!(entries[1].name, "Alice Wang");
        assert_eq!(entries[1].section, "Advisors");
        assert_eq!(entries[2].name, "Charlie Brown");
        assert_eq!(entries[2].section, "Advisors");
    }

    #[test]
    fn test_extract_entries_h3_with_child_list() {
        let content = "\
## Team

### Jane Smith
- Role: VP Engineering
- Started: 2022
- Reports to: CEO
";
        let entries = extract_entity_entries(content, "eee555");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].facts.len(), 3);
    }
}
