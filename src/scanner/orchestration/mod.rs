//! Scan orchestration - full_scan and scan_all_repositories

mod duplicates;
mod embedding;
mod links;
mod preread;
mod results;
mod types;

use std::collections::HashSet;
use std::fs;
use std::time::Instant;
use tracing::{info, warn};

use crate::models::{FileTimingInfo, TemporalScanStats};
use crate::{
    calculate_fact_stats, Database, DocumentProcessor, EmbeddingProvider, LinkDetector, Repository,
    ScanResult, ScanStats,
};

use super::options::ScanOptions;
use super::progress::OptionalProgress;
use duplicates::check_duplicates;
use embedding::{run_embedding_phase, EmbeddingPhaseInput};
use links::{run_link_detection_phase, LinkPhaseInput};
use preread::pre_read_files;
use results::{build_interrupted_result, InterruptedResultParams};
use types::{PendingDoc, PreReadFile};

/// Perform a full scan of a repository
#[tracing::instrument(
    name = "full_scan",
    skip(db, scanner, processor, embedding, link_detector, opts),
    fields(repo_id = %repo.id, repo_path = %repo.path.display())
)]
pub async fn full_scan(
    repo: &Repository,
    db: &Database,
    scanner: &super::Scanner,
    processor: &DocumentProcessor,
    embedding: &dyn EmbeddingProvider,
    link_detector: &LinkDetector,
    opts: &ScanOptions,
) -> anyhow::Result<ScanResult> {
    let scan_start = Instant::now();
    let file_discovery_start = Instant::now();

    let files = scanner.find_markdown_files(&repo.path);
    let known = db.get_documents_for_repo(&repo.id)?;
    let mut seen = HashSet::new();
    let mut changed_ids = HashSet::new();
    let mut result = ScanResult::default();
    let total_files = files.len();

    // Temporal stats tracking
    let mut total_facts = 0usize;
    let mut facts_with_tags = 0usize;
    let mut below_threshold_docs = 0usize;

    let file_discovery_ms = file_discovery_start.elapsed().as_millis() as u64;

    info!("Scanning {} files in {}", total_files, repo.path.display());

    // Create progress bar if enabled and enough files
    let pb = if opts.show_progress && !opts.verbose && !opts.dry_run {
        OptionalProgress::new(
            total_files as u64,
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} (ETA: {eta})",
            "scanning",
            10,
        )
    } else {
        OptionalProgress::none()
    };

    if !opts.dry_run {
        db.begin_transaction()?;
    }

    let mut pending: Vec<PendingDoc> = Vec::with_capacity(files.len());

    // Pre-read files in parallel (I/O bound) - includes parsing (hash, ID extraction)
    let parsing_start = Instant::now();
    let pre_read: Vec<PreReadFile> = pre_read_files(files);
    let parsing_ms = parsing_start.elapsed().as_millis() as u64;

    // Pass 1: Process pre-read files sequentially (needs DB access)
    for (idx, pre) in pre_read.into_iter().enumerate() {
        pb.set_position((idx + 1) as u64);
        if total_files >= 50 && (idx + 1) % 25 == 0 && !opts.show_progress {
            info!("Progress: {}/{} files", idx + 1, total_files);
        }

        // Skip files older than --since filter
        if let Some(since) = opts.since {
            if let Some(modified_at) = pre.modified_at {
                if modified_at < since {
                    if opts.verbose || opts.dry_run {
                        println!("  SKIP {} (older than --since)", pre.path.display());
                    }
                    continue;
                }
            }
        }

        let content = match pre.content {
            Ok(c) => c,
            Err(e) => {
                if opts.verbose || opts.dry_run {
                    println!("  SKIP {}: {}", pre.path.display(), e);
                }
                warn!("Skip {}: {}", pre.path.display(), e);
                continue;
            }
        };

        let hash = pre.hash.expect("hash should exist when content is Ok");
        let id = if let Some(id) = pre.existing_id {
            id
        } else if opts.dry_run {
            processor.generate_unique_id(db)
        } else {
            let id = processor.generate_unique_id(db);
            let new_content = processor.inject_header(&content, &id);
            fs::write(&pre.path, &new_content)?;
            id
        };

        seen.insert(id.clone());

        let relative = pre
            .path
            .strip_prefix(&repo.path)
            .unwrap_or(&pre.path)
            .to_string_lossy()
            .to_string();

        let is_new = !known.contains_key(&id);
        let is_modified = known.get(&id).map(|d| d.file_hash != hash).unwrap_or(false);
        let is_moved = known
            .get(&id)
            .map(|d| d.file_path != relative)
            .unwrap_or(false);

        // Skip unchanged documents unless force_reindex is set
        if !is_new && !is_modified && !is_moved && !opts.force_reindex {
            if opts.verbose || opts.dry_run {
                println!("  UNCHANGED {}", pre.path.display());
            }
            result.unchanged += 1;
            continue;
        }

        // When force_reindex is set, treat unchanged docs as needing reindex
        let is_reindex = opts.force_reindex && !is_new && !is_modified && !is_moved;

        changed_ids.insert(id.clone());

        let title = processor.extract_title(&content, &pre.path);

        if is_new {
            if opts.verbose || opts.dry_run {
                println!("  NEW {} ({})", relative, title);
            }
            result.added += 1;
        } else if is_reindex {
            // Force reindex - content unchanged but embeddings regenerated
            if opts.verbose || opts.dry_run {
                println!("  REINDEX {} ({})", relative, title);
            }
            result.reindexed += 1;
        } else if is_moved && !is_modified {
            // File moved but content unchanged - counts as moved only
            if opts.verbose || opts.dry_run {
                let old_path = known.get(&id).map(|d| d.file_path.as_str()).unwrap_or("?");
                println!("  MOVED {} -> {} ({})", old_path, relative, title);
            }
            result.moved += 1;
        } else if is_moved && is_modified {
            // File moved AND content modified - counts as updated (with move note)
            if opts.verbose || opts.dry_run {
                let old_path = known.get(&id).map(|d| d.file_path.as_str()).unwrap_or("?");
                println!("  UPDATED+MOVED {} -> {} ({})", old_path, relative, title);
            }
            result.updated += 1;
        } else {
            // Content modified, same path
            if opts.verbose || opts.dry_run {
                println!("  UPDATED {} ({})", relative, title);
            }
            result.updated += 1;
        }

        if opts.dry_run {
            continue;
        }

        let doc_type = processor.derive_type(&pre.path, &repo.path);

        // Validate type against allowed_types if configured
        if let Some(ref perspective) = repo.perspective {
            if let Some(ref allowed) = perspective.allowed_types {
                if !allowed.iter().any(|t| t.to_lowercase() == doc_type) {
                    warn!(
                        "Unknown type '{}' for {}: allowed types are {:?}",
                        doc_type, relative, allowed
                    );
                    if opts.verbose {
                        println!(
                            "  WARN: Unknown type '{}' (allowed: {:?})",
                            doc_type, allowed
                        );
                    }
                }
            }
        }

        // Calculate temporal stats for this document
        let fact_stats = calculate_fact_stats(&content);
        total_facts += fact_stats.total_facts;
        facts_with_tags += fact_stats.facts_with_tags;

        // Check if below threshold and warn
        let is_below_threshold =
            fact_stats.total_facts > 0 && fact_stats.coverage < opts.min_coverage;
        if is_below_threshold {
            below_threshold_docs += 1;
            if opts.verbose {
                println!(
                    "  ⚠ Temporal: {}/{} facts have tags ({:.0}%)",
                    fact_stats.facts_with_tags,
                    fact_stats.total_facts,
                    fact_stats.coverage * 100.0
                );
            }
        } else if opts.verbose && fact_stats.total_facts > 0 {
            println!(
                "    Temporal: {}/{} facts have tags ({:.0}%)",
                fact_stats.facts_with_tags,
                fact_stats.total_facts,
                fact_stats.coverage * 100.0
            );
        }

        let size_bytes = fs::metadata(&pre.path)
            .map(|m| m.len())
            .unwrap_or(content.len() as u64);

        pending.push(PendingDoc {
            id,
            content,
            relative,
            hash,
            title,
            doc_type,
            path: pre.path,
            size_bytes,
        });
    }

    // Finish Pass 1 progress bar
    pb.finish_and_clear();

    // Generate embeddings in batches and save documents
    let (docs_embedded, embedding_ms, db_write_ms, file_timings) =
        if !opts.dry_run && !pending.is_empty() {
            let emb_output = run_embedding_phase(EmbeddingPhaseInput {
                pending: pending.clone(),
                repo_id: &repo.id,
                embedding,
                db,
                chunk_size: opts.chunk_size,
                chunk_overlap: opts.chunk_overlap,
                embedding_batch_size: opts.embedding_batch_size,
                show_progress: opts.show_progress,
                verbose: opts.verbose,
                collect_stats: opts.collect_stats,
            })
            .await?;

            if emb_output.interrupted {
                return Ok(build_interrupted_result(InterruptedResultParams {
                    added: result.added,
                    updated: result.updated,
                    deleted: result.deleted,
                    unchanged: result.unchanged,
                    moved: result.moved,
                    reindexed: result.reindexed,
                    links_detected: 0,
                    total_facts,
                    facts_with_tags,
                    below_threshold_docs,
                    file_discovery_ms,
                    parsing_ms,
                    embedding_ms: emb_output.embedding_ms,
                    db_write_ms: emb_output.db_write_ms,
                    link_detection_ms: 0,
                    total_ms: scan_start.elapsed().as_millis() as u64,
                    docs_embedded: emb_output.docs_embedded,
                    docs_link_detected: 0,
                }));
            }

            (
                emb_output.docs_embedded,
                emb_output.embedding_ms,
                emb_output.db_write_ms,
                emb_output.file_timings,
            )
        } else {
            (0, 0, 0, Vec::new())
        };

    // Mark deleted documents
    for (id, doc) in &known {
        if !seen.contains(id) && !doc.is_deleted {
            if opts.verbose || opts.dry_run {
                println!("  DELETE {}", doc.file_path);
            }
            if !opts.dry_run {
                db.mark_deleted(id)?;
            }
            result.deleted += 1;
        }
    }

    if !opts.dry_run {
        db.commit_transaction()?;

        // Invalidate cross-check hashes for documents that link TO changed documents.
        // When a document's content changes, any document referencing it may now have
        // stale cross-validation results and needs re-checking.
        if !changed_ids.is_empty() {
            let mut to_invalidate: HashSet<String> = HashSet::new();
            for id in &changed_ids {
                if let Ok(links) = db.get_links_to(id) {
                    for link in links {
                        if !changed_ids.contains(&link.source_id) {
                            to_invalidate.insert(link.source_id);
                        }
                    }
                }
            }
            if !to_invalidate.is_empty() {
                let ids: Vec<&str> = to_invalidate.iter().map(|s| s.as_str()).collect();
                db.clear_cross_check_hashes(&ids)?;
                info!(
                    "Invalidated cross-check hashes for {} linked documents",
                    ids.len()
                );
            }
        }
    }

    if opts.dry_run {
        return Ok(result);
    }

    // Check for duplicates if requested
    if opts.check_duplicates && !changed_ids.is_empty() {
        result.duplicates = check_duplicates(db, &changed_ids)?;
    }

    // Pass 2: Detect links using LLM (skip if --no-links)
    let link_output = run_link_detection_phase(LinkPhaseInput {
        db,
        link_detector,
        repo_id: &repo.id,
        changed_ids: &changed_ids,
        added_count: result.added,
        show_progress: opts.show_progress,
        verbose: opts.verbose,
        skip_links: opts.skip_links,
    })
    .await?;

    if link_output.interrupted {
        return Ok(build_interrupted_result(InterruptedResultParams {
            added: result.added,
            updated: result.updated,
            deleted: result.deleted,
            unchanged: result.unchanged,
            moved: result.moved,
            reindexed: result.reindexed,
            links_detected: link_output.links_detected,
            total_facts,
            facts_with_tags,
            below_threshold_docs,
            file_discovery_ms,
            parsing_ms,
            embedding_ms,
            db_write_ms,
            link_detection_ms: link_output.link_detection_ms,
            total_ms: scan_start.elapsed().as_millis() as u64,
            docs_embedded,
            docs_link_detected: link_output.docs_link_detected,
        }));
    }

    result.links_detected = link_output.links_detected;
    let link_detection_ms = link_output.link_detection_ms;
    let docs_link_detected = link_output.docs_link_detected;

    // Set total document count
    // moved = files that changed path only (no content change)
    // updated = files with content changes (may also have moved)
    // reindexed = files with unchanged content but regenerated embeddings
    result.total =
        result.added + result.updated + result.unchanged + result.moved + result.reindexed;

    // Collect stats if requested
    if opts.collect_stats {
        // Build slowest_files list (top 10 by embedding time)
        let mut slowest: Vec<FileTimingInfo> = file_timings
            .iter()
            .map(|(doc_idx, _chunks, emb_ms, total_ms)| {
                let doc = &pending[*doc_idx];
                FileTimingInfo {
                    file_path: doc.relative.clone(),
                    title: doc.title.clone(),
                    size_bytes: doc.size_bytes,
                    embedding_ms: *emb_ms,
                    total_ms: *total_ms,
                }
            })
            .collect();
        slowest.sort_by(|a, b| b.embedding_ms.cmp(&a.embedding_ms));
        slowest.truncate(10);

        result.stats = Some(ScanStats {
            file_discovery_ms,
            parsing_ms,
            embedding_ms,
            db_write_ms,
            link_detection_ms,
            total_ms: scan_start.elapsed().as_millis() as u64,
            docs_embedded,
            docs_link_detected,
            slowest_files: slowest,
        });
    }

    // Always collect temporal stats (lightweight, no extra I/O)
    let overall_coverage = if total_facts > 0 {
        facts_with_tags as f32 / total_facts as f32
    } else {
        1.0 // No facts = 100% coverage (nothing to tag)
    };
    result.temporal_stats = Some(TemporalScanStats {
        total_facts,
        facts_with_tags,
        coverage: overall_coverage,
        below_threshold_docs,
    });

    Ok(result)
}

/// Scan all repositories
pub async fn scan_all_repositories(
    db: &Database,
    scanner: &super::Scanner,
    processor: &DocumentProcessor,
    embedding: &dyn EmbeddingProvider,
    link_detector: &LinkDetector,
    opts: &ScanOptions,
) -> anyhow::Result<ScanResult> {
    let repos = db.list_repositories()?;
    let mut total = ScanResult::default();

    for repo in repos {
        if opts.verbose || opts.dry_run {
            println!("Scanning repo: {} ({})", repo.name, repo.path.display());
        }
        match full_scan(
            &repo,
            db,
            scanner,
            processor,
            embedding,
            link_detector,
            opts,
        )
        .await
        {
            Ok(result) => {
                total.added += result.added;
                total.updated += result.updated;
                total.unchanged += result.unchanged;
                total.deleted += result.deleted;
                total.links_detected += result.links_detected;
            }
            Err(e) => {
                warn!("Failed to scan repo {}: {}", repo.id, e);
            }
        }
    }

    Ok(total)
}
