//! Lint command for checking knowledge base quality.
//!
//! This module provides the `factbase lint` command which checks for:
//! - Orphan documents (no incoming or outgoing links)
//! - Broken manual `[[id]]` links
//! - Stub documents (very short content)
//! - Unknown document types
//! - Stale documents (when `--max-age` specified)
//! - Duplicate documents (when `--check-duplicates` specified)
//! - Temporal tag validation (when `--check-temporal` specified)
//! - Source footnote validation (when `--check-sources` specified)
//!
//! # Module Organization
//!
//! - `args` - Command argument parsing (`LintArgs`)
//! - `checks` - Individual lint check functions
//! - `review` - Review question generation
//! - `output` - Output formatting structs
//! - `incremental` - Incremental lint tracking (timestamps, filtering)
//! - `watch` - Watch mode for continuous linting
//! - `execute` - Lint execution helpers
//!
//! # Public API
//!
//! Only [`LintArgs`] and [`cmd_lint`] are exported for use by main.rs.
//! Internal submodules are used within the lint command implementation.

mod args;
mod checks;
mod execute;
mod incremental;
mod output;
mod review;
mod watch;

// Re-exports for external use (only LintArgs is used by main.rs)
pub use args::LintArgs;

// Internal imports from submodules
use checks::{lint_document_content, DocLintResult, ParallelLintOptions};
use output::{
    print_lint_result, ExportedDocQuestions, LintResult, LintSourceStats, LintTemporalStats,
};

use super::{
    parse_since_filter, resolve_repos, setup_database, setup_embedding_with_timeout,
    setup_llm_with_timeout, setup_review_llm_with_timeout, OutputFormat,
};
use chrono::Utc;
use factbase::{config::validate_timeout, format_json, format_yaml, ProgressReporter};
use incremental::{filter_documents_by_time, get_effective_since, update_lint_timestamps};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use tracing::info;
use watch::run_lint_watch_mode;

#[tracing::instrument(
    name = "cmd_lint",
    skip(args),
    fields(repo = ?args.repo, check_temporal = args.check_temporal, check_sources = args.check_sources, review = args.review)
)]
pub async fn cmd_lint(args: LintArgs) -> anyhow::Result<()> {
    let (config, db) = setup_database()?;
    let repos = db.list_repositories()?;

    // Parse --since filter if provided
    let since = parse_since_filter(&args.since)?;
    if let Some(dt) = &since {
        info!(
            "Filtering files modified since {}",
            dt.format("%Y-%m-%d %H:%M:%S")
        );
    }

    // Determine effective output format (--json is shorthand for --format json)
    let format = OutputFormat::resolve(args.json, args.format);
    let is_table_format = matches!(format, OutputFormat::Table);

    // Apply --check-all flag (equivalent to --check-temporal --check-sources --check-duplicates)
    let check_temporal = args.check_temporal || args.check_all;
    let check_sources = args.check_sources || args.check_all;
    let check_duplicates = args.check_duplicates || args.check_all;

    // Validate timeout if provided
    if let Some(timeout) = args.timeout {
        validate_timeout(timeout)?;
    }

    // Initialize ReviewLlm when --review flag is used
    // Currently question generation is rule-based, but this prepares for future LLM-based generation
    let _review_llm = if args.review {
        let review_llm = setup_review_llm_with_timeout(&config, args.timeout).await;
        info!("Review LLM initialized with model: {}", review_llm.model());
        Some(review_llm)
    } else {
        None
    };

    // Initialize embedding + LLM providers when --cross-check is used
    let cross_check_providers = if args.cross_check {
        let embedding = setup_embedding_with_timeout(&config, args.timeout).await;
        let llm = setup_llm_with_timeout(&config, args.timeout).await;
        info!("Cross-check providers initialized");
        Some((embedding, llm))
    } else {
        None
    };

    let repos_to_lint = resolve_repos(repos, args.repo.as_deref())?;

    // Unified progress reporter for both review and cross-check passes
    let progress = ProgressReporter::Cli { quiet: args.quiet };

    let mut warnings = 0;
    let mut errors = 0;
    let mut fixed = 0;

    // Document type distribution aggregation
    let mut type_counts: HashMap<String, usize> = HashMap::new();

    // Temporal stats aggregation
    let mut temporal_stats = if check_temporal {
        Some(LintTemporalStats::default())
    } else {
        None
    };

    // Source stats aggregation
    let mut source_stats = if check_sources {
        Some(LintSourceStats::default())
    } else {
        None
    };

    // Collection for exported questions when --export-questions is used
    // Pre-allocate for typical case of ~16 documents with questions
    let mut exported_questions: Vec<ExportedDocQuestions> = Vec::with_capacity(16);

    // Track lint start time for updating last_lint_at
    let lint_start_time = Utc::now();

    for repo in &repos_to_lint {
        if is_table_format {
            println!("Linting repository: {} ({})", repo.name, repo.id);
        }

        let all_docs = db.list_documents(None, Some(&repo.id), None, 10000)?;

        // Determine the effective since filter using helper
        let effective_since = get_effective_since(since, args.incremental, repo, is_table_format);

        // Filter documents by modification time if filtering is active
        let docs: Vec<_> = if let Some(since_dt) = effective_since {
            filter_documents_by_time(all_docs, since_dt, &repo.path)
        } else {
            all_docs
        };

        if is_table_format && effective_since.is_some() {
            println!(
                "  Incremental lint: {}/{} documents to check",
                docs.len(),
                db.list_documents(None, Some(&repo.id), None, 10000)?.len()
            );
        }

        // Show batch info if batching is enabled
        let batch_size = args.batch_size;
        let total_batches = if batch_size > 0 {
            docs.len().div_ceil(batch_size)
        } else {
            1
        };
        if is_table_format && batch_size > 0 && total_batches > 1 {
            println!(
                "  Processing {} documents in {} batches of {}",
                docs.len(),
                total_batches,
                batch_size
            );
        }

        let doc_ids: HashSet<_> = docs.iter().map(|d| d.id.as_str()).collect();

        // Aggregate document type counts
        for doc in &docs {
            let doc_type = doc
                .doc_type
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *type_counts.entry(doc_type).or_insert(0) += 1;
        }

        let allowed_types = repo
            .perspective
            .as_ref()
            .and_then(|p| p.allowed_types.as_ref());

        // Report review phase via ProgressReporter
        if args.review {
            progress.phase("Generating review questions");
        }

        // Process documents in batches if batch_size > 0, otherwise process all at once
        let doc_batches: Vec<&[factbase::Document]> = if batch_size > 0 {
            docs.chunks(batch_size).collect()
        } else {
            vec![&docs[..]]
        };

        for (batch_idx, batch_docs) in doc_batches.iter().enumerate() {
            // Show batch progress
            if is_table_format && !args.quiet && batch_size > 0 && total_batches > 1 {
                println!(
                    "  Batch {}/{}: processing {} documents...",
                    batch_idx + 1,
                    total_batches,
                    batch_docs.len()
                );
            }

            // Parallel processing of temporal and source checks for this batch
            let parallel_results: Option<Vec<DocLintResult>> = if args.parallel {
                if is_table_format && !args.quiet && batch_size == 0 {
                    println!("  Processing {} documents in parallel...", batch_docs.len());
                }
                let opts = ParallelLintOptions {
                    check_temporal,
                    check_sources,
                    min_length: args.min_length,
                    max_age_days: args.max_age,
                    allowed_types: allowed_types.cloned(),
                };
                let results: Vec<DocLintResult> = batch_docs
                    .par_iter()
                    .map(|doc| {
                        lint_document_content(
                            &doc.content,
                            &doc.id,
                            &doc.title,
                            doc.doc_type.as_deref(),
                            doc.file_modified_at,
                            doc.indexed_at,
                            &opts,
                        )
                    })
                    .collect();
                Some(results)
            } else {
                None
            };

            // Aggregate parallel results if available
            if let Some(ref results) = parallel_results {
                for doc_result in results.iter() {
                    // Print messages
                    if is_table_format {
                        for msg in &doc_result.messages {
                            println!("{msg}");
                        }
                    }
                    errors += doc_result.errors;
                    warnings += doc_result.warnings;

                    // Aggregate temporal stats
                    if let Some(ref mut ts) = temporal_stats {
                        ts.total_facts += doc_result.temporal_total_facts;
                        ts.facts_with_tags += doc_result.temporal_facts_with_tags;
                        ts.format_errors += doc_result.temporal_format_errors;
                        ts.conflicts += doc_result.temporal_conflicts;
                        ts.illogical_sequences += doc_result.temporal_illogical_sequences;
                        for (k, v) in &doc_result.temporal_by_type {
                            *ts.by_type.entry(k.clone()).or_insert(0) += v;
                        }
                    }

                    // Aggregate source stats
                    if let Some(ref mut ss) = source_stats {
                        ss.total_facts += doc_result.source_total_facts;
                        ss.facts_with_sources += doc_result.source_facts_with_sources;
                        ss.orphan_refs += doc_result.source_orphan_refs;
                        ss.orphan_defs += doc_result.source_orphan_defs;
                        for (k, v) in &doc_result.source_by_type {
                            *ss.by_type.entry(k.clone()).or_insert(0) += v;
                        }
                    }
                }
            }

            // Calculate base index for progress bar when using batches
            let batch_base_idx = batch_idx * batch_size;

            // Batch fetch links for all documents in this batch (2 queries instead of 2*N)
            let batch_doc_ids: Vec<&str> = batch_docs.iter().map(|d| d.id.as_str()).collect();
            let batch_links = db.get_links_for_documents(&batch_doc_ids)?;

            for (doc_idx, doc) in batch_docs.iter().enumerate() {
                // Report progress via ProgressReporter
                let global_idx = if batch_size > 0 {
                    batch_base_idx + doc_idx
                } else {
                    doc_idx
                };
                if args.review {
                    progress.report(global_idx + 1, docs.len(), &doc.title);
                }

                // Get pre-fetched links for this document
                let (links_from, links_to) = batch_links
                    .get(&doc.id)
                    .map_or((&[][..], &[][..]), |(out, inc)| {
                        (out.as_slice(), inc.as_slice())
                    });

                // Check for broken links using helper (also handles orphan detection)
                let link_result = execute::check_document_links(
                    doc,
                    &doc_ids,
                    links_from,
                    links_to,
                    args.fix,
                    args.dry_run,
                    is_table_format,
                )?;
                warnings += link_result.warnings;
                errors += link_result.errors;
                fixed += link_result.fixed;

                // Skip stub/type/stale checks if already done in parallel
                if parallel_results.is_none() {
                    warnings += execute::check_document_basics(
                        doc,
                        args.min_length,
                        args.max_age,
                        allowed_types,
                        is_table_format,
                    );
                }

                // Check temporal tag validity and collect stats (skip if already done in parallel)
                if check_temporal && parallel_results.is_none() {
                    let (temp_warnings, temp_errors) =
                        execute::check_temporal_tags(doc, &mut temporal_stats, is_table_format);
                    warnings += temp_warnings;
                    errors += temp_errors;
                }

                // Check for orphan source references - skip if already done in parallel
                if check_sources && parallel_results.is_none() {
                    let (src_warnings, src_errors) =
                        execute::check_source_refs(doc, &mut source_stats, is_table_format);
                    warnings += src_warnings;
                    errors += src_errors;
                }

                // Generate review questions when --review flag is set
                if args.review {
                    let opts = execute::ReviewQuestionOptions {
                        min_similarity: args.min_similarity,
                        dry_run: args.dry_run,
                        export_mode: args.export_questions.is_some(),
                        is_table_format,
                        max_age: args.max_age,
                    };
                    if let Some(exported) =
                        execute::generate_review_questions(doc, repo, &db, &opts)?
                    {
                        exported_questions.push(exported);
                    }
                }
            }
        } // End of batch loop

        // Cross-document fact validation pass (--cross-check)
        if let Some((ref embedding, ref llm)) = cross_check_providers {
            progress.phase("Cross-document validation");
            if is_table_format && !args.quiet {
                println!("  Cross-checking {} documents...", docs.len());
            }
            let mut skipped = 0usize;
            for (i, doc) in docs.iter().enumerate() {
                // Skip documents whose content hasn't changed since last cross-check
                if !db.needs_cross_check(&doc.id)? {
                    skipped += 1;
                    continue;
                }
                if is_table_format && !args.quiet {
                    eprint!("\r  Cross-checking document {} of {}...", i + 1, docs.len());
                }
                progress.report(i + 1, docs.len(), &format!("Cross-checking {}", doc.title));
                match factbase::cross_validate_document(
                    &doc.content,
                    &doc.id,
                    &db,
                    embedding.as_ref(),
                    llm.as_ref(),
                )
                .await
                {
                    Ok(questions) => {
                        if !questions.is_empty() {
                            warnings += questions.len();
                            if is_table_format {
                                eprintln!();
                                for q in &questions {
                                    println!(
                                        "  CROSS: {} [{}] @q[{}] {}",
                                        doc.title,
                                        doc.id,
                                        q.question_type.as_str(),
                                        q.description
                                    );
                                }
                            }
                            // Append questions to file unless dry-run
                            if !args.dry_run {
                                let updated =
                                    factbase::append_review_questions(&doc.content, &questions);
                                let abs_path =
                                    std::path::Path::new(&repo.path).join(&doc.file_path);
                                fs::write(&abs_path, &updated)?;
                            }
                        }
                        // Mark document as cross-checked so subsequent runs skip it
                        if !args.dry_run {
                            db.set_cross_check_hash(&doc.id)?;
                        }
                    }
                    Err(e) => {
                        if is_table_format {
                            eprintln!();
                            eprintln!(
                                "  WARN: Cross-check failed for {} [{}]: {}",
                                doc.title, doc.id, e
                            );
                        }
                        warnings += 1;
                    }
                }
            }
            if is_table_format && !args.quiet {
                eprintln!("\r  Cross-check complete.                    ");
                if skipped > 0 {
                    println!("  Skipped {skipped} unchanged document(s).");
                }
            }
        }

        if is_table_format && !type_counts.is_empty() {
            println!("  Type distribution:");
            let mut types: Vec<_> = type_counts.iter().collect();
            types.sort_by(|a, b| b.1.cmp(a.1));
            for (doc_type, count) in types {
                let marker = if let Some(allowed) = allowed_types {
                    if allowed.iter().any(|t| t.to_lowercase() == *doc_type) {
                        "✓"
                    } else {
                        "✗"
                    }
                } else {
                    " "
                };
                println!("    {marker} {doc_type}: {count}");
            }
        }

        // Check for duplicate documents
        if check_duplicates {
            warnings +=
                execute::check_duplicates(&docs, &db, args.min_similarity, is_table_format)?;
        }
    }

    // Update last_lint_at timestamp for incremental mode
    if args.incremental && !args.dry_run {
        update_lint_timestamps(&db, &repos_to_lint, lint_start_time, is_table_format)?;
    }

    // Calculate final coverage percentage
    if let Some(ref mut ts) = temporal_stats {
        ts.coverage_percent = if ts.total_facts > 0 {
            (ts.facts_with_tags as f32 / ts.total_facts as f32) * 100.0
        } else {
            0.0
        };
    }

    // Calculate final source coverage percentage
    if let Some(ref mut ss) = source_stats {
        ss.coverage_percent = if ss.total_facts > 0 {
            (ss.facts_with_sources as f32 / ss.total_facts as f32) * 100.0
        } else {
            0.0
        };
    }

    // Write exported questions to file if --export-questions was used
    if let Some(ref export_path) = args.export_questions {
        let total_questions: usize = exported_questions.iter().map(|d| d.questions.len()).sum();
        let total_docs = exported_questions.len();

        // Determine format from file extension
        let output = if super::utils::ends_with_ext(export_path, ".yaml") || super::utils::ends_with_ext(export_path, ".yml") {
            format_yaml(&exported_questions)?
        } else {
            // Default to JSON
            format_json(&exported_questions)?
        };

        fs::write(export_path, &output)?;

        if is_table_format {
            println!(
                "\nExported {total_questions} question(s) from {total_docs} document(s) to {export_path}"
            );
        }
    }

    // Output results
    let result = LintResult {
        errors,
        warnings,
        fixed,
        type_distribution: type_counts,
        temporal_stats,
        source_stats,
    };

    print_lint_result(&result, format)?;

    // Watch mode: monitor for file changes and re-lint
    if args.watch {
        use crate::commands::watch_helper::WatchContext;

        if !args.quiet {
            println!("\nWatching for changes... (Press Ctrl+C to stop)");
        }

        let mut ctx = WatchContext::new(&config, repos_to_lint)?;
        run_lint_watch_mode(&mut ctx, &db, args.min_length, args.quiet)?;
    }

    Ok(())
}
