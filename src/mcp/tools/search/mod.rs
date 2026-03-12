//! Search-related MCP tools: search_knowledge, search_content, get_fact_pairs

mod get_fact_pairs;
mod search_content;
mod search_knowledge;
mod search_temporal;

pub use get_fact_pairs::get_fact_pairs;
pub use search_content::search_content;
pub use search_knowledge::search_knowledge;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::{SearchResult, TemporalTagType};
use crate::processor::{overlaps_point, overlaps_range, parse_temporal_tags};
use std::collections::HashMap;

/// Parses a `during` range string (e.g. "2020..2022") into start/end tuple.
pub(crate) fn parse_during_range(during: &str) -> Result<(String, String), FactbaseError> {
    let parts: Vec<&str> = during.split("..").collect();
    if parts.len() != 2 {
        return Err(FactbaseError::parse(
            "Invalid during format. Use YYYY..YYYY or YYYY-MM..YYYY-MM",
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Fetches document content for a list of IDs into a HashMap.
pub(crate) fn fetch_docs_content(
    db: &Database,
    result_ids: &[String],
) -> Result<HashMap<String, String>, FactbaseError> {
    let mut map = HashMap::new();
    for id in result_ids {
        if let Ok(Some(doc)) = db.get_document(id) {
            map.insert(id.clone(), doc.content);
        }
    }
    Ok(map)
}

/// Applies temporal filtering to search results in-place.
///
/// Removes results that don't match the temporal criteria:
/// - Documents without temporal tags are excluded
/// - `exclude_unknown`: removes documents with `@t[?]` tags
/// - `as_of`: keeps only documents with tags overlapping the given date
/// - `during_range`: keeps only documents with tags overlapping the given range
pub(crate) fn apply_temporal_filter(
    results: &mut Vec<SearchResult>,
    docs_content: &HashMap<String, String>,
    as_of: Option<&str>,
    during_range: Option<&(String, String)>,
    exclude_unknown: bool,
) {
    results.retain(|r| {
        let Some(content) = docs_content.get(&r.id) else {
            return false;
        };
        let tags = parse_temporal_tags(content);
        if tags.is_empty() {
            return false;
        }
        if exclude_unknown
            && tags
                .iter()
                .any(|tag| tag.tag_type == TemporalTagType::Unknown)
        {
            return false;
        }
        if let Some(as_of_date) = as_of {
            if !tags.iter().any(|tag| overlaps_point(tag, as_of_date)) {
                return false;
            }
        }
        if let Some((start, end)) = during_range {
            if !tags.iter().any(|tag| overlaps_range(tag, start, end)) {
                return false;
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_during_range_valid_years() {
        let (start, end) = parse_during_range("2020..2022").unwrap();
        assert_eq!(start, "2020");
        assert_eq!(end, "2022");
    }

    #[test]
    fn test_parse_during_range_valid_months() {
        let (start, end) = parse_during_range("2020-01..2022-12").unwrap();
        assert_eq!(start, "2020-01");
        assert_eq!(end, "2022-12");
    }

    #[test]
    fn test_parse_during_range_invalid_no_separator() {
        assert!(parse_during_range("2020-2022").is_err());
    }

    #[test]
    fn test_parse_during_range_invalid_single_dot() {
        assert!(parse_during_range("2020.2022").is_err());
    }

    #[test]
    fn test_parse_during_range_triple_dot() {
        // "2020...2022" splits into ["2020", ".2022"] - 2 parts, passes validation
        let (start, end) = parse_during_range("2020...2022").unwrap();
        assert_eq!(start, "2020");
        assert_eq!(end, ".2022");
    }

    #[test]
    fn test_apply_temporal_filter_removes_docs_without_tags() {
        let mut results = vec![SearchResult {
            id: "doc1".into(),
            title: "No Tags".into(),
            doc_type: None,
            file_path: "doc1.md".into(),
            relevance_score: 0.9,
            snippet: String::new(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        }];
        let mut docs = HashMap::new();
        docs.insert("doc1".into(), "# No Tags\n\n- Just a fact".into());
        apply_temporal_filter(&mut results, &docs, None, None, false);
        assert!(
            results.is_empty(),
            "docs without temporal tags should be removed"
        );
    }

    #[test]
    fn test_apply_temporal_filter_keeps_docs_with_tags() {
        let mut results = vec![SearchResult {
            id: "doc1".into(),
            title: "Has Tags".into(),
            doc_type: None,
            file_path: "doc1.md".into(),
            relevance_score: 0.9,
            snippet: String::new(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        }];
        let mut docs = HashMap::new();
        docs.insert("doc1".into(), "# Has Tags\n\n- Fact @t[=2023-01]".into());
        apply_temporal_filter(&mut results, &docs, None, None, false);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_apply_temporal_filter_exclude_unknown() {
        let mut results = vec![SearchResult {
            id: "doc1".into(),
            title: "Unknown".into(),
            doc_type: None,
            file_path: "doc1.md".into(),
            relevance_score: 0.9,
            snippet: String::new(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        }];
        let mut docs = HashMap::new();
        docs.insert("doc1".into(), "# Unknown\n\n- Fact @t[?]".into());
        apply_temporal_filter(&mut results, &docs, None, None, true);
        assert!(results.is_empty(), "unknown tags should be excluded");
    }

    fn make_search_result(id: &str, title: &str) -> SearchResult {
        SearchResult {
            id: id.into(),
            title: title.into(),
            doc_type: None,
            file_path: format!("{id}.md"),
            relevance_score: 0.9,
            snippet: String::new(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        }
    }

    #[test]
    fn test_apply_temporal_filter_as_of() {
        let mut results = vec![
            make_search_result("doc1", "In Range"),
            make_search_result("doc2", "Out of Range"),
        ];
        let mut docs = HashMap::new();
        docs.insert("doc1".into(), "- Fact @t[2020..2025]".into());
        docs.insert("doc2".into(), "- Fact @t[=2018]".into());
        apply_temporal_filter(&mut results, &docs, Some("2023"), None, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }

    #[test]
    fn test_apply_temporal_filter_during_range() {
        let mut results = vec![make_search_result("doc1", "Overlaps")];
        let mut docs = HashMap::new();
        docs.insert("doc1".into(), "- Fact @t[2020..2025]".into());
        let range = ("2022".to_string(), "2024".to_string());
        apply_temporal_filter(&mut results, &docs, None, Some(&range), false);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_apply_temporal_filter_missing_content() {
        let mut results = vec![make_search_result("missing", "Missing")];
        let docs = HashMap::new();
        apply_temporal_filter(&mut results, &docs, None, None, false);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fetch_docs_content_returns_map() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let doc = test_doc("abc123", "Test");
        db.upsert_document(&doc).unwrap();
        let map = fetch_docs_content(&db, &["abc123".into()]).unwrap();
        assert!(map.contains_key("abc123"));
        assert!(map["abc123"].contains("Test"));
    }

    #[test]
    fn test_fetch_docs_content_missing_id_skipped() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let map = fetch_docs_content(&db, &["nonexistent".into()]).unwrap();
        assert!(map.is_empty());
    }
}
