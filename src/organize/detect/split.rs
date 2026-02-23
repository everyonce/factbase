//! Split candidate detection.
//!
//! Identifies documents that cover multiple distinct topics and could be split.

use super::{collect_active_documents, cosine_similarity};
use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::organize::{SplitCandidate, SplitSection};

/// Minimum content length for a section to be considered for embedding.
const MIN_SECTION_CONTENT: usize = 50;

/// Minimum number of sections required to consider splitting.
const MIN_SECTIONS_FOR_SPLIT: usize = 2;

/// Extract sections from markdown content based on headers.
///
/// Returns sections with their content. The first section (before any header)
/// is labeled "Introduction" with level 0.
pub fn extract_sections(content: &str) -> Vec<SplitSection> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();
    let mut current_title = "Introduction".to_string();
    let mut current_level: u8 = 0;
    let mut current_start: usize = 1;
    let mut current_content = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        // Skip factbase header
        if line.starts_with("<!-- factbase:") {
            continue;
        }

        // Check for header
        if let Some((level, title)) = parse_header(line) {
            // Save previous section if it has content
            if !current_content.is_empty() {
                let content_str = current_content.join("\n").trim().to_string();
                if !content_str.is_empty() {
                    sections.push(SplitSection {
                        title: current_title.clone(),
                        level: current_level,
                        start_line: current_start,
                        end_line: line_num - 1,
                        content: content_str,
                    });
                }
            }

            // Start new section
            current_title = title;
            current_level = level;
            current_start = line_num;
            current_content.clear();
        } else {
            current_content.push(*line);
        }
    }

    // Save final section
    if !current_content.is_empty() {
        let content_str = current_content.join("\n").trim().to_string();
        if !content_str.is_empty() {
            sections.push(SplitSection {
                title: current_title,
                level: current_level,
                start_line: current_start,
                end_line: lines.len(),
                content: content_str,
            });
        }
    }

    sections
}

/// Parse a markdown header line, returning (level, title).
fn parse_header(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }

    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }

    let title = trimmed[level..].trim().to_string();
    if title.is_empty() {
        return None;
    }

    Some((level as u8, title))
}

/// Detects documents that are candidates for splitting based on section dissimilarity.
///
/// A document is a split candidate if its sections have low mutual similarity,
/// indicating they cover distinct topics that could be separate documents.
///
/// # Arguments
/// * `db` - Database connection
/// * `embedding` - Embedding provider for generating section embeddings
/// * `threshold` - Maximum average similarity for split candidate (default 0.5)
/// * `repo_id` - Optional repository filter
///
/// # Returns
/// Vector of split candidates, sorted by average similarity ascending (most distinct first).
pub async fn detect_split_candidates(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    threshold: f32,
    repo_id: Option<&str>,
) -> Result<Vec<SplitCandidate>, FactbaseError> {
    let docs = collect_active_documents(db, repo_id)?;

    let mut candidates = Vec::new();

    for doc in docs {
        // Extract sections from document
        let sections = extract_sections(&doc.content);

        // Need at least 2 sections with sufficient content
        let valid_sections: Vec<_> = sections
            .into_iter()
            .filter(|s| s.content.len() >= MIN_SECTION_CONTENT)
            .collect();

        if valid_sections.len() < MIN_SECTIONS_FOR_SPLIT {
            continue;
        }

        // Generate embeddings for each section
        let section_texts: Vec<&str> = valid_sections.iter().map(|s| s.content.as_str()).collect();
        let embeddings = embedding.generate_batch(&section_texts).await?;

        // Calculate pairwise similarities
        let mut similarities = Vec::new();
        for i in 0..embeddings.len() {
            for j in (i + 1)..embeddings.len() {
                let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
                similarities.push(sim);
            }
        }

        if similarities.is_empty() {
            continue;
        }

        let avg_similarity: f32 = similarities.iter().sum::<f32>() / similarities.len() as f32;
        let min_similarity: f32 = similarities
            .iter()
            .copied()
            .min_by(|a, b| a.partial_cmp(b).expect("non-NaN similarity"))
            .unwrap_or(0.0);

        // If average similarity is below threshold, it's a split candidate
        if avg_similarity < threshold {
            let section_names: Vec<_> = valid_sections.iter().map(|s| s.title.as_str()).collect();
            let rationale = format!(
                "Sections {} have low mutual similarity (avg: {:.2}, min: {:.2})",
                section_names.join(", "),
                avg_similarity,
                min_similarity
            );

            candidates.push(SplitCandidate {
                doc_id: doc.id.clone(),
                doc_title: doc.title.clone(),
                sections: valid_sections,
                avg_similarity,
                min_similarity,
                rationale,
            });
        }
    }

    // Sort by average similarity ascending (most distinct first)
    candidates.sort_by(|a, b| {
        a.avg_similarity
            .partial_cmp(&b.avg_similarity)
            .expect("non-NaN similarity")
    });

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_h1() {
        let result = parse_header("# Title");
        assert_eq!(result, Some((1, "Title".to_string())));
    }

    #[test]
    fn test_parse_header_h2() {
        let result = parse_header("## Section Name");
        assert_eq!(result, Some((2, "Section Name".to_string())));
    }

    #[test]
    fn test_parse_header_h6() {
        let result = parse_header("###### Deep Header");
        assert_eq!(result, Some((6, "Deep Header".to_string())));
    }

    #[test]
    fn test_parse_header_not_header() {
        assert_eq!(parse_header("Not a header"), None);
        assert_eq!(parse_header("- List item"), None);
        assert_eq!(parse_header(""), None);
    }

    #[test]
    fn test_parse_header_empty_title() {
        assert_eq!(parse_header("##"), None);
        assert_eq!(parse_header("## "), None);
    }

    #[test]
    fn test_parse_header_with_leading_space() {
        let result = parse_header("  ## Indented");
        assert_eq!(result, Some((2, "Indented".to_string())));
    }

    #[test]
    fn test_extract_sections_simple() {
        let content =
            "# Title\n\nIntro text.\n\n## Section 1\n\nContent 1.\n\n## Section 2\n\nContent 2.";
        let sections = extract_sections(content);

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].title, "Title");
        assert_eq!(sections[0].level, 1);
        assert!(sections[0].content.contains("Intro text"));
        assert_eq!(sections[1].title, "Section 1");
        assert_eq!(sections[1].level, 2);
        assert_eq!(sections[2].title, "Section 2");
    }

    #[test]
    fn test_extract_sections_no_headers() {
        let content = "Just some text\nwithout any headers.";
        let sections = extract_sections(content);

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Introduction");
        assert_eq!(sections[0].level, 0);
    }

    #[test]
    fn test_extract_sections_skips_factbase_header() {
        let content = "<!-- factbase:abc123 -->\n# Title\n\nContent here.";
        let sections = extract_sections(content);

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Title");
        assert!(!sections[0].content.contains("factbase"));
    }

    #[test]
    fn test_extract_sections_preserves_line_numbers() {
        let content = "# Title\n\nLine 3\nLine 4\n\n## Section\n\nLine 8";
        let sections = extract_sections(content);

        assert_eq!(sections[0].start_line, 1);
        assert_eq!(sections[1].start_line, 6);
    }
}
