//! Document repair: auto-fix detected corruption patterns.
//!
//! Detects and fixes:
//! - Corrupted titles (temporal tags or footnote refs in H1)
//! - Garbage footnotes (review-answer text as source citations)
//! - Duplicate footnote definitions
//! - Orphaned footnote definitions (unreferenced)
//! - Duplicate fact lines

use crate::output::truncate_str;
use crate::patterns::{
    body_end_offset, FACT_LINE_REGEX, SOURCE_DEF_REGEX, SOURCE_REF_CAPTURE_REGEX,
    TEMPORAL_TAG_DETECT_REGEX,
};
use std::collections::{HashMap, HashSet};

/// Result of repairing a document.
#[derive(Debug)]
pub struct RepairResult {
    pub fixes: usize,
    pub descriptions: Vec<String>,
    /// The repaired content (None if no changes needed).
    pub content: Option<String>,
}

/// Phrases in footnote definitions that indicate review-answer text was dumped
/// as a source citation.
const GARBAGE_FOOTNOTE_PHRASES: &[&str] = &[
    "not a conflict",
    "sequential progression",
    "unable to verify",
    "no conflict",
    "confirmed correct",
    "this is correct",
    "already addressed",
    "no action needed",
    "no change needed",
    "appears accurate",
    "verified correct",
    "overlapping roles",
    "concurrent positions",
    "no issue found",
    "classification:",
    "answer_type:",
    "change_instruction",
];

/// Repair all detected corruption in a document.
pub fn repair_document(content: &str) -> RepairResult {
    let mut result = RepairResult {
        fixes: 0,
        descriptions: Vec::new(),
        content: None,
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let trailing_newline = content.ends_with('\n');

    repair_title(&mut lines, &mut result);
    repair_garbage_footnotes(&mut lines, &mut result);
    repair_duplicate_footnote_defs(&mut lines, &mut result);
    repair_orphaned_footnote_defs(&mut lines, &mut result);
    repair_duplicate_fact_lines(&mut lines, &mut result);

    if result.fixes > 0 {
        let mut repaired = lines.join("\n");
        if trailing_newline && !repaired.ends_with('\n') {
            repaired.push('\n');
        }
        result.content = Some(repaired);
    }

    result
}

fn repair_title(lines: &mut [String], result: &mut RepairResult) {
    for line in lines.iter_mut() {
        if line.starts_with("# ") && !line.starts_with("## ") {
            let original = line.clone();
            let title_part = &original[2..];

            let mut cleaned = title_part.to_string();
            if TEMPORAL_TAG_DETECT_REGEX.is_match(&cleaned) {
                cleaned = TEMPORAL_TAG_DETECT_REGEX
                    .replace_all(&cleaned, "")
                    .to_string();
            }
            if SOURCE_REF_CAPTURE_REGEX.is_match(&cleaned) {
                cleaned = SOURCE_REF_CAPTURE_REGEX
                    .replace_all(&cleaned, "")
                    .to_string();
            }
            let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
            let new_line = format!("# {cleaned}");
            if new_line != original {
                *line = new_line;
                result.fixes += 1;
                result
                    .descriptions
                    .push(format!("Cleaned title: \"{}\" → \"{}\"", original.trim(), line.trim()));
            }
            break;
        }
    }
}

fn repair_garbage_footnotes(lines: &mut Vec<String>, result: &mut RepairResult) {
    let mut to_remove = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let def_text = cap[2].to_lowercase();
            for phrase in GARBAGE_FOOTNOTE_PHRASES {
                if def_text.contains(phrase) {
                    to_remove.push(i);
                    result.fixes += 1;
                    result.descriptions.push(format!(
                        "Removed garbage footnote [^{}]: {}",
                        &cap[1],
                        truncate_str(&cap[2], 60)
                    ));
                    break;
                }
            }
        }
    }
    remove_lines(lines, &to_remove);
}

fn repair_duplicate_footnote_defs(lines: &mut Vec<String>, result: &mut RepairResult) {
    let mut seen: HashMap<u32, usize> = HashMap::new();
    let mut to_remove = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            if seen.contains_key(&num) {
                to_remove.push(i);
                result.fixes += 1;
                result
                    .descriptions
                    .push(format!("Removed duplicate footnote [^{num}] definition"));
            } else {
                seen.insert(num, i);
            }
        }
    }
    remove_lines(lines, &to_remove);
}

fn repair_orphaned_footnote_defs(lines: &mut Vec<String>, result: &mut RepairResult) {
    let content = lines.join("\n");
    let end = body_end_offset(&content);
    let body = &content[..end];

    let mut referenced: HashSet<u32> = HashSet::new();
    for line in body.lines() {
        if SOURCE_DEF_REGEX.is_match(line) {
            continue;
        }
        for cap in SOURCE_REF_CAPTURE_REGEX.captures_iter(line) {
            if let Ok(num) = cap[1].parse::<u32>() {
                referenced.insert(num);
            }
        }
    }

    let mut to_remove = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            if !referenced.contains(&num) {
                to_remove.push(i);
                result.fixes += 1;
                result
                    .descriptions
                    .push(format!("Removed orphaned footnote [^{num}] definition"));
            }
        }
    }
    remove_lines(lines, &to_remove);
}

fn repair_duplicate_fact_lines(lines: &mut Vec<String>, result: &mut RepairResult) {
    let content = lines.join("\n");
    let end = body_end_offset(&content);

    let mut char_count = 0;
    let mut body_end_line = lines.len();
    for (i, line) in lines.iter().enumerate() {
        if char_count >= end {
            body_end_line = i;
            break;
        }
        char_count += line.len() + 1;
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut to_remove = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i >= body_end_line {
            break;
        }
        if !FACT_LINE_REGEX.is_match(line) {
            continue;
        }
        let normalized = line.trim().to_string();
        if !seen.insert(normalized) {
            to_remove.push(i);
            result.fixes += 1;
            result
                .descriptions
                .push(format!("Removed duplicate fact: {}", truncate_str(line.trim(), 60)));
        }
    }
    remove_lines(lines, &to_remove);
}

fn remove_lines(lines: &mut Vec<String>, indices: &[usize]) {
    for &i in indices.iter().rev() {
        if i < lines.len() {
            lines.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repair_clean_document_no_changes() {
        let content = "<!-- factbase:abc123 -->\n# Test Entity\n\n- Fact one [^1]\n\n[^1]: Source A\n";
        let result = repair_document(content);
        assert_eq!(result.fixes, 0);
        assert!(result.content.is_none());
    }

    #[test]
    fn test_repair_corrupted_title() {
        let content = "# Test Entity @t[?] [^1]\n\n- Fact\n";
        let result = repair_document(content);
        assert!(result.fixes > 0);
        let repaired = result.content.unwrap();
        assert!(repaired.starts_with("# Test Entity\n"));
    }

    #[test]
    fn test_repair_garbage_footnote() {
        let content = "- Fact [^1]\n\n[^1]: Not a conflict, sequential progression\n";
        let result = repair_document(content);
        assert!(result.fixes > 0);
        let repaired = result.content.unwrap();
        assert!(!repaired.contains("Not a conflict"));
    }

    #[test]
    fn test_repair_duplicate_footnote_defs() {
        let content = "- Fact [^1]\n\n[^1]: Source A\n[^1]: Source B\n";
        let result = repair_document(content);
        assert!(result.fixes > 0);
        let repaired = result.content.unwrap();
        assert!(repaired.contains("[^1]: Source A"));
        assert!(!repaired.contains("[^1]: Source B"));
    }

    #[test]
    fn test_repair_orphaned_footnote() {
        let content = "- Fact without refs\n\n[^5]: Orphaned source\n";
        let result = repair_document(content);
        assert!(result.fixes > 0);
        let repaired = result.content.unwrap();
        assert!(!repaired.contains("[^5]"));
    }

    #[test]
    fn test_repair_duplicate_fact_lines() {
        let content = "# Title\n\n- Exact same fact\n- Different fact\n- Exact same fact\n";
        let result = repair_document(content);
        assert!(result.fixes > 0);
        let repaired = result.content.unwrap();
        assert_eq!(repaired.matches("Exact same fact").count(), 1);
        assert!(repaired.contains("Different fact"));
    }

    #[test]
    fn test_repair_preserves_trailing_newline() {
        let content = "# Title @t[?]\n\n- Fact\n";
        let result = repair_document(content);
        assert!(result.content.as_ref().unwrap().ends_with('\n'));
    }

    #[test]
    fn test_repair_multiple_issues() {
        let content = "# Bad Title @t[?]\n\n- Dup fact\n- Dup fact\n\n[^1]: Not a conflict\n[^9]: Orphaned\n";
        let result = repair_document(content);
        assert!(result.fixes >= 3, "Expected at least 3 fixes, got {}", result.fixes);
    }

    #[test]
    fn test_repair_diff_descriptions() {
        let content = "# Title @t[?]\n\n- Fact\n";
        let result = repair_document(content);
        assert!(!result.descriptions.is_empty());
        assert!(result.descriptions[0].contains("Cleaned title"));
    }
}
