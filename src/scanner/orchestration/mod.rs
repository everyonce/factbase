//! Scan orchestration - full_scan and scan_all_repositories

mod embedding;
pub mod facts;
pub mod links;
mod results;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, warn};

use chrono::{DateTime, Utc};
use rayon::prelude::*;

use crate::models::TemporalScanStats;
use crate::processor::content_hash;
use crate::ProgressReporter;
use crate::{
    calculate_fact_stats, count_facts_with_sources, Database, Document, DocumentProcessor,
    EmbeddingProvider, LinkDetector, Repository, ScanResult, ScanStats,
};
use crate::models::{normalize_pair, DuplicatePair};

use super::options::ScanOptions;
use super::progress::OptionalProgress;
use embedding::{run_embedding_phase, EmbeddingPhaseInput};
// facts module used by scan pass 3 (fact embedding generation)
use links::{run_link_detection_phase, LinkPhaseInput};
use results::{build_interrupted_result, InterruptedResultParams};

// --- Inlined from types.rs ---

/// Document pending embedding generation
#[derive(Clone)]
pub(super) struct PendingDoc {
    pub id: String,
    pub content: String,
    pub relative: String,
    pub hash: String,
    pub title: String,
    pub doc_type: String,
    pub path: PathBuf,
}

/// Pre-read file data from parallel I/O phase
pub(super) struct PreReadFile {
    pub path: PathBuf,
    pub content: Result<String, String>,
    pub hash: Option<String>,
    pub existing_id: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
}

/// Chunk information for embedding generation
pub(super) struct ChunkInfo {
    pub doc_idx: usize,
    pub chunk_idx: usize,
    pub chunk_start: usize,
    pub chunk_end: usize,
    pub content: String,
}

// --- Inlined from preread.rs ---

/// Pre-read files in parallel (I/O bound)
pub(super) fn pre_read_files(files: Vec<PathBuf>) -> Vec<PreReadFile> {
    files
        .into_par_iter()
        .map(|path| {
            let content = fs::read_to_string(&path).map_err(|e| e.to_string());
            let (hash, existing_id) = if let Ok(ref c) = content {
                let h = content_hash(c);
                let id = DocumentProcessor::extract_id_static(c);
                (Some(h), id)
            } else {
                (None, None)
            };
            let modified_at = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::<Utc>::from);
            PreReadFile {
                path,
                content,
                hash,
                existing_id,
                modified_at,
            }
        })
        .collect()
}

// --- Inlined from duplicates.rs ---

/// Threshold for considering documents as duplicates (95% similarity)
const DUPLICATE_THRESHOLD: f32 = 0.95;

/// Check for duplicate documents among changed documents
fn check_duplicates(
    db: &Database,
    changed_ids: &HashSet<String>,
) -> anyhow::Result<Vec<DuplicatePair>> {
    let mut duplicates = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for doc_id in changed_ids {
        if let Ok(similar) = db.find_similar_documents(doc_id, DUPLICATE_THRESHOLD) {
            for (similar_id, similar_title, similarity) in similar {
                let pair = normalize_pair(doc_id, &similar_id);
                if seen_pairs.insert(pair) {
                    let doc_title = db
                        .get_document(doc_id)?
                        .map_or_else(|| doc_id.clone(), |d| d.title);
                    duplicates.push(DuplicatePair {
                        doc1_id: doc_id.clone(),
                        doc1_title: doc_title,
                        doc2_id: similar_id,
                        doc2_title: similar_title,
                        similarity,
                    });
                }
            }
        }
    }

    Ok(duplicates)
}

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

    let mut files = ctx.scanner.find_markdown_files(&repo.path);
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

    // Apply file_offset for resume — skip already-processed files
    let (files, total_files) = if ctx.opts.file_offset > 0 && ctx.opts.file_offset < files.len() {
        let remaining = files.split_off(ctx.opts.file_offset);
        let total = ctx.opts.file_offset + remaining.len();
        (remaining, total)
    } else if ctx.opts.file_offset > 0 && ctx.opts.file_offset >= files.len() {
        // All files already processed in a previous call — skip file loop entirely
        (Vec::new(), files.len())
    } else {
        let total = files.len();
        (files, total)
    };

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
        // Check deadline before starting a new chunk
        if let Some(deadline) = ctx.opts.deadline {
            if Instant::now() > deadline {
                result.total = result.added + result.updated + result.unchanged + result.moved + result.reindexed;
                result.file_offset = ctx.opts.file_offset + global_idx;
                result.interrupted = true;
                return Ok(result);
            }
        }

        let pre_read: Vec<PreReadFile> = pre_read_files(file_chunk.to_vec());

        // Pass 1: Process pre-read files sequentially (needs DB access)
        for pre in pre_read.into_iter() {
            global_idx += 1;
            pb.set_position(global_idx as u64);
            if !ctx.opts.show_progress {
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

            // Strip answered [x] questions before computing hash
            let (content, hash) = if !ctx.opts.dry_run {
                let (pruned, count) = crate::processor::strip_answered_questions(&content);
                if count > 0 {
                    fs::write(&pre.path, &pruned)?;
                    result.questions_pruned += count;
                    let new_hash = crate::processor::content_hash(&pruned);
                    (pruned, new_hash)
                } else {
                    let hash = pre.hash.expect("hash should exist when content is Ok");
                    (content, hash)
                }
            } else {
                let hash = pre.hash.expect("hash should exist when content is Ok");
                (content, hash)
            };

            let id = if let Some(id) = pre.existing_id {
                id
            } else if ctx.opts.dry_run {
                ctx.processor.generate_unique_id(db)
            } else {
                let id = ctx.processor.generate_unique_id(db);
                let resolved_format = repo
                    .perspective
                    .as_ref()
                    .and_then(|p| p.format.as_ref())
                    .map(|f| f.resolve())
                    .unwrap_or_default();
                let new_content =
                    ctx.processor
                        .inject_id_with_format(&content, &id, &resolved_format);
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

            // Preserve review queue from DB when disk file is stale
            let content = if !is_new {
                if let Some(db_doc) = known.get(&id) {
                    crate::patterns::merge_review_queue(&content, &db_doc.content)
                        .unwrap_or(content)
                } else {
                    content
                }
            } else {
                content
            };

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
            if ctx.opts.skip_embeddings {
                // Index-only mode: upsert documents to DB without generating embeddings
                let db_start = Instant::now();
                for doc in pending.drain(..) {
                    let document = Document {
                        id: doc.id,
                        repo_id: repo.id.clone(),
                        file_path: doc.relative,
                        file_hash: doc.hash,
                        title: doc.title,
                        doc_type: Some(doc.doc_type),
                        content: doc.content,
                        file_modified_at: fs::metadata(&doc.path)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(chrono::DateTime::from),
                        indexed_at: Utc::now(),
                        is_deleted: false,
                    };
                    db.upsert_document(&document)?;
                }
                total_db_write_ms += db_start.elapsed().as_millis() as u64;
            } else {
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
                progress: ctx.progress,
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
                    fact_embeddings_generated: 0,
                    file_offset: ctx.opts.file_offset + global_idx,
                }));
            }
            } // end else (embedding enabled)
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
                db.delete_fact_embeddings_for_doc(id)?;
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
        force_relink: ctx.opts.force_relink,
        link_batch_size: ctx.opts.link_batch_size,
        progress: ctx.progress,
        deadline: None,
        doc_offset: 0,
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
            fact_embeddings_generated: 0,
            file_offset: total_files,
        }));
    }

    result.links_detected = link_output.links_detected;
    let link_detection_ms = link_output.link_detection_ms;
    let docs_link_detected = link_output.docs_link_detected;

    // Pass 3: Generate fact embeddings for changed documents
    if !ctx.opts.skip_embeddings {
    let fact_ids = if !changed_ids.is_empty() {
        changed_ids.clone()
    } else {
        let total_docs = result.added + result.updated + result.unchanged + result.moved + result.reindexed;
        if total_docs > 0 && db.get_fact_embedding_count()? == 0 {
            seen.clone()
        } else {
            HashSet::new()
        }
    };
    if !fact_ids.is_empty() {
        let _ = db.invalidate_fact_pair_cache();
        ctx.progress.phase("Generating fact embeddings");
        let fact_output = facts::run_fact_embedding_phase(&facts::FactEmbeddingInput {
            changed_ids: &fact_ids,
            embedding: ctx.embedding,
            db,
            embedding_batch_size: ctx.opts.embedding_batch_size,
            progress: ctx.progress,
            deadline: ctx.opts.deadline,
        }).await?;
        result.fact_embeddings_generated = fact_output.generated;
    }
    result.fact_embeddings_needed = 0;
    } // end skip_embeddings check

    // Set total document count
    // moved = files that changed path only (no content change)
    // updated = files with content changes (may also have moved)
    // reindexed = files with unchanged content but regenerated embeddings
    result.total =
        result.added + result.updated + result.unchanged + result.moved + result.reindexed;
    result.embeddings_skipped = ctx.opts.skip_embeddings;

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
                total.fact_embeddings_generated += result.fact_embeddings_generated;
                total.fact_embeddings_needed += result.fact_embeddings_needed;
            }
            Err(e) => {
                warn!("Failed to scan repo {}: {}", repo.id, e);
            }
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::models::Repository;
    use crate::scanner::options::ScanOptions;
    use crate::ProgressReporter;
    use std::collections::HashSet;
    use tempfile::TempDir;

    /// Create a repo dir with perspective.yaml and register it in the DB.
    fn setup_repo(db: &Database) -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "name: test\ndescription: test repo\n",
        )
        .unwrap();
        let repo = Repository {
            id: "test".into(),
            name: "test".into(),
            path: tmp.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        (tmp, repo)
    }

    fn scan_ctx<'a>(
        scanner: &'a super::super::Scanner,
        processor: &'a DocumentProcessor,
        embedding: &'a dyn EmbeddingProvider,
        link_detector: &'a LinkDetector,
        opts: &'a ScanOptions,
        progress: &'a ProgressReporter,
    ) -> ScanContext<'a> {
        ScanContext {
            scanner,
            processor,
            embedding,
            link_detector,
            opts,
            progress,
        }
    }

    // ── pre_read_files ──

    #[test]
    fn test_pre_read_files_reads_content_and_hash() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("doc.md");
        std::fs::write(&p, "<!-- factbase:abc123 -->\n# Hello").unwrap();

        let results = pre_read_files(vec![p]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.is_ok());
        assert!(results[0].hash.is_some());
        assert_eq!(results[0].existing_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_pre_read_files_no_id() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("doc.md");
        std::fs::write(&p, "# No ID").unwrap();

        let results = pre_read_files(vec![p]);
        assert_eq!(results[0].existing_id, None);
        assert!(results[0].hash.is_some());
    }

    #[test]
    fn test_pre_read_files_nonexistent() {
        let results = pre_read_files(vec![PathBuf::from("/nonexistent/file.md")]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.is_err());
        assert!(results[0].hash.is_none());
        assert!(results[0].existing_id.is_none());
    }

    #[test]
    fn test_pre_read_files_multiple() {
        let tmp = TempDir::new().unwrap();
        let p1 = tmp.path().join("a.md");
        let p2 = tmp.path().join("b.md");
        std::fs::write(&p1, "# A").unwrap();
        std::fs::write(&p2, "# B").unwrap();

        let results = pre_read_files(vec![p1, p2]);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.content.is_ok()));
    }

    #[test]
    fn test_pre_read_files_modified_at() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("doc.md");
        std::fs::write(&p, "# Doc").unwrap();

        let results = pre_read_files(vec![p]);
        assert!(results[0].modified_at.is_some());
    }

    // ── check_duplicates ──

    #[test]
    fn test_check_duplicates_empty_set() {
        let (db, _tmp) = test_db();
        let changed = HashSet::new();
        let result = check_duplicates(&db, &changed).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_check_duplicates_no_similar_docs() {
        let (db, _tmp) = test_db();
        let (_, repo) = setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let dim = embedding.dimension();

        // Insert two very different docs
        let doc1 = Document {
            id: "aaa111".into(), repo_id: repo.id.clone(),
            file_path: "a.md".into(), file_hash: "h1".into(),
            title: "Alpha".into(), doc_type: Some("doc".into()),
            content: "# Alpha\n\nCompletely different content about cats.".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        let doc2 = Document {
            id: "bbb222".into(), repo_id: repo.id.clone(),
            file_path: "b.md".into(), file_hash: "h2".into(),
            title: "Beta".into(), doc_type: Some("doc".into()),
            content: "# Beta\n\nEntirely unrelated content about dogs.".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();
        // Different embeddings → low similarity
        db.upsert_embedding_chunk("aaa111", 0, 0, 100, &vec![1.0; dim]).unwrap();
        db.upsert_embedding_chunk("bbb222", 0, 0, 100, &vec![-1.0; dim]).unwrap();

        let mut changed = HashSet::new();
        changed.insert("aaa111".to_string());
        let result = check_duplicates(&db, &changed).unwrap();
        assert!(result.is_empty(), "Different docs should not be duplicates");
    }

    #[test]
    fn test_check_duplicates_finds_similar() {
        let (db, _tmp) = test_db();
        let (_, repo) = setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let dim = embedding.dimension();

        let doc1 = Document {
            id: "aaa111".into(), repo_id: repo.id.clone(),
            file_path: "a.md".into(), file_hash: "h1".into(),
            title: "Alpha".into(), doc_type: Some("doc".into()),
            content: "# Alpha\n\nSame content.".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        let doc2 = Document {
            id: "bbb222".into(), repo_id: repo.id.clone(),
            file_path: "b.md".into(), file_hash: "h2".into(),
            title: "Beta".into(), doc_type: Some("doc".into()),
            content: "# Beta\n\nSame content.".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();
        // Identical embeddings → 100% similarity
        let emb = vec![0.5; dim];
        db.upsert_embedding_chunk("aaa111", 0, 0, 100, &emb).unwrap();
        db.upsert_embedding_chunk("bbb222", 0, 0, 100, &emb).unwrap();

        let mut changed = HashSet::new();
        changed.insert("aaa111".to_string());
        let result = check_duplicates(&db, &changed).unwrap();
        assert!(!result.is_empty(), "Identical embeddings should be detected as duplicates");
        assert_eq!(result[0].doc1_id, "aaa111");
        assert_eq!(result[0].doc2_id, "bbb222");
    }

    #[test]
    fn test_check_duplicates_deduplicates_pairs() {
        let (db, _tmp) = test_db();
        let (_, repo) = setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let dim = embedding.dimension();

        let doc1 = Document {
            id: "aaa111".into(), repo_id: repo.id.clone(),
            file_path: "a.md".into(), file_hash: "h1".into(),
            title: "Alpha".into(), doc_type: Some("doc".into()),
            content: "# Alpha".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        let doc2 = Document {
            id: "bbb222".into(), repo_id: repo.id.clone(),
            file_path: "b.md".into(), file_hash: "h2".into(),
            title: "Beta".into(), doc_type: Some("doc".into()),
            content: "# Beta".into(),
            file_modified_at: None, indexed_at: chrono::Utc::now(), is_deleted: false,
        };
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();
        let emb = vec![0.5; dim];
        db.upsert_embedding_chunk("aaa111", 0, 0, 100, &emb).unwrap();
        db.upsert_embedding_chunk("bbb222", 0, 0, 100, &emb).unwrap();

        // Both docs in changed set — should still only produce one pair
        let mut changed = HashSet::new();
        changed.insert("aaa111".to_string());
        changed.insert("bbb222".to_string());
        let result = check_duplicates(&db, &changed).unwrap();
        assert_eq!(result.len(), 1, "Should deduplicate (A,B) and (B,A) into one pair");
    }

    // ── pre_read_files edge cases ──

    #[test]
    fn test_pre_read_files_empty_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("empty.md");
        std::fs::write(&path, "").unwrap();
        let results = pre_read_files(vec![path]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_pre_read_files_with_existing_id() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("doc.md");
        std::fs::write(&path, "<!-- factbase:abc123 -->\n# Title\n\nContent.").unwrap();
        let results = pre_read_files(vec![path]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].existing_id.as_deref(), Some("abc123"));
    }

    // ── full_scan: new file detection ──

    #[tokio::test]
    async fn test_full_scan_detects_new_files() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# New Doc\n\nSome content.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.added, 1);
        assert_eq!(result.total, 1);
        assert_eq!(result.deleted, 0);
        assert_eq!(result.unchanged, 0);
    }

    #[tokio::test]
    async fn test_full_scan_unchanged_on_rescan() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        // First scan — adds file and injects ID header into the file
        let r1 = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r1.added, 1);

        // Second scan — file was modified by ID injection, so it's "updated"
        let r2 = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.updated, 1);

        // Third scan — now truly unchanged
        let r3 = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r3.added, 0);
        assert_eq!(r3.unchanged, 1);
        assert_eq!(r3.updated, 0);
    }

    #[tokio::test]
    async fn test_full_scan_detects_modified_file() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        let path = tmp.path().join("doc.md");
        std::fs::write(&path, "# Doc\n\nOriginal.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        full_scan(&repo, &db, &ctx).await.unwrap();

        // Read back the file (now has injected ID) and modify content
        let content = std::fs::read_to_string(&path).unwrap();
        let modified = content.replace("Original.", "Modified content.");
        std::fs::write(&path, modified).unwrap();

        let r2 = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.updated, 1);
        assert_eq!(r2.added, 0);
    }

    #[tokio::test]
    async fn test_full_scan_detects_deleted_file() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        let path = tmp.path().join("doc.md");
        std::fs::write(&path, "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        full_scan(&repo, &db, &ctx).await.unwrap();
        std::fs::remove_file(&path).unwrap();

        let r2 = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(r2.deleted, 1);
        assert_eq!(r2.added, 0);
    }

    #[tokio::test]
    async fn test_full_scan_skips_dot_directories() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("visible.md"), "# Visible").unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/hidden.md"), "# Hidden").unwrap();
        std::fs::create_dir(tmp.path().join(".kiro")).unwrap();
        std::fs::write(tmp.path().join(".kiro/task.md"), "# Task").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.added, 1, "Only visible.md should be indexed");
        assert_eq!(result.total, 1);
    }

    #[tokio::test]
    async fn test_full_scan_resume_with_file_offset() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        for i in 0..5 {
            std::fs::write(
                tmp.path().join(format!("doc{i}.md")),
                format!("# Doc {i}\n\nContent {i}."),
            )
            .unwrap();
        }

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            file_offset: 3,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        // With offset 3, only 2 of 5 files should be processed
        assert_eq!(result.added, 2);
    }

    #[tokio::test]
    async fn test_full_scan_dry_run_no_db_writes() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            dry_run: true,
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.added, 1);

        // DB should have no documents
        let docs = db.get_documents_for_repo("test").unwrap();
        assert!(docs.is_empty(), "dry_run should not write to DB");
    }

    #[tokio::test]
    async fn test_full_scan_malformed_file_skipped() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        // Write a valid file
        std::fs::write(tmp.path().join("good.md"), "# Good\n\nContent.").unwrap();
        // Write a binary/unreadable file with .md extension — create a dir with .md name
        // to simulate an unreadable file, use a symlink to nowhere
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/nonexistent/path", tmp.path().join("bad.md")).unwrap();
        }

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        // Good file should be indexed, bad file skipped without crash
        assert_eq!(result.added, 1);
    }

    #[tokio::test]
    async fn test_full_scan_multiple_files() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::create_dir(tmp.path().join("topics")).unwrap();
        std::fs::write(tmp.path().join("topics/alpha.md"), "# Alpha\n\nAlpha content.").unwrap();
        std::fs::write(tmp.path().join("topics/beta.md"), "# Beta\n\nBeta content.").unwrap();
        std::fs::write(tmp.path().join("root.md"), "# Root\n\nRoot content.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.added, 3);
        assert_eq!(result.total, 3);
    }

    #[tokio::test]
    async fn test_full_scan_skip_embeddings_mode() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            skip_embeddings: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.added, 1);
        assert!(result.embeddings_skipped);

        // Document should still be in DB
        let docs = db.get_documents_for_repo("test").unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn test_full_scan_force_reindex() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        // First scan
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);
        full_scan(&repo, &db, &ctx).await.unwrap();

        // Second scan to stabilize (ID injection changed the file)
        full_scan(&repo, &db, &ctx).await.unwrap();

        // Third scan with force_reindex — content is now stable
        let opts2 = ScanOptions {
            skip_links: true,
            force_reindex: true,
            ..Default::default()
        };
        let ctx2 = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts2, &progress);
        let r3 = full_scan(&repo, &db, &ctx2).await.unwrap();
        assert_eq!(r3.reindexed, 1);
        assert_eq!(r3.added, 0);
        assert_eq!(r3.unchanged, 0);
    }

    #[tokio::test]
    async fn test_full_scan_deadline_interrupts() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        for i in 0..10 {
            std::fs::write(
                tmp.path().join(format!("doc{i}.md")),
                format!("# Doc {i}\n\nContent."),
            )
            .unwrap();
        }

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        // Deadline already passed
        let opts = ScanOptions {
            skip_links: true,
            deadline: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert!(result.interrupted);
    }

    #[tokio::test]
    async fn test_full_scan_temporal_stats_collected() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(
            tmp.path().join("doc.md"),
            "# Doc\n\n- Fact one @t[2024-01]\n- Fact two\n",
        )
        .unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        let ts = result.temporal_stats.unwrap();
        assert_eq!(ts.total_facts, 2);
        assert_eq!(ts.facts_with_tags, 1);
        assert!(ts.coverage > 0.0 && ts.coverage < 1.0);
    }

    #[tokio::test]
    async fn test_full_scan_empty_repo() {
        let (db, _db_tmp) = test_db();
        let (_, repo) = setup_repo(&db);

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        assert_eq!(result.total, 0);
        assert_eq!(result.added, 0);
    }

    #[tokio::test]
    async fn test_full_scan_file_offset_beyond_total() {
        let (db, _db_tmp) = test_db();
        let (tmp, repo) = setup_repo(&db);
        std::fs::write(tmp.path().join("doc.md"), "# Doc\n\nContent.").unwrap();

        let scanner = super::super::Scanner::new(&[]);
        let processor = DocumentProcessor::new();
        let embedding = MockEmbedding::new(1024);
        let link_detector = LinkDetector::new();
        let opts = ScanOptions {
            skip_links: true,
            file_offset: 100,
            ..Default::default()
        };
        let progress = ProgressReporter::Silent;
        let ctx = scan_ctx(&scanner, &processor, &embedding, &link_detector, &opts, &progress);

        let result = full_scan(&repo, &db, &ctx).await.unwrap();
        // All files skipped
        assert_eq!(result.added, 0);
    }
}
