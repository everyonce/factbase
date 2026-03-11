//! Output formatting helpers for search results.

use factbase::database::Database;
use factbase::models::SearchResult;
use serde::Serialize;
use std::collections::HashMap;

/// Summary statistics for search results
#[derive(Debug, Serialize)]
pub struct SearchSummary {
    pub total_matches: usize,
    pub avg_relevance: f32,
    pub min_relevance: f32,
    pub max_relevance: f32,
    pub by_type: HashMap<String, usize>,
    pub by_repo: HashMap<String, usize>,
}

/// Relevance statistics (avg, min, max)
#[derive(Debug, PartialEq)]
pub struct RelevanceStats {
    pub avg: f32,
    pub min: f32,
    pub max: f32,
}

/// Calculate relevance statistics from search results.
/// Returns (0.0, 0.0, 0.0) for empty results.
pub fn calculate_relevance_stats(results: &[SearchResult]) -> RelevanceStats {
    if results.is_empty() {
        return RelevanceStats {
            avg: 0.0,
            min: 0.0,
            max: 0.0,
        };
    }
    let sum: f32 = results.iter().map(|r| r.relevance_score).sum();
    let min = results
        .iter()
        .map(|r| r.relevance_score)
        .fold(f32::INFINITY, f32::min);
    let max = results
        .iter()
        .map(|r| r.relevance_score)
        .fold(f32::NEG_INFINITY, f32::max);
    RelevanceStats {
        avg: sum / results.len() as f32,
        min,
        max,
    }
}

/// Count search results by document type.
/// Documents without a type are counted as "unknown".
pub fn count_by_type(results: &[SearchResult]) -> HashMap<String, usize> {
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for r in results {
        let type_key = r.doc_type.as_deref().unwrap_or("unknown");
        *by_type.entry(type_key.to_owned()).or_insert(0) += 1;
    }
    by_type
}

/// Format a search result as a compact one-line string.
/// Format: "[XX%] id: title"
pub fn format_compact_result(result: &SearchResult) -> String {
    format!(
        "[{:.0}%] {}: {}",
        result.relevance_score * 100.0,
        result.id,
        result.title
    )
}

/// Compute summary statistics from search results
pub fn compute_search_summary(
    results: &[SearchResult],
    db: &Database,
) -> anyhow::Result<SearchSummary> {
    let total_matches = results.len();
    let stats = calculate_relevance_stats(results);
    let by_type = count_by_type(results);

    // Count by repository (requires database lookup)
    let mut by_repo: HashMap<String, usize> = HashMap::new();
    for r in results {
        if let Ok(Some(doc)) = db.get_document(&r.id) {
            *by_repo.entry(doc.repo_id).or_insert(0) += 1;
        }
    }

    Ok(SearchSummary {
        total_matches,
        avg_relevance: stats.avg,
        min_relevance: stats.min,
        max_relevance: stats.max,
        by_type,
        by_repo,
    })
}

/// Print search summary in human-readable format
pub fn print_search_summary(summary: &SearchSummary, query: &str) {
    println!("Search Summary for: \"{query}\"");
    println!("{}", "=".repeat(40));
    println!();
    println!("Total matches: {}", summary.total_matches);
    if summary.total_matches > 0 {
        println!(
            "Relevance: avg {:.0}%, min {:.0}%, max {:.0}%",
            summary.avg_relevance * 100.0,
            summary.min_relevance * 100.0,
            summary.max_relevance * 100.0
        );
    }
    println!();

    if !summary.by_type.is_empty() {
        println!("By type:");
        let mut types: Vec<_> = summary.by_type.iter().collect();
        types.sort_by(|a, b| b.1.cmp(a.1));
        for (t, count) in types {
            println!("  {t}: {count}");
        }
        println!();
    }

    if !summary.by_repo.is_empty() {
        println!("By repository:");
        let mut repos: Vec<_> = summary.by_repo.iter().collect();
        repos.sort_by(|a, b| b.1.cmp(a.1));
        for (repo, count) in repos {
            println!("  {repo}: {count}");
        }
    }
}

/// Print search results in compact format (one line per result)
pub fn print_compact_results(results: &[SearchResult]) {
    for r in results {
        println!("{}", format_compact_result(r));
    }
}

/// Print search results in detailed format
pub fn print_detailed_results(results: &[SearchResult]) {
    for (i, r) in results.iter().enumerate() {
        println!("{}. {} ({:.0}%)", i + 1, r.title, r.relevance_score * 100.0);
        if let Some(t) = &r.doc_type {
            println!("   Type: {t}");
        }
        println!("   Path: {}", r.file_path);
        println!("   ID: {}", r.id);
        if let Some(idx) = r.chunk_index {
            if let (Some(start), Some(end)) = (r.chunk_start, r.chunk_end) {
                println!("   Chunk: {idx} (chars {start}-{end})");
            }
        }
        let display_snippet = r.highlighted_snippet.as_ref().unwrap_or(&r.snippet);
        if !display_snippet.is_empty() {
            println!("   {}", display_snippet.replace('\n', " "));
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::search::tests::make_result;

    #[test]
    fn test_calculate_relevance_stats_empty() {
        let results: Vec<SearchResult> = vec![];
        let stats = calculate_relevance_stats(&results);
        assert_eq!(stats.avg, 0.0);
        assert_eq!(stats.min, 0.0);
        assert_eq!(stats.max, 0.0);
    }

    #[test]
    fn test_calculate_relevance_stats_single() {
        let results = vec![make_result("a", "Doc A", None, 0.75)];
        let stats = calculate_relevance_stats(&results);
        assert_eq!(stats.avg, 0.75);
        assert_eq!(stats.min, 0.75);
        assert_eq!(stats.max, 0.75);
    }

    #[test]
    fn test_calculate_relevance_stats_multiple() {
        let results = vec![
            make_result("a", "Doc A", None, 0.9),
            make_result("b", "Doc B", None, 0.6),
            make_result("c", "Doc C", None, 0.3),
        ];
        let stats = calculate_relevance_stats(&results);
        // avg = (0.9 + 0.6 + 0.3) / 3 = 0.6
        assert!((stats.avg - 0.6).abs() < 0.001);
        assert_eq!(stats.min, 0.3);
        assert_eq!(stats.max, 0.9);
    }

    #[test]
    fn test_count_by_type_empty() {
        let results: Vec<SearchResult> = vec![];
        let counts = count_by_type(&results);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_count_by_type_with_types() {
        let results = vec![
            make_result("a", "Doc A", Some("person"), 0.9),
            make_result("b", "Doc B", Some("person"), 0.8),
            make_result("c", "Doc C", Some("project"), 0.7),
        ];
        let counts = count_by_type(&results);
        assert_eq!(counts.get("person"), Some(&2));
        assert_eq!(counts.get("project"), Some(&1));
    }

    #[test]
    fn test_count_by_type_unknown() {
        let results = vec![
            make_result("a", "Doc A", None, 0.9),
            make_result("b", "Doc B", Some("person"), 0.8),
        ];
        let counts = count_by_type(&results);
        assert_eq!(counts.get("unknown"), Some(&1));
        assert_eq!(counts.get("person"), Some(&1));
    }

    #[test]
    fn test_format_compact_result() {
        let result = make_result("abc123", "Test Document", Some("person"), 0.85);
        let formatted = format_compact_result(&result);
        assert_eq!(formatted, "[85%] abc123: Test Document");
    }

    #[test]
    fn test_format_compact_result_low_score() {
        let result = make_result("def456", "Another Doc", None, 0.123);
        let formatted = format_compact_result(&result);
        assert_eq!(formatted, "[12%] def456: Another Doc");
    }
}
