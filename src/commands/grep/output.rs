use super::{args::GrepArgs, OutputFormat};
use factbase::models::ContentSearchResult;
use factbase::output::should_highlight;
use factbase::output::ansi;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct GrepStats {
    pub total_matches: usize,
    pub document_count: usize,
    pub repository_count: usize,
    pub top_files: Vec<FileMatchCount>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct FileMatchCount {
    pub file_path: String,
    pub title: String,
    pub match_count: usize,
}

/// Compute statistics from grep results
pub(super) fn compute_grep_stats(results: &[ContentSearchResult]) -> GrepStats {
    let total_matches: usize = results.iter().map(|r| r.matches.len()).sum();
    let document_count = results.len();

    // Count unique repositories
    let mut repos: HashSet<&str> = HashSet::new();
    for r in results {
        repos.insert(&r.repo_id);
    }
    let repository_count = repos.len();

    // Get top 3 files by match count
    let mut file_counts: Vec<FileMatchCount> = results
        .iter()
        .map(|r| FileMatchCount {
            file_path: r.file_path.clone(),
            title: r.title.clone(),
            match_count: r.matches.len(),
        })
        .collect();
    file_counts.sort_by(|a, b| b.match_count.cmp(&a.match_count));
    file_counts.truncate(3);

    GrepStats {
        total_matches,
        document_count,
        repository_count,
        top_files: file_counts,
    }
}

/// Determine if output should be highlighted based on flags and environment
pub(super) fn should_highlight_output(args: &GrepArgs, format: &OutputFormat) -> bool {
    // No highlighting for JSON/YAML output
    if !matches!(format, OutputFormat::Table) {
        return false;
    }

    // Use shared should_highlight function with explicit flag
    should_highlight(args.highlight)
}

/// Highlight all matches of the pattern in the text using ANSI escape codes
pub(super) fn highlight_matches(text: &str, pattern: &regex::Regex) -> String {
    pattern
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}{}", ansi::BOLD_RED, &caps[0], ansi::RESET)
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use factbase::models::{ContentMatch, ContentSearchResult};

    fn make_result(
        repo_id: &str,
        file_path: &str,
        title: &str,
        match_count: usize,
    ) -> ContentSearchResult {
        ContentSearchResult {
            id: "test123".to_string(),
            title: title.to_string(),
            doc_type: None,
            file_path: file_path.to_string(),
            repo_id: repo_id.to_string(),
            matches: (0..match_count)
                .map(|i| ContentMatch {
                    line_number: i + 1,
                    line: format!("line {}", i),
                    context: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_compute_grep_stats_empty() {
        let results: Vec<ContentSearchResult> = vec![];
        let stats = compute_grep_stats(&results);
        assert_eq!(stats.total_matches, 0);
        assert_eq!(stats.document_count, 0);
        assert_eq!(stats.repository_count, 0);
        assert!(stats.top_files.is_empty());
    }

    #[test]
    fn test_compute_grep_stats_single_result() {
        let results = vec![make_result("repo1", "file1.md", "Title 1", 3)];
        let stats = compute_grep_stats(&results);
        assert_eq!(stats.total_matches, 3);
        assert_eq!(stats.document_count, 1);
        assert_eq!(stats.repository_count, 1);
        assert_eq!(stats.top_files.len(), 1);
        assert_eq!(stats.top_files[0].match_count, 3);
    }

    #[test]
    fn test_compute_grep_stats_multiple_repos() {
        let results = vec![
            make_result("repo1", "file1.md", "Title 1", 2),
            make_result("repo2", "file2.md", "Title 2", 3),
            make_result("repo1", "file3.md", "Title 3", 1),
        ];
        let stats = compute_grep_stats(&results);
        assert_eq!(stats.total_matches, 6);
        assert_eq!(stats.document_count, 3);
        assert_eq!(stats.repository_count, 2);
    }

    #[test]
    fn test_compute_grep_stats_top_files_sorted() {
        let results = vec![
            make_result("repo1", "low.md", "Low", 1),
            make_result("repo1", "high.md", "High", 10),
            make_result("repo1", "mid.md", "Mid", 5),
        ];
        let stats = compute_grep_stats(&results);
        assert_eq!(stats.top_files.len(), 3);
        assert_eq!(stats.top_files[0].match_count, 10);
        assert_eq!(stats.top_files[1].match_count, 5);
        assert_eq!(stats.top_files[2].match_count, 1);
    }

    #[test]
    fn test_compute_grep_stats_top_files_truncated() {
        let results = vec![
            make_result("repo1", "f1.md", "T1", 1),
            make_result("repo1", "f2.md", "T2", 2),
            make_result("repo1", "f3.md", "T3", 3),
            make_result("repo1", "f4.md", "T4", 4),
            make_result("repo1", "f5.md", "T5", 5),
        ];
        let stats = compute_grep_stats(&results);
        assert_eq!(stats.top_files.len(), 3);
        assert_eq!(stats.top_files[0].match_count, 5);
        assert_eq!(stats.top_files[2].match_count, 3);
    }

    #[test]
    fn test_highlight_matches_simple() {
        let pattern = regex::Regex::new("TODO").unwrap();
        let result = highlight_matches("Fix this TODO item", &pattern);
        assert!(result.contains(ansi::BOLD_RED));
        assert!(result.contains(ansi::RESET));
        assert!(result.contains("TODO"));
    }

    #[test]
    fn test_highlight_matches_multiple() {
        let pattern = regex::Regex::new("test").unwrap();
        let result = highlight_matches("test one test two", &pattern);
        let count = result.matches(ansi::BOLD_RED).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_highlight_matches_no_match() {
        let pattern = regex::Regex::new("xyz").unwrap();
        let result = highlight_matches("no match here", &pattern);
        assert_eq!(result, "no match here");
        assert!(!result.contains(ansi::BOLD_RED));
    }

    #[test]
    fn test_grep_stats_serialization() {
        let stats = GrepStats {
            total_matches: 10,
            document_count: 3,
            repository_count: 2,
            top_files: vec![FileMatchCount {
                file_path: "test.md".to_string(),
                title: "Test".to_string(),
                match_count: 5,
            }],
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_matches\":10"));
        assert!(json.contains("\"document_count\":3"));
        assert!(json.contains("\"repository_count\":2"));
        assert!(json.contains("\"top_files\""));
    }

    #[test]
    fn test_file_match_count_serialization() {
        let fmc = FileMatchCount {
            file_path: "docs/readme.md".to_string(),
            title: "README".to_string(),
            match_count: 7,
        };
        let json = serde_json::to_string(&fmc).unwrap();
        assert!(json.contains("\"file_path\":\"docs/readme.md\""));
        assert!(json.contains("\"title\":\"README\""));
        assert!(json.contains("\"match_count\":7"));
    }
}
