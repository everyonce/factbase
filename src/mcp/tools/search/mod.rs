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
}
