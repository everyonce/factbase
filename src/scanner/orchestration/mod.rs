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

use crate::models::TemporalScanStats;
use crate::ProgressReporter;
use crate::{
    calculate_fact_stats, count_facts_with_sources, Database, DocumentProcessor,
    EmbeddingProvider, LinkDetector, Repository, ScanResult, ScanStats,
};

use super::options::ScanOptions;
use super::progress::OptionalProgress;
use duplicates::check_duplicates;
use embedding::{run_embedding_phase, EmbeddingPhaseInput};
use links::{run_link_detection_phase, LinkPhaseInput};
use preread::pre_read_files;
use results::{build_interrupted_result, InterruptedResultParams};
use types::{PendingDoc, PreReadFile};

/// Bundles the "tools" needed for a scan, reducing parameter count on `full_scan`.
pub struct ScanContext<'a> {
    pub scanner: &'a super::Scanner,
    pub processor: &'a DocumentProcessor,
    pub embedding: &'a dyn EmbeddingProvider,
    pub link_detector: &'a LinkDetector,
    pub opts: &'a ScanOptions,
    pub progress: &'a ProgressReporter,
}

/// Perform a full scan of a repository
#[tracing::instrument(
    name = "full_scan",
    skip(db, ctx),
    fields(repo_id = %repo.id, repo_path = %repo.path.display())
)]
pub async fn full_scan(
    repo: &Repository,
    db: &Database,
    ctx: &ScanContext<'_>,
) -> anyhow::Result<ScanResult> {
    // Reload perspective.yaml from disk and update DB if changed
    let repo = {
        let mut r = repo.clone();
        let disk_perspective = crate::models::load_perspective_from_file(&r.path);
        if disk_perspective != r.perspective {
            r.perspective = disk_perspective;
            db.upsert_repository(&r)?;
        }
        r
    };
    let repo = &repo;

    let scan_start = Instant::now();
    let file_discovery_start = Instant::now();

    let files = ctx.scanner.find_markdown_files(&repo.path);
    let known = db.get_documents_for_repo(&repo.id)?;
    let mut seen = HashSet::new();
    let mut changed_ids = HashSet::new();
    let mut result = ScanResult::default();
    let total_files = files.len();

    // Temporal stats tracking
    let mut total_facts = 0usize;
    let mut facts_with_tags = 0usize;
    let mut facts_with_sources = 0usize;
    let mut below_threshold_docs = 0usize;

    let file_discovery_ms = file_discovery_start.elapsed().as_millis() as u64;

    info!("Scanning {} files in {}", total_files, repo.path.display());
    ctx.progress.phase("Indexing documents");

    // Create progress bar if enabled and enough files
    let pb = if ctx.opts.show_progress && !ctx.opts.verbose && !ctx.opts.dry_run {
        OptionalProgress::new(
            total_files as u64,
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} (ETA: {eta})",
            "scanning",
            10,
        )
    } else {
        OptionalProgress::none()
    };

    let mut pending: Vec<PendingDoc> = Vec::new();
    let mut all_file_timings: Vec<(usize, usize, u64, u64)> = Vec::new();
    let mut total_docs_embedded = 0usize;
    let mut total_embedding_ms = 0u64;
    let mut total_db_write_ms = 0u64;
    let mut global_idx = 0usize;

    // Process files in chunks to bound memory usage
    const SCAN_CHUNK_SIZE: usize = 100;
    let parsing_start = Instant::now();

    for file_chunk in files.chunks(SCAN_CHUNK_SIZE) {
        let pre_read: Vec<PreReadFile> = pre_read_files(file_chunk.to_vec());

        // Pass 1: Process pre-read files sequentially (needs DB access)
        for pre in pre_read.into_iter() {
            global_idx += 1;
            pb.set_position(global_idx as u64);
            if total_files >= 50 && global_idx.is_multiple_of(25) && !ctx.opts.show_progress {
                ctx.progress
                    .report(global_idx, total_files, "files processed");
            }

            // Skip files older than --since filter
            if let Some(since) = ctx.opts.since {
                if let Some(modified_at) = pre.modified_at {
                    if modified_at < since {
                        if ctx.opts.verbose || ctx.opts.dry_run {
                            println!("  SKIP {} (older than --since)", pre.path.display());
                        }
                        continue;
                    }
                }
            }

            let content = match pre.content {
                Ok(c) => c,
                Err(e) => {
                    if ctx.opts.verbose || ctx.opts.dry_run {
                        println!("  SKIP {}: {}", pre.path.display(), e);
                    }
                    warn!("Skip {}: {}", pre.path.display(), e);
                    continue;
                }
            };

            let hash = pre.hash.expect("hash should exist when content is Ok");
            let id = if let Some(id) = pre.existing_id {
                id
            } else if ctx.opts.dry_run {
                ctx.processor.generate_unique_id(db)
            } else {
                let id = ctx.processor.generate_unique_id(db);
                let new_content = ctx.processor.inject_header(&content, &id);
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
            let is_modified = known.get(&id).is_some_and(|d| d.file_hash != hash);
            let is_moved = known.get(&id).is_some_and(|d| d.file_path != relative);

            // Skip unchanged documents unless force_reindex is set
            if !is_new && !is_modified && !is_moved && !ctx.opts.force_reindex {
                if ctx.opts.verbose || ctx.opts.dry_run {
                    println!("  UNCHANGED {}", pre.path.display());
                }
                result.unchanged += 1;
                continue;
            }

            // When force_reindex is set, treat unchanged docs as needing reindex
            let is_reindex = ctx.opts.force_reindex && !is_new && !is_modified && !is_moved;

            changed_ids.insert(id.clone());

            let title = ctx.processor.extract_title(&content, &pre.path);

            if is_new {
                if ctx.opts.verbose || ctx.opts.dry_run {
                    println!("  NEW {relative} ({title})");
                }
                result.added += 1;
            } else if is_reindex {
                // Force reindex - content unchanged but embeddings regenerated
                if ctx.opts.verbose || ctx.opts.dry_run {
                    println!("  REINDEX {relative} ({title})");
                }
                result.reindexed += 1;
            } else if is_moved && !is_modified {
                // File moved but content unchanged - counts as moved only
                if ctx.opts.verbose || ctx.opts.dry_run {
                    let old_path = known.get(&id).map_or("?", |d| d.file_path.as_str());
                    println!("  MOVED {old_path} -> {relative} ({title})");
                }
                result.moved += 1;
            } else if is_moved && is_modified {
                // File moved AND content modified - counts as updated (with move note)
                if ctx.opts.verbose || ctx.opts.dry_run {
                    let old_path = known.get(&id).map_or("?", |d| d.file_path.as_str());
                    println!("  UPDATED+MOVED {old_path} -> {relative} ({title})");
                }
                result.updated += 1;
            } else {
                // Content modified, same path
                if ctx.opts.verbose || ctx.opts.dry_run {
                    println!("  UPDATED {relative} ({title})");
                }
                result.updated += 1;
            }

            if ctx.opts.dry_run {
                continue;
            }

            let doc_type = ctx.processor.derive_type(&pre.path, &repo.path);

            // Validate type against allowed_types if configured
            if let Some(ref perspective) = repo.perspective {
                if let Some(ref allowed) = perspective.allowed_types {
                    if !allowed.iter().any(|t| t.to_lowercase() == doc_type) {
                        warn!(
                            "Unknown type '{}' for {}: allowed types are {:?}",
                            doc_type, relative, allowed
                        );
                        if ctx.opts.verbose {
                            println!("  WARN: Unknown type '{doc_type}' (allowed: {allowed:?})");
                        }
                    }
                }
            }

            // Calculate temporal stats for this document
            let fact_stats = calculate_fact_stats(&content);
            total_facts += fact_stats.total_facts;
            facts_with_tags += fact_stats.facts_with_tags;
            facts_with_sources += count_facts_with_sources(&content);

            // Check if below threshold and warn
            let is_below_threshold =
                fact_stats.total_facts > 0 && fact_stats.coverage < ctx.opts.min_coverage;
            if is_below_threshold {
                below_threshold_docs += 1;
                if ctx.opts.verbose {
                    println!(
                        "  ⚠ Temporal: {}/{} facts have tags ({:.0}%)",
                        fact_stats.facts_with_tags,
                        fact_stats.total_facts,
                        fact_stats.coverage * 100.0
                    );
                }
            } else if ctx.opts.verbose && fact_stats.total_facts > 0 {
                println!(
                    "    Temporal: {}/{} facts have tags ({:.0}%)",
                    fact_stats.facts_with_tags,
                    fact_stats.total_facts,
                    fact_stats.coverage * 100.0
                );
            }

            pending.push(PendingDoc {
                id,
                content,
                relative,
                hash,
                title,
                doc_type,
                path: pre.path,
            });
        }

        // Embed this chunk's pending docs before moving to next chunk
        if !ctx.opts.dry_run && !pending.is_empty() {
            let chunk_output = run_embedding_phase(EmbeddingPhaseInput {
                pending: std::mem::take(&mut pending),
                repo_id: &repo.id,
                embedding: ctx.embedding,
                db,
                chunk_size: ctx.opts.chunk_size,
                chunk_overlap: ctx.opts.chunk_overlap,
                embedding_batch_size: ctx.opts.embedding_batch_size,
                show_progress: ctx.opts.show_progress,
                verbose: ctx.opts.verbose,
                collect_stats: ctx.opts.collect_stats,
            })
            .await?;

            total_docs_embedded += chunk_output.docs_embedded;
            total_embedding_ms += chunk_output.embedding_ms;
            total_db_write_ms += chunk_output.db_write_ms;
            all_file_timings.extend(chunk_output.file_timings);

            if chunk_output.interrupted {
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
                    facts_with_sources,
                    below_threshold_docs,
                    file_discovery_ms,
                    parsing_ms: parsing_start.elapsed().as_millis() as u64,
                    embedding_ms: total_embedding_ms,
                    db_write_ms: total_db_write_ms,
                    link_detection_ms: 0,
                    total_ms: scan_start.elapsed().as_millis() as u64,
                    docs_embedded: total_docs_embedded,
                    docs_link_detected: 0,
                }));
            }
        }
    } // end file_chunk loop

    let parsing_ms = parsing_start.elapsed().as_millis() as u64;

    // Finish Pass 1 progress bar
    pb.finish_and_clear();

    // Embedding was done per-chunk above; use accumulated results
    let (docs_embedded, embedding_ms, db_write_ms, _file_timings) = (
        total_docs_embedded,
        total_embedding_ms,
        total_db_write_ms,
        all_file_timings,
    );

    // Mark deleted documents
    for (id, doc) in &known {
        if !seen.contains(id) && !doc.is_deleted {
            if ctx.opts.verbose || ctx.opts.dry_run {
                println!("  DELETE {}", doc.file_path);
            }
            if !ctx.opts.dry_run {
                db.mark_deleted(id)?;
            }
            result.deleted += 1;
        }
    }

    if !ctx.opts.dry_run {
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
                let ids: Vec<&str> = to_invalidate.iter().map(String::as_str).collect();
                db.clear_cross_check_hashes(&ids)?;
                info!(
                    "Invalidated cross-check hashes for {} linked documents",
                    ids.len()
                );
            }
        }
    }

    if ctx.opts.dry_run {
        return Ok(result);
    }

    // Check for duplicates if requested
    if ctx.opts.check_duplicates && !changed_ids.is_empty() {
        result.duplicates = check_duplicates(db, &changed_ids)?;
    }

    // Pass 2: Detect links using LLM (skip if --no-links)
    ctx.progress.phase("Detecting links");
    let link_output = run_link_detection_phase(LinkPhaseInput {
        db,
        link_detector: ctx.link_detector,
        repo_id: &repo.id,
        changed_ids: &changed_ids,
        added_count: result.added,
        show_progress: ctx.opts.show_progress,
        verbose: ctx.opts.verbose,
        skip_links: ctx.opts.skip_links,
        link_batch_size: ctx.opts.link_batch_size,
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
            facts_with_sources,
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
    if ctx.opts.collect_stats {
        result.stats = Some(ScanStats {
            file_discovery_ms,
            parsing_ms,
            embedding_ms,
            db_write_ms,
            link_detection_ms,
            total_ms: scan_start.elapsed().as_millis() as u64,
            docs_embedded,
            docs_link_detected,
            slowest_files: Vec::new(),
        });
    }

    // Always collect temporal stats (lightweight, no extra I/O)
    let overall_coverage = if total_facts > 0 {
        facts_with_tags as f32 / total_facts as f32
    } else {
        1.0 // No facts = 100% coverage (nothing to tag)
    };
    let source_coverage = if total_facts > 0 {
        facts_with_sources as f32 / total_facts as f32
    } else {
        1.0
    };
    result.temporal_stats = Some(TemporalScanStats {
        total_facts,
        facts_with_tags,
        coverage: overall_coverage,
        below_threshold_docs,
        facts_with_sources,
        source_coverage,
    });

    Ok(result)
}

/// Scan all repositories
pub async fn scan_all_repositories(
    db: &Database,
    ctx: &ScanContext<'_>,
) -> anyhow::Result<ScanResult> {
    let repos = db.list_repositories()?;
    let mut total = ScanResult::default();

    for repo in repos {
        ctx.progress
            .phase(&format!("Scanning repository '{}'", repo.id));
        if ctx.opts.verbose || ctx.opts.dry_run {
            println!("Scanning repo: {} ({})", repo.name, repo.path.display());
        }
        match full_scan(&repo, db, ctx).await {
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
