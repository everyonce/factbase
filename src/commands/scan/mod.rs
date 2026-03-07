//! Scan command implementation.
//!
//! This module handles document indexing, verification, and maintenance operations.
//!
//! # Module Organization
//!
//! - `args` - Command-line argument parsing (ScanArgs)
//! - `verify` - Document integrity verification and fixes
//! - `prune` - Orphaned entry removal
//! - `stats` - Statistics display (stats-only, check)
//!
//! # Public API
//!
//! - [`ScanArgs`] - CLI arguments for scan command
//! - [`cmd_scan`] - Main scan command entry point

mod args;
mod assess;
mod prune;
mod stats;
mod verify;

pub use args::ScanArgs;

use super::{parse_since, resolve_repos, setup_database, setup_embedding_with_timeout};
use factbase::{
    config::validate_timeout, find_repo_for_path, format_json, full_scan, scan_all_repositories,
    DocumentProcessor, FileWatcher, ProgressReporter, ScanContext, ScanCoordinator, ScanOptions,
    Scanner,
};
use prune::cmd_scan_prune;
use stats::{cmd_scan_check, cmd_scan_stats_only};
use std::io::{self, IsTerminal};
use std::time::Duration as StdDuration;
use tracing::{error, info};
use verify::cmd_scan_verify;

#[tracing::instrument(
    name = "cmd_scan",
    skip(args),
    fields(repo = ?args.repo, dry_run = args.dry_run, watch = args.watch)
)]
pub async fn cmd_scan(args: ScanArgs) -> anyhow::Result<()> {
    let (config, db) = setup_database()?;
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let quiet = args.quiet || args.json;
    let json_output = args.json;

    let repos = db.list_repositories()?;
    let target_repos = resolve_repos(repos, args.repo.as_deref())?;

    // --progress and --no-progress are mutually exclusive
    if args.progress && args.no_progress {
        anyhow::bail!("--progress and --no-progress are mutually exclusive");
    }

    // Handle --stats-only: quick statistics without Ollama or database modifications
    if args.stats_only {
        return cmd_scan_stats_only(&target_repos, &scanner, json_output, quiet);
    }

    // Handle --assess: onboarding assessment without modifying anything
    if args.assess {
        return assess::cmd_scan_assess(&target_repos, &scanner, json_output, quiet, args.detailed);
    }

    // Handle --check: validate index integrity for CI
    if args.check {
        return cmd_scan_check(&db, &config, &target_repos, json_output, quiet);
    }

    // Handle --verify: check document integrity without re-indexing
    if args.verify {
        return cmd_scan_verify(&db, &target_repos, json_output, quiet, args.fix, args.yes);
    }

    // --fix without --verify is an error
    if args.fix {
        anyhow::bail!("--fix requires --verify flag");
    }

    // Handle --prune: remove orphaned database entries
    if args.prune {
        return cmd_scan_prune(
            &db,
            &target_repos,
            json_output,
            quiet,
            args.hard,
            args.dry_run,
            args.yes,
        );
    }

    // --hard without --prune is an error
    if args.hard {
        anyhow::bail!("--hard requires --prune flag");
    }

    let processor = DocumentProcessor::new();

    if args.dry_run && !quiet {
        println!("Dry run mode - no changes will be made");
    }

    // Parse --since filter if provided
    let since = if let Some(ref since_str) = args.since {
        let dt = parse_since(since_str)?;
        if !quiet {
            println!(
                "Filtering files modified since {}",
                dt.format("%Y-%m-%d %H:%M:%S UTC")
            );
        }
        Some(dt)
    } else {
        None
    };

    // Validate timeout if provided
    if let Some(timeout) = args.timeout {
        validate_timeout(timeout)?;
    }

    let embedding = setup_embedding_with_timeout(&config, args.timeout).await;

    // Dimension mismatch detection: compare provider dim vs DB metadata
    let provider_dim = embedding.dimension();
    let stored_dim = db.get_stored_embedding_dim()?;
    if let Some(db_dim) = stored_dim {
        if db_dim != provider_dim {
            if args.reindex {
                if !quiet {
                    eprintln!(
                        "Dimension change detected ({db_dim} → {provider_dim}). Rebuilding embedding tables."
                    );
                }
                db.rebuild_embedding_tables(provider_dim)?;
                db.set_embedding_info(&config.embedding.model, provider_dim)?;
            } else {
                anyhow::bail!(
                    "Embedding dimension mismatch: database has {db_dim}-dim vectors but current provider uses {provider_dim}-dim.\n\
                     Run `factbase scan --reindex` to rebuild all embeddings with the new provider."
                );
            }
        }
    } else {
        // First scan or empty DB — record metadata and ensure tables match
        let actual_table_dim = db.get_embedding_dimension()?;
        if actual_table_dim.is_some() && actual_table_dim != Some(provider_dim) {
            // Table exists with different dimension (e.g., default 1024 but provider is 384)
            if !quiet {
                eprintln!(
                    "Adjusting embedding tables for {provider_dim}-dim vectors."
                );
            }
            db.rebuild_embedding_tables(provider_dim)?;
        }
        db.set_embedding_info(&config.embedding.model, provider_dim)?;
    }

    let link_detector = factbase::LinkDetector::new();

    let batch_size = args
        .batch_size
        .unwrap_or(config.processor.embedding_batch_size);

    // Warn about very large batch sizes
    if batch_size > 100 && !quiet {
        eprintln!(
            "{}",
            factbase::format_warning(&format!(
                "Large batch size ({batch_size}) may increase memory usage significantly"
            ))
        );
    }

    // Determine progress bar visibility:
    // --progress forces on, --no-progress forces off, otherwise auto-detect TTY
    let show_progress = if args.progress {
        true
    } else if args.no_progress || quiet {
        false
    } else {
        IsTerminal::is_terminal(&io::stdout())
    };

    let opts = ScanOptions {
        verbose: args.detailed && !quiet,
        dry_run: args.dry_run,
        show_progress,
        check_duplicates: args.check_duplicates,
        collect_stats: args.stats || json_output,
        since,
        force_reindex: args.reindex,
        embedding_batch_size: batch_size,
        skip_links: args.no_links,
        skip_embeddings: args.no_embed,
        force_relink: args.relink,
        ..ScanOptions::from_config(&config)
    };

    if args.no_links && !quiet {
        println!("Link detection skipped. Run `factbase scan` without --no-links to detect links.");
    }

    if args.no_embed && !quiet {
        println!("Embedding generation skipped. Use `factbase embeddings import` to load embeddings.");
    }

    if args.reindex && !quiet {
        println!("Reindex mode - regenerating embeddings for all documents");
    }

    if args.relink && !quiet {
        println!("Relink mode - detecting links for all documents");
    }

    let progress = ProgressReporter::Cli { quiet };

    let ctx = ScanContext {
        scanner: &scanner,
        processor: &processor,
        embedding: &embedding,
        link_detector: &link_detector,
        opts: &opts,
        progress: &progress,
    };

    let result = if target_repos.len() == 1 {
        full_scan(&target_repos[0], &db, &ctx).await?
    } else {
        scan_all_repositories(&db, &ctx).await?
    };

    // Check if scan was interrupted
    let was_interrupted = result.interrupted;

    if json_output {
        println!("{}", format_json(&result)?);
    } else if !quiet {
        if was_interrupted {
            println!("Scan interrupted - partial results saved");
        }
        println!("{result}");
        if result.links_detected > 0 {
            println!("{} links detected", result.links_detected);
        }
        if result.fact_embeddings_generated > 0 {
            println!("{} fact embeddings generated", result.fact_embeddings_generated);
        }
        if result.fact_embeddings_needed > 0 {
            println!("{} documents need fact embeddings (run: factbase check --mode embeddings)", result.fact_embeddings_needed);
        }
        if let Some(ref stats) = result.stats {
            println!("\nTiming:");
            println!(
                "  Discovery: {:.2}s, Parsing: {:.2}s, Embedding: {:.2}s ({} docs)",
                stats.file_discovery_ms as f64 / 1000.0,
                stats.parsing_ms as f64 / 1000.0,
                stats.embedding_ms as f64 / 1000.0,
                stats.docs_embedded,
            );
            println!(
                "  DB writes: {:.2}s, Links: {:.2}s ({} docs)",
                stats.db_write_ms as f64 / 1000.0,
                stats.link_detection_ms as f64 / 1000.0,
                stats.docs_link_detected
            );
            println!("  Total: {:.2}s", stats.total_ms as f64 / 1000.0);

            // Show slowest files if any were processed
            if !stats.slowest_files.is_empty() {
                println!("\nSlowest files:");
                for file in &stats.slowest_files {
                    println!(
                        "  {:.2}s  {} ({} bytes)",
                        file.embedding_ms as f64 / 1000.0,
                        file.file_path,
                        file.size_bytes
                    );
                }
            }
        }
        if let Some(ref temporal) = result.temporal_stats {
            if temporal.total_facts > 0 {
                let warn_icon = if temporal.below_threshold_docs > 0 {
                    "⚠ "
                } else {
                    ""
                };
                println!(
                    "{}Temporal coverage: {:.0}% ({}/{} facts)",
                    warn_icon,
                    temporal.coverage * 100.0,
                    temporal.facts_with_tags,
                    temporal.total_facts
                );
                println!(
                    "Source coverage: {:.0}% ({}/{} facts)",
                    temporal.source_coverage * 100.0,
                    temporal.facts_with_sources,
                    temporal.total_facts
                );
                if temporal.below_threshold_docs > 0 {
                    println!(
                        "  {} document(s) below {:.0}% threshold",
                        temporal.below_threshold_docs,
                        config.temporal.min_coverage * 100.0
                    );
                }
            }
        }
        if !result.duplicates.is_empty() {
            println!("\nPotential duplicates found:");
            for dup in &result.duplicates {
                println!(
                    "  {} ({}) <-> {} ({}) - {:.1}% similar",
                    dup.doc1_title,
                    dup.doc1_id,
                    dup.doc2_title,
                    dup.doc2_id,
                    dup.similarity * 100.0
                );
            }
        }
    }

    if args.watch {
        if !quiet {
            println!("\nWatching for changes... (Press Ctrl+C to stop)");
        }

        let mut watcher =
            FileWatcher::new(config.watcher.debounce_ms, &config.watcher.ignore_patterns)?;

        for repo in &target_repos {
            watcher.watch_directory(&repo.path)?;
        }

        let scan_coordinator = ScanCoordinator::new();
        let watch_opts = ScanOptions {
            verbose: args.detailed && !quiet,
            dry_run: args.dry_run,
            embedding_batch_size: args
                .batch_size
                .unwrap_or(config.processor.embedding_batch_size),
            skip_links: args.no_links,
            ..ScanOptions::from_config(&config)
        };

        loop {
            if let Some(changed_paths) = watcher.try_recv() {
                for path in &changed_paths {
                    info!("File changed: {}", path.display());
                }

                if let Some(path) = changed_paths.first() {
                    if let Some(repo) = find_repo_for_path(path, &target_repos) {
                        if scan_coordinator.try_start() {
                            info!("Rescanning repository: {}", repo.id);
                            let watch_ctx = ScanContext {
                                scanner: &scanner,
                                processor: &processor,
                                embedding: &embedding,
                                link_detector: &link_detector,
                                opts: &watch_opts,
                                progress: &progress,
                            };
                            match full_scan(repo, &db, &watch_ctx).await {
                                Ok(r) => {
                                    if !quiet {
                                        println!("Rescan: {r}")
                                    }
                                }
                                Err(e) => error!("Scan error: {}", e),
                            }
                            scan_coordinator.finish();
                        }
                    }
                }
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    if !quiet { println!("\nStopping file watcher..."); }
                    break;
                }
                _ = tokio::time::sleep(StdDuration::from_millis(100)) => {}
            }
        }
    }

    // Exit with non-zero code if interrupted
    if was_interrupted {
        anyhow::bail!("Scan interrupted by signal");
    }

    Ok(())
}
