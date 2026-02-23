use super::{
    args::GrepArgs,
    filter_by_excluded_types,
    output::{compute_grep_stats, highlight_matches, should_highlight_output},
    parse_since_filter, print_output, setup_database_only, OutputFormat,
};
use crate::commands::setup_database;
use chrono::{DateTime, Utc};
use factbase::{ContentSearchParams, ProgressReporter};
use regex::RegexBuilder;
use serde::Serialize;

/// Run a single grep search (used by both cmd_grep and watch mode)
pub(super) fn run_single_grep(args: &GrepArgs) -> anyhow::Result<()> {
    // Parse --since if provided
    let since: Option<DateTime<Utc>> = parse_since_filter(&args.since)?;

    let db = setup_database_only()?;
    let progress = ProgressReporter::Cli { quiet: args.quiet };

    let params = ContentSearchParams {
        pattern: &args.pattern,
        limit: args.limit,
        doc_type: args.doc_type.as_deref(),
        repo_id: args.repo.as_deref(),
        context_lines: args.context,
        since,
        progress: &progress,
    };
    let results = db.search_content(&params)?;

    // Apply --exclude-type filtering
    let results = if let Some(ref exclude_types) = args.exclude_type {
        filter_by_excluded_types(results, exclude_types, |r| r.doc_type.as_deref())
    } else {
        results
    };

    // --json flag overrides --format
    let format = OutputFormat::resolve(args.json, args.format);

    // Handle --stats mode
    if args.stats {
        let stats = compute_grep_stats(&results);
        let pattern = args.pattern.clone();
        print_output(format, &stats, || {
            if stats.total_matches == 0 {
                println!("No matches found for '{pattern}'");
            } else {
                println!(
                    "{} matches in {} documents across {} repositories",
                    stats.total_matches, stats.document_count, stats.repository_count
                );
                if !stats.top_files.is_empty() {
                    println!("\nTop files:");
                    for file in &stats.top_files {
                        println!(
                            "  {} ({} matches) - {}",
                            file.title, file.match_count, file.file_path
                        );
                    }
                }
            }
        })?;
        return Ok(());
    }

    // Handle --count mode
    if args.count {
        let total_matches: usize = results.iter().map(|r| r.matches.len()).sum();
        #[derive(Serialize)]
        struct CountResult {
            count: usize,
        }
        print_output(
            format,
            &CountResult {
                count: total_matches,
            },
            || {
                println!("{total_matches}");
            },
        )?;
        return Ok(());
    }

    // Determine if highlighting should be enabled
    let use_highlight = should_highlight_output(args, &format);
    let pattern = args.pattern.clone();
    let quiet = args.quiet;
    let context = args.context;

    print_output(format, &results, || {
        if results.is_empty() {
            if !quiet {
                println!("No matches found for '{pattern}'");
            }
        } else {
            // Build regex for highlighting
            let highlight_regex = if use_highlight {
                RegexBuilder::new(&regex::escape(&pattern))
                    .case_insensitive(true)
                    .build()
                    .ok()
            } else {
                None
            };

            for result in &results {
                println!("\n{} [{}] ({})", result.title, result.id, result.file_path);
                for (i, m) in result.matches.iter().enumerate() {
                    // Add separator between match groups when showing context
                    if context > 0 && i > 0 {
                        println!("  --");
                    }

                    if context > 0 && !m.context.is_empty() {
                        // Show context lines with highlighting
                        for context_line in m.context.lines() {
                            let highlighted = if let Some(ref re) = highlight_regex {
                                highlight_matches(context_line, re)
                            } else {
                                context_line.to_string()
                            };
                            println!("  {highlighted}");
                        }
                    } else {
                        // No context - just show the matching line
                        let line_text = m.line.trim();
                        let highlighted = if let Some(ref re) = highlight_regex {
                            highlight_matches(line_text, re)
                        } else {
                            line_text.to_string()
                        };
                        println!("  Line {}: {}", m.line_number, highlighted);
                    }
                }
            }
            if !quiet {
                println!("\n{} document(s) matched", results.len());
            }
        }
    })?;

    Ok(())
}

/// Run grep in watch mode - re-run search when files change
pub(super) fn run_grep_watch_mode(args: &GrepArgs) -> anyhow::Result<()> {
    use super::watch_helper::{run_sync_watch_loop, WatchContext};
    use crate::commands::utils::resolve_repos;

    let (config, db) = setup_database()?;
    let repos = resolve_repos(db.list_repositories()?, args.repo.as_deref())?;
    let mut ctx = WatchContext::new(&config, repos)?;

    let pattern = args.pattern.clone();
    run_sync_watch_loop(
        &mut ctx,
        || {
            println!("Searching for: \"{pattern}\"");
            println!("{}", "=".repeat(50));
        },
        || run_single_grep(args),
    )
}
