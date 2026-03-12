//! Inbox block parsing and integration for `review --apply`.
//!
//! Parses `<!-- factbase:inbox -->` ... `<!-- /factbase:inbox -->` blocks
//! and uses LLM to integrate inbox content into the document.

use crate::error::FactbaseError;

/// A parsed inbox block with its location in the document.
#[derive(Debug, Clone)]
pub struct InboxBlock {
    /// The content between the inbox markers (trimmed).
    pub content: String,
    /// Start line index (0-based, inclusive) of the opening marker.
    pub start_line: usize,
    /// End line index (0-based, inclusive) of the closing marker.
    pub end_line: usize,
}

const INBOX_OPEN: &str = "<!-- factbase:inbox -->";
const INBOX_CLOSE: &str = "<!-- /factbase:inbox -->";

/// Extract all inbox blocks from document content.
pub fn extract_inbox_blocks(content: &str) -> Vec<InboxBlock> {
    let lines: Vec<&str> = content.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim() == INBOX_OPEN {
            let start = i;
            i += 1;
            // Find closing marker
            while i < lines.len() && lines[i].trim() != INBOX_CLOSE {
                i += 1;
            }
            if i < lines.len() {
                // Collect content between markers
                let inner: Vec<&str> = lines[start + 1..i].to_vec();
                let content = inner.join("\n").trim().to_string();
                if !content.is_empty() {
                    blocks.push(InboxBlock {
                        content,
                        start_line: start,
                        end_line: i,
                    });
                }
            }
        }
        i += 1;
    }

    blocks
}

/// Remove inbox blocks from document content.
/// Blocks must be sorted by start_line ascending (as returned by extract_inbox_blocks).
pub fn strip_inbox_blocks(content: &str, blocks: &[InboxBlock]) -> String {
    if blocks.is_empty() {
        return content.to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut skip_ranges: Vec<(usize, usize)> =
        blocks.iter().map(|b| (b.start_line, b.end_line)).collect();
    skip_ranges.sort_by_key(|r| r.0);

    let mut skip_idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if skip_idx < skip_ranges.len()
            && i >= skip_ranges[skip_idx].0
            && i <= skip_ranges[skip_idx].1
        {
            if i == skip_ranges[skip_idx].1 {
                skip_idx += 1;
            }
            continue;
        }
        result.push(*line);
    }

    // Remove trailing blank lines left by stripping
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }

    result.join("\n")
}

/// Default template for the inbox merge prompt.
pub const DEFAULT_INBOX_MERGE_PROMPT: &str = r#"Integrate the INBOX notes into the DOCUMENT below. The inbox contains corrections, updates, or new facts that should be merged into the appropriate sections.

DOCUMENT:
{document_content}

INBOX:
{inbox_content}

RULES:
1. Apply all corrections and updates from the inbox to the relevant lines
2. Add temporal tags (@t[YYYY], @t[YYYY-MM], @t[YYYY..], etc.) when the inbox provides dates
3. Add source footnotes [^N] when the inbox provides sources
4. Insert new facts into the appropriate section
5. Do NOT remove existing content unless the inbox explicitly says to delete or replace it
6. Do NOT include the inbox block markers in the output
7. Preserve the factbase ID header (<!-- factbase:XXXXXX -->)
8. Preserve the Review Queue section (<!-- factbase:review -->) if present
9. Output the complete updated document only"#;

/// Build the LLM prompt for integrating inbox content into a document.
pub fn build_inbox_prompt(
    document_content: &str,
    inbox_content: &str,
    prompts: &crate::config::PromptsConfig,
    repo_path: Option<&std::path::Path>,
) -> String {
    let file_override = repo_path
        .and_then(|p| crate::config::prompts::load_file_override(p, "prompts/inbox-merge.txt"));
    let default = file_override.as_deref().unwrap_or(DEFAULT_INBOX_MERGE_PROMPT);
    crate::config::prompts::resolve_prompt(
        prompts,
        "inbox_merge",
        default,
        &[
            ("document_content", document_content),
            ("inbox_content", inbox_content),
        ],
    )
}

/// Apply inbox integration by stripping inbox blocks. Returns the clean content.
/// The agent handles merging via update_document.
pub async fn apply_inbox_integration(
    content: &str,
    blocks: &[InboxBlock],
) -> Result<String, FactbaseError> {
    let clean_content = strip_inbox_blocks(content, blocks);

    if clean_content.is_empty() {
        return Err(FactbaseError::internal(
            "Stripping inbox blocks resulted in empty content".to_string(),
        ));
    }

    Ok(clean_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_inbox_blocks_single() {
        let content = "<!-- factbase:a1cb2b -->\n# Title\n\nSome content\n\n<!-- factbase:inbox -->\nUpdate: CEO changed to Jane Doe in 2026\n<!-- /factbase:inbox -->\n";
        let blocks = extract_inbox_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "Update: CEO changed to Jane Doe in 2026");
        assert_eq!(blocks[0].start_line, 5);
        assert_eq!(blocks[0].end_line, 7);
    }

    #[test]
    fn test_extract_inbox_blocks_multiple() {
        let content = "# Title\n\n<!-- factbase:inbox -->\nFirst update\n<!-- /factbase:inbox -->\n\nMiddle content\n\n<!-- factbase:inbox -->\nSecond update\n<!-- /factbase:inbox -->\n";
        let blocks = extract_inbox_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].content, "First update");
        assert_eq!(blocks[1].content, "Second update");
    }

    #[test]
    fn test_extract_inbox_blocks_empty() {
        let content = "# Title\n\nNo inbox here\n";
        let blocks = extract_inbox_blocks(content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_extract_inbox_blocks_empty_content() {
        let content = "# Title\n\n<!-- factbase:inbox -->\n\n<!-- /factbase:inbox -->\n";
        let blocks = extract_inbox_blocks(content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_extract_inbox_blocks_unclosed() {
        let content = "# Title\n\n<!-- factbase:inbox -->\nOrphaned block\n";
        let blocks = extract_inbox_blocks(content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_extract_inbox_blocks_multiline() {
        let content = "# Title\n\n<!-- factbase:inbox -->\nLine 1\nLine 2\nLine 3\n<!-- /factbase:inbox -->\n";
        let blocks = extract_inbox_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_strip_inbox_blocks() {
        let content = "# Title\n\nContent before\n\n<!-- factbase:inbox -->\nUpdate here\n<!-- /factbase:inbox -->\n\nContent after";
        let blocks = extract_inbox_blocks(content);
        let stripped = strip_inbox_blocks(content, &blocks);
        assert_eq!(stripped, "# Title\n\nContent before\n\n\nContent after");
    }

    #[test]
    fn test_strip_inbox_blocks_none() {
        let content = "# Title\n\nNo inbox";
        let stripped = strip_inbox_blocks(content, &[]);
        assert_eq!(stripped, content);
    }

    #[test]
    fn test_strip_inbox_blocks_multiple() {
        let content = "# Title\n\n<!-- factbase:inbox -->\nA\n<!-- /factbase:inbox -->\n\nMiddle\n\n<!-- factbase:inbox -->\nB\n<!-- /factbase:inbox -->\n\nEnd";
        let blocks = extract_inbox_blocks(content);
        let stripped = strip_inbox_blocks(content, &blocks);
        assert!(stripped.contains("Middle"));
        assert!(stripped.contains("End"));
        assert!(!stripped.contains("factbase:inbox"));
    }

    #[test]
    fn test_build_inbox_prompt_contains_content() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_inbox_prompt("# Doc\n\n- Fact one", "CEO is now Jane", &prompts, None);
        assert!(prompt.contains("# Doc"));
        assert!(prompt.contains("CEO is now Jane"));
        assert!(prompt.contains("DOCUMENT:"));
        assert!(prompt.contains("INBOX:"));
    }

    #[test]
    fn test_build_inbox_prompt_file_override() {
        let tmp = tempfile::TempDir::new().unwrap();
        let prompts_dir = tmp.path().join(".factbase").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("inbox-merge.txt"), "Merge: {document_content} + {inbox_content}").unwrap();
        let prompts = crate::config::PromptsConfig::default();
        let result = build_inbox_prompt("doc content", "inbox content", &prompts, Some(tmp.path()));
        assert_eq!(result, "Merge: doc content + inbox content");
    }

    #[test]
    fn test_build_inbox_prompt_no_override_uses_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let prompts = crate::config::PromptsConfig::default();
        let result = build_inbox_prompt("doc", "inbox", &prompts, Some(tmp.path()));
        assert!(result.contains("DOCUMENT:"), "should use compiled-in default");
    }
}
