//! Shared filter parsing and application for search and grep commands.

use std::collections::HashMap;

use factbase::{parse_source_references, parse_temporal_tags, Database, Link, SearchResult};

/// Parsed filter expression for document metadata filtering.
///
/// Used by search and grep commands to filter results by type, temporal tags,
/// source references, and link counts.
#[derive(Debug, Clone)]
pub enum FilterExpr {
    /// Filter by document type (case-insensitive)
    Type(String),
    /// Filter to documents with temporal tags
    HasTemporal,
    /// Filter to documents with source references
    HasSources,
    /// Filter to documents with more than N outgoing links
    LinksGreaterThan(usize),
    /// Filter to documents with fewer than N outgoing links
    LinksLessThan(usize),
    /// Filter to documents with exactly N outgoing links
    LinksEquals(usize),
}

/// Parse a filter expression string into a FilterExpr.
///
/// Supported formats:
/// - `type:person` - Filter by document type
/// - `has:temporal` - Filter to documents with temporal tags
/// - `has:sources` - Filter to documents with source references
/// - `links:>5` - Filter to documents with more than 5 links
/// - `links:<3` - Filter to documents with fewer than 3 links
/// - `links:0` - Filter to documents with exactly 0 links
///
/// Returns `None` if the expression is not recognized.
pub fn parse_filter_expr(expr: &str) -> Option<FilterExpr> {
    let expr = expr.trim();
    if let Some(value) = expr.strip_prefix("type:") {
        Some(FilterExpr::Type(value.to_lowercase()))
    } else if expr == "has:temporal" {
        Some(FilterExpr::HasTemporal)
    } else if expr == "has:sources" {
        Some(FilterExpr::HasSources)
    } else if let Some(value) = expr.strip_prefix("links:>") {
        value.parse().ok().map(FilterExpr::LinksGreaterThan)
    } else if let Some(value) = expr.strip_prefix("links:<") {
        value.parse().ok().map(FilterExpr::LinksLessThan)
    } else if let Some(value) = expr.strip_prefix("links:") {
        value.parse().ok().map(FilterExpr::LinksEquals)
    } else {
        None
    }
}

/// Check if a search result matches a filter expression.
///
/// For link-based filters, if `outgoing_links` is provided, it will be used
/// instead of querying the database. This enables batch fetching optimization.
pub fn matches_filter(
    result: &SearchResult,
    filter: &FilterExpr,
    db: &Database,
    outgoing_links: Option<&[Link]>,
) -> bool {
    match filter {
        FilterExpr::Type(t) => result
            .doc_type
            .as_ref()
            .is_some_and(|dt| dt.to_lowercase() == *t),
        FilterExpr::HasTemporal => {
            if let Ok(Some(doc)) = db.get_document(&result.id) {
                !parse_temporal_tags(&doc.content).is_empty()
            } else {
                false
            }
        }
        FilterExpr::HasSources => {
            if let Ok(Some(doc)) = db.get_document(&result.id) {
                !parse_source_references(&doc.content).is_empty()
            } else {
                false
            }
        }
        FilterExpr::LinksGreaterThan(n) => {
            let count = outgoing_links
                .map(|links| links.len())
                .unwrap_or_else(|| db.get_links_from(&result.id).unwrap_or_default().len());
            count > *n
        }
        FilterExpr::LinksLessThan(n) => {
            let count = outgoing_links
                .map(|links| links.len())
                .unwrap_or_else(|| db.get_links_from(&result.id).unwrap_or_default().len());
            count < *n
        }
        FilterExpr::LinksEquals(n) => {
            let count = outgoing_links
                .map(|links| links.len())
                .unwrap_or_else(|| db.get_links_from(&result.id).unwrap_or_default().len());
            count == *n
        }
    }
}

/// Check if any filters require link information.
fn needs_links(filters: &[FilterExpr]) -> bool {
    filters.iter().any(|f| {
        matches!(
            f,
            FilterExpr::LinksGreaterThan(_)
                | FilterExpr::LinksLessThan(_)
                | FilterExpr::LinksEquals(_)
        )
    })
}

/// Batch fetch outgoing links for all results if needed.
fn fetch_links_if_needed(
    results: &[SearchResult],
    filters: &[FilterExpr],
    db: &Database,
) -> HashMap<String, Vec<Link>> {
    if !needs_links(filters) || results.is_empty() {
        return HashMap::new();
    }

    let doc_ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    db.get_links_for_documents(&doc_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, (outgoing, _))| (id, outgoing))
        .collect()
}

/// Apply include filters to search results (all filters must match).
pub fn apply_include_filters(
    results: &mut Vec<SearchResult>,
    filters: &[FilterExpr],
    db: &Database,
    limit: usize,
) {
    if filters.is_empty() {
        return;
    }

    // Batch fetch links if any filter needs them
    let links_map = fetch_links_if_needed(results, filters, db);

    results.retain(|r| {
        let outgoing = links_map.get(&r.id).map(|v| v.as_slice());
        filters.iter().all(|f| matches_filter(r, f, db, outgoing))
    });
    results.truncate(limit);
}

/// Apply exclude filters to search results (any match excludes).
pub fn apply_exclude_filters(
    results: &mut Vec<SearchResult>,
    excludes: &[FilterExpr],
    db: &Database,
    limit: usize,
) {
    if excludes.is_empty() {
        return;
    }

    // Batch fetch links if any filter needs them
    let links_map = fetch_links_if_needed(results, excludes, db);

    results.retain(|r| {
        let outgoing = links_map.get(&r.id).map(|v| v.as_slice());
        !excludes.iter().any(|f| matches_filter(r, f, db, outgoing))
    });
    results.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filter_expr_type() {
        let expr = parse_filter_expr("type:person");
        assert!(matches!(expr, Some(FilterExpr::Type(t)) if t == "person"));

        let expr = parse_filter_expr("type:Person");
        assert!(matches!(expr, Some(FilterExpr::Type(t)) if t == "person"));
    }

    #[test]
    fn test_parse_filter_expr_has_temporal() {
        let expr = parse_filter_expr("has:temporal");
        assert!(matches!(expr, Some(FilterExpr::HasTemporal)));
    }

    #[test]
    fn test_parse_filter_expr_has_sources() {
        let expr = parse_filter_expr("has:sources");
        assert!(matches!(expr, Some(FilterExpr::HasSources)));
    }

    #[test]
    fn test_parse_filter_expr_links_greater_than() {
        let expr = parse_filter_expr("links:>5");
        assert!(matches!(expr, Some(FilterExpr::LinksGreaterThan(5))));

        let expr = parse_filter_expr("links:>0");
        assert!(matches!(expr, Some(FilterExpr::LinksGreaterThan(0))));
    }

    #[test]
    fn test_parse_filter_expr_links_less_than() {
        let expr = parse_filter_expr("links:<10");
        assert!(matches!(expr, Some(FilterExpr::LinksLessThan(10))));
    }

    #[test]
    fn test_parse_filter_expr_links_equals() {
        let expr = parse_filter_expr("links:3");
        assert!(matches!(expr, Some(FilterExpr::LinksEquals(3))));
    }

    #[test]
    fn test_parse_filter_expr_invalid() {
        assert!(parse_filter_expr("invalid").is_none());
        assert!(parse_filter_expr("has:invalid").is_none());
        assert!(parse_filter_expr("links:abc").is_none());
        assert!(parse_filter_expr("").is_none());
    }

    #[test]
    fn test_parse_filter_expr_whitespace() {
        let expr = parse_filter_expr("  type:person  ");
        assert!(matches!(expr, Some(FilterExpr::Type(t)) if t == "person"));
    }

    #[test]
    fn test_exclude_filter_type() {
        let expr = parse_filter_expr("type:draft");
        assert!(matches!(expr, Some(FilterExpr::Type(t)) if t == "draft"));

        let expr = parse_filter_expr("type:archived");
        assert!(matches!(expr, Some(FilterExpr::Type(t)) if t == "archived"));
    }

    #[test]
    fn test_exclude_filter_has_temporal() {
        let expr = parse_filter_expr("has:temporal");
        assert!(matches!(expr, Some(FilterExpr::HasTemporal)));
    }

    #[test]
    fn test_exclude_filter_links() {
        let expr = parse_filter_expr("links:>10");
        assert!(matches!(expr, Some(FilterExpr::LinksGreaterThan(10))));

        let expr = parse_filter_expr("links:0");
        assert!(matches!(expr, Some(FilterExpr::LinksEquals(0))));
    }
}
