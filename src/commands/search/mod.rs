//! Search command implementation.
//!
//! Semantic search across documents with filtering, sorting, and watch mode.
//!
//! # Submodules
//!
//! - `args` - Command arguments (SearchArgs)
//! - `filters` - Filter parsing and application
//! - `output` - Output formatting helpers
//! - `watch` - Watch mode logic

mod args;
mod filters;
mod output;
pub(crate) mod test_helpers;
mod watch;

// Re-export public items
pub use args::SearchArgs;

use crate::commands::{filter_by_excluded_types, find_repo_with_config, setup_cached_embedding};
use factbase::{
    calculate_recency_boost, config::validate_timeout, format_json, overlaps_point, overlaps_range,
    parse_temporal_tags, EmbeddingProvider, TemporalTagType,
};
use filters::{apply_exclude_filters, apply_include_filters, parse_filter_expr, FilterExpr};
use output::{
    compute_search_summary, print_compact_results, print_detailed_results, print_search_summary,
};
use std::cmp::Ordering;
use std::fs;
use watch::run_search_watch_mode;

#[tracing::instrument(
    name = "cmd_search",
    skip(args),
    fields(query = %args.query, limit = args.limit, repo = ?args.repo, doc_type = ?args.doc_type)
)]
pub async fn cmd_search(args: SearchArgs) -> anyhow::Result<()> {
    // Watch mode: re-run search when files change
    if args.watch {
        return run_search_watch_mode(args).await;
    }

    // Single search execution
    run_single_search(&args).await
}

/// Execute a single search and display results
pub(crate) async fn run_single_search(args: &SearchArgs) -> anyhow::Result<()> {
    let (config, db, _repo) = find_repo_with_config(args.repo.as_deref())?;

    // Parse --during range if provided
    let during_range = if let Some(ref during) = args.during {
        let parts: Vec<&str> = during.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid --during format. Use YYYY..YYYY or YYYY-MM..YYYY-MM");
        }
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    };

    // Fetch more results if we need to filter by temporal tags
    let fetch_limit = if args.as_of.is_some()
        || during_range.is_some()
        || args.exclude_unknown
        || args.filter.is_some()
        || args.exclude.is_some()
        || args.exclude_type.is_some()
    {
        args.limit * 5 // Fetch extra to account for filtering
    } else {
        args.limit
    };

    // Parse filter expressions if provided
    let filters: Vec<FilterExpr> = args
        .filter
        .as_ref()
        .map(|f| f.iter().filter_map(|e| parse_filter_expr(e)).collect())
        .unwrap_or_default();

    // Parse exclude expressions if provided
    let excludes: Vec<FilterExpr> = args
        .exclude
        .as_ref()
        .map(|f| f.iter().filter_map(|e| parse_filter_expr(e)).collect())
        .unwrap_or_default();

    let mut results = if args.title || args.offline {
        // Title search: no Ollama needed (offline-compatible)
        db.search_by_title(
            &args.query,
            fetch_limit,
            args.doc_type.as_deref(),
            args.repo.as_deref(),
        )?
    } else {
        // Validate timeout if provided
        if let Some(timeout) = args.timeout {
            validate_timeout(timeout)?;
        }
        let cached_embedding = setup_cached_embedding(&config, args.timeout).await;
        let query_embedding = cached_embedding.generate(&args.query).await?;
        db.search_semantic_with_query(
            &query_embedding,
            fetch_limit,
            args.doc_type.as_deref(),
            args.repo.as_deref(),
            Some(&args.query),
        )?
    };

    // Apply temporal filtering if --as-of is specified
    if let Some(ref as_of_date) = args.as_of {
        results.retain(|r| {
            if let Ok(Some(doc)) = db.get_document(&r.id) {
                let tags = parse_temporal_tags(&doc.content);
                !tags.is_empty() && tags.iter().any(|tag| overlaps_point(tag, as_of_date))
            } else {
                false
            }
        });
        results.truncate(args.limit);
    }

    // Apply temporal filtering if --during is specified
    if let Some((ref start, ref end)) = during_range {
        results.retain(|r| {
            if let Ok(Some(doc)) = db.get_document(&r.id) {
                let tags = parse_temporal_tags(&doc.content);
                !tags.is_empty() && tags.iter().any(|tag| overlaps_range(tag, start, end))
            } else {
                false
            }
        });
        results.truncate(args.limit);
    }

    // Apply --exclude-unknown filtering
    if args.exclude_unknown {
        results.retain(|r| {
            if let Ok(Some(doc)) = db.get_document(&r.id) {
                let tags = parse_temporal_tags(&doc.content);
                // Must have temporal tags and none of them can be Unknown
                !tags.is_empty()
                    && !tags
                        .iter()
                        .any(|tag| tag.tag_type == TemporalTagType::Unknown)
            } else {
                false
            }
        });
        results.truncate(args.limit);
    }

    // Apply --filter expressions
    apply_include_filters(&mut results, &filters, &db, args.limit);

    // Apply --exclude expressions (inverse of --filter)
    apply_exclude_filters(&mut results, &excludes, &db, args.limit);

    // Apply --exclude-type filtering (simpler syntax for type exclusion)
    if let Some(ref exclude_types) = args.exclude_type {
        results = filter_by_excluded_types(results, exclude_types, |r| r.doc_type.as_deref());
        results.truncate(args.limit);
    }

    // Apply recency boosting if --boost-recent is specified
    if args.boost_recent {
        let window_days = config.temporal.recency_window_days;
        let boost_factor = config.temporal.recency_boost_factor;

        for r in &mut results {
            if let Ok(Some(doc)) = db.get_document(&r.id) {
                let tags = parse_temporal_tags(&doc.content);
                let boost = calculate_recency_boost(&tags, window_days, boost_factor);
                r.relevance_score *= boost;
            }
        }

        // Re-sort by boosted relevance score (descending)
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(Ordering::Equal)
        });
        results.truncate(args.limit);
    }

    // Apply --sort ordering (after all filtering and boosting)
    match args.sort.as_str() {
        "relevance" => {
            // Already sorted by relevance (default from search)
        }
        "date" => {
            // Sort by file modification time (newest first)
            results.sort_by(|a, b| {
                let time_a = fs::metadata(&a.file_path).and_then(|m| m.modified()).ok();
                let time_b = fs::metadata(&b.file_path).and_then(|m| m.modified()).ok();
                time_b.cmp(&time_a) // Newest first
            });
        }
        "title" => {
            // Sort alphabetically by title (case-insensitive)
            results.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        "type" => {
            // Group by document type, then by relevance within type
            results.sort_by(|a, b| {
                let type_a = a.doc_type.as_deref().unwrap_or("unknown");
                let type_b = b.doc_type.as_deref().unwrap_or("unknown");
                match type_a.cmp(type_b) {
                    Ordering::Equal => b
                        .relevance_score
                        .partial_cmp(&a.relevance_score)
                        .unwrap_or(Ordering::Equal),
                    other => other,
                }
            });
        }
        _ => {} // Invalid sort option (shouldn't happen due to value_parser)
    }

    // Count mode: output only the number of results
    if args.count {
        if args.json {
            println!("{}", results.len());
        } else {
            println!("{} results", results.len());
        }
        return Ok(());
    }

    // Summary mode: show aggregate statistics instead of individual results
    if args.summary {
        let summary = compute_search_summary(&results, &db)?;
        if args.json {
            println!("{}", format_json(&summary)?);
        } else {
            print_search_summary(&summary, &args.query);
        }
        return Ok(());
    }

    if args.json {
        println!("{}", format_json(&results)?);
        return Ok(());
    }

    if results.is_empty() {
        if !args.quiet {
            println!("No results found for: {}", args.query);
        }
        return Ok(());
    }

    // Compact output: one line per result
    if args.compact {
        print_compact_results(&results);
        return Ok(());
    }

    print_detailed_results(&results);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::test_helpers::tests::make_result;
    use std::cmp::Ordering;

    #[test]
    fn test_sort_by_title() {
        let mut results = [
            make_result("a", "Zebra", None, 0.9),
            make_result("b", "Apple", None, 0.8),
            make_result("c", "banana", None, 0.7),
        ];

        // Sort by title (case-insensitive)
        results.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

        assert_eq!(results[0].title, "Apple");
        assert_eq!(results[1].title, "banana");
        assert_eq!(results[2].title, "Zebra");
    }

    #[test]
    fn test_sort_by_type() {
        let mut results = [
            make_result("a", "Doc A", Some("project"), 0.7),
            make_result("b", "Doc B", Some("person"), 0.9),
            make_result("c", "Doc C", Some("person"), 0.8),
        ];

        // Sort by type, then by relevance within type
        results.sort_by(|a, b| {
            let type_a = a.doc_type.as_deref().unwrap_or("unknown");
            let type_b = b.doc_type.as_deref().unwrap_or("unknown");
            match type_a.cmp(type_b) {
                Ordering::Equal => b
                    .relevance_score
                    .partial_cmp(&a.relevance_score)
                    .unwrap_or(Ordering::Equal),
                other => other,
            }
        });

        // person comes before project alphabetically
        assert_eq!(results[0].doc_type, Some("person".to_string()));
        assert_eq!(results[0].id, "b"); // Higher relevance within person type
        assert_eq!(results[1].doc_type, Some("person".to_string()));
        assert_eq!(results[1].id, "c");
        assert_eq!(results[2].doc_type, Some("project".to_string()));
    }

    #[test]
    fn test_sort_by_type_with_unknown() {
        let mut results = [
            make_result("a", "Doc A", None, 0.9),
            make_result("b", "Doc B", Some("person"), 0.8),
        ];

        // Sort by type (None becomes "unknown")
        results.sort_by(|a, b| {
            let type_a = a.doc_type.as_deref().unwrap_or("unknown");
            let type_b = b.doc_type.as_deref().unwrap_or("unknown");
            type_a.cmp(type_b)
        });

        // person comes before unknown alphabetically
        assert_eq!(results[0].doc_type, Some("person".to_string()));
        assert_eq!(results[1].doc_type, None);
    }
}
