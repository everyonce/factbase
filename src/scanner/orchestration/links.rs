//! Link detection phase for document scanning

use std::collections::HashSet;
use std::time::Instant;
use tracing::info;

use crate::progress::ProgressReporter;
use crate::{Database, LinkDetector};

use crate::scanner::progress::OptionalProgress;

/// Input parameters for the link detection phase
pub struct LinkPhaseInput<'a> {
    pub db: &'a Database,
    pub link_detector: &'a LinkDetector,
    pub repo_id: &'a str,
    pub changed_ids: &'a HashSet<String>,
    pub added_count: usize,
    pub show_progress: bool,
    pub verbose: bool,
    pub skip_links: bool,
    pub force_relink: bool,
    pub link_batch_size: usize,
    pub progress: &'a ProgressReporter,
    /// Optional deadline for time-boxed operation
    pub deadline: Option<std::time::Instant>,
    /// Number of documents to skip (for resume after interruption)
    pub doc_offset: usize,
}

/// Output from the link detection phase
pub struct LinkPhaseOutput {
    pub links_detected: usize,
    pub link_detection_ms: u64,
    pub docs_link_detected: usize,
    pub interrupted: bool,
    /// Number of documents processed (for resume)
    pub doc_offset: usize,
}

/// Run the link detection phase
pub async fn run_link_detection_phase(
    input: LinkPhaseInput<'_>,
) -> anyhow::Result<LinkPhaseOutput> {
    if input.skip_links {
        if input.verbose {
            println!("Link detection skipped (--no-links)");
        }
        return Ok(LinkPhaseOutput {
            links_detected: 0,
            link_detection_ms: 0,
            docs_link_detected: 0,
            interrupted: false,
            doc_offset: 0,
        });
    }

    let link_detection_start = Instant::now();
    let mut links_detected = 0usize;

    let known_entities = input.db.get_all_document_titles(Some(input.repo_id))?;
    let all_docs = input.db.get_documents_for_repo(input.repo_id)?;

    // Force full link detection if --relink or if no links exist yet (migrated/copied KB)
    let force_all = input.force_relink
        || (!all_docs.is_empty() && !input.db.has_links_for_repo(input.repo_id)?);

    let new_titles: Vec<&str> = input
        .changed_ids
        .iter()
        .filter_map(|id| all_docs.get(id).map(|d| d.title.as_str()))
        .collect();

    let new_keywords: HashSet<String> = new_titles
        .iter()
        .flat_map(|t| t.split_whitespace())
        .filter(|w| w.len() >= 3)
        .map(str::to_lowercase)
        .collect();

    let full_rescan = input.added_count > 10;
    let mut rescan_count = 0;

    if force_all && input.verbose {
        println!(
            "Full link detection: {}",
            if input.force_relink {
                "--relink requested"
            } else {
                "no existing links found"
            }
        );
    }

    // Count docs needing link detection for progress bar
    let docs_to_scan: Vec<_> = all_docs
        .iter()
        .filter(|(id, doc)| {
            if doc.is_deleted {
                return false;
            }
            if force_all {
                return true;
            }
            if input.changed_ids.contains(*id) {
                return true;
            }
            if input.added_count > 0 {
                let should_rescan = full_rescan
                    || new_keywords
                        .iter()
                        .any(|kw| doc.content.to_lowercase().contains(kw));
                return should_rescan;
            }
            false
        })
        .collect();

    let link_batch_size = input.link_batch_size;
    let total_batches = docs_to_scan.len().div_ceil(link_batch_size);

    let link_pb = if input.show_progress {
        OptionalProgress::new(
            total_batches as u64,
            "[{elapsed_precise}] {bar:40.yellow/blue} {pos}/{len} {msg} (ETA: {eta})",
            "detecting links (batched)",
            2,
        )
    } else {
        OptionalProgress::none()
    };

    // Apply doc_offset for resume — skip already-processed docs
    let docs_to_scan = if input.doc_offset > 0 && input.doc_offset < docs_to_scan.len() {
        docs_to_scan[input.doc_offset..].to_vec()
    } else {
        docs_to_scan
    };
    let mut total_docs_processed = input.doc_offset;

    // Process in batches
    for (batch_idx, chunk) in docs_to_scan.chunks(link_batch_size).enumerate() {
        // Check deadline before starting a new batch
        if let Some(deadline) = input.deadline {
            if std::time::Instant::now() > deadline {
                link_pb.finish_and_clear();
                return Ok(LinkPhaseOutput {
                    links_detected,
                    link_detection_ms: link_detection_start.elapsed().as_millis() as u64,
                    docs_link_detected: total_docs_processed - input.doc_offset,
                    interrupted: true,
                    doc_offset: total_docs_processed,
                });
            }
        }

        if crate::shutdown::is_shutdown_requested() {
            link_pb.finish_and_clear();
            return Ok(LinkPhaseOutput {
                links_detected,
                link_detection_ms: link_detection_start.elapsed().as_millis() as u64,
                docs_link_detected: total_docs_processed - input.doc_offset,
                interrupted: true,
                doc_offset: total_docs_processed,
            });
        }

        link_pb.set_position((batch_idx + 1) as u64);
        let docs_done = (batch_idx + 1) * link_batch_size;
        input.progress.report(
            docs_done.min(docs_to_scan.len()),
            docs_to_scan.len(),
            "documents link-detected",
        );

        // Prepare batch data
        let batch_docs: Vec<(&str, &str, &str)> = chunk
            .iter()
            .map(|(id, doc)| (id.as_str(), doc.title.as_str(), doc.content.as_str()))
            .collect();

        // Batch detect links
        let batch_results = input
            .link_detector
            .detect_links_batch(&batch_docs, &known_entities);

        // Store results
        for (id, _) in chunk {
            if !input.changed_ids.contains(id.as_str()) {
                rescan_count += 1;
            }
            if let Some(links) = batch_results.get(id.as_str()) {
                links_detected += links.len();
                input.db.update_links(id, links)?;
            }
        }

        total_docs_processed += chunk.len();
    }

    link_pb.finish_and_clear();

    if input.added_count > 0 && !full_rescan {
        info!(
            "Link detection: {} changed + {} keyword-matched of {} total docs",
            input.changed_ids.len(),
            rescan_count,
            all_docs.len()
        );
    }

    Ok(LinkPhaseOutput {
        links_detected,
        link_detection_ms: link_detection_start.elapsed().as_millis() as u64,
        docs_link_detected: total_docs_processed - input.doc_offset,
        interrupted: false,
        doc_offset: total_docs_processed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::embedding::EmbeddingProvider;
    use crate::models::{Document, Repository};
    use crate::ProgressReporter;
    use tempfile::TempDir;

    fn setup_repo_with_docs(db: &crate::Database) -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository {
            id: "test".into(),
            name: "test".into(),
            path: tmp.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        // Insert two documents that reference each other
        let doc1 = Document {
            id: "aaa111".into(),
            repo_id: "test".into(),
            file_path: "alpha.md".into(),
            file_hash: "h1".into(),
            title: "Alpha".into(),
            doc_type: Some("document".into()),
            content: "# Alpha\n\nThis mentions Beta.".into(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        let doc2 = Document {
            id: "bbb222".into(),
            repo_id: "test".into(),
            file_path: "beta.md".into(),
            file_hash: "h2".into(),
            title: "Beta".into(),
            doc_type: Some("document".into()),
            content: "# Beta\n\nThis mentions Alpha.".into(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        // Store embeddings so find_similar works
        let emb = MockEmbedding::new(1024);
        let dim = emb.dimension();
        db.upsert_embedding_chunk("aaa111", 0, 0, 100, &vec![0.1; dim])
            .unwrap();
        db.upsert_embedding_chunk("bbb222", 0, 0, 100, &vec![0.1; dim])
            .unwrap();

        (tmp, "test".into())
    }

    #[tokio::test]
    async fn test_link_phase_skip_links() {
        let (db, _tmp) = test_db();
        let changed = HashSet::new();
        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: "test",
            changed_ids: &changed,
            added_count: 0,
            show_progress: false,
            verbose: false,
            skip_links: true,
            force_relink: false,
            link_batch_size: 5,
            progress: &progress,
            deadline: None,
            doc_offset: 0,
        })
        .await
        .unwrap();

        assert_eq!(output.links_detected, 0);
        assert_eq!(output.link_detection_ms, 0);
        assert!(!output.interrupted);
    }

    #[tokio::test]
    async fn test_link_phase_detects_links_for_changed_docs() {
        let (db, _db_tmp) = test_db();
        let (_tmp, repo_id) = setup_repo_with_docs(&db);

        let mut changed = HashSet::new();
        changed.insert("aaa111".to_string());

        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: &repo_id,
            changed_ids: &changed,
            added_count: 1,
            show_progress: false,
            verbose: false,
            skip_links: false,
            force_relink: false,
            link_batch_size: 5,
            progress: &progress,
            deadline: None,
            doc_offset: 0,
        })
        .await
        .unwrap();

        assert!(!output.interrupted);
        // Both docs should be scanned (changed + keyword match)
        assert!(output.docs_link_detected >= 1);
    }

    #[tokio::test]
    async fn test_link_phase_force_relink() {
        let (db, _db_tmp) = test_db();
        let (_tmp, repo_id) = setup_repo_with_docs(&db);

        let changed = HashSet::new(); // no changes
        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: &repo_id,
            changed_ids: &changed,
            added_count: 0,
            show_progress: false,
            verbose: false,
            skip_links: false,
            force_relink: true,
            link_batch_size: 5,
            progress: &progress,
            deadline: None,
            doc_offset: 0,
        })
        .await
        .unwrap();

        // Force relink should process all docs even with no changes
        assert_eq!(output.docs_link_detected, 2);
        assert!(!output.interrupted);
    }

    #[tokio::test]
    async fn test_link_phase_deadline_interrupts() {
        let (db, _db_tmp) = test_db();
        let (_tmp, repo_id) = setup_repo_with_docs(&db);

        let mut changed = HashSet::new();
        changed.insert("aaa111".to_string());

        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: &repo_id,
            changed_ids: &changed,
            added_count: 0,
            show_progress: false,
            verbose: false,
            skip_links: false,
            force_relink: true,
            link_batch_size: 1, // small batch to hit deadline check
            progress: &progress,
            deadline: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            doc_offset: 0,
        })
        .await
        .unwrap();

        assert!(output.interrupted);
    }

    #[tokio::test]
    async fn test_link_phase_empty_repo() {
        let (db, _db_tmp) = test_db();
        let repo = Repository {
            id: "empty".into(),
            name: "empty".into(),
            path: std::path::PathBuf::from("/tmp/empty"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let changed = HashSet::new();
        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: "empty",
            changed_ids: &changed,
            added_count: 0,
            show_progress: false,
            verbose: false,
            skip_links: false,
            force_relink: false,
            link_batch_size: 5,
            progress: &progress,
            deadline: None,
            doc_offset: 0,
        })
        .await
        .unwrap();

        assert_eq!(output.links_detected, 0);
        assert_eq!(output.docs_link_detected, 0);
    }

    #[tokio::test]
    async fn test_link_phase_doc_offset_resume() {
        let (db, _db_tmp) = test_db();
        let (_tmp, repo_id) = setup_repo_with_docs(&db);

        let changed = HashSet::new();
        let link_detector = LinkDetector::new();
        let progress = ProgressReporter::Silent;

        let output = run_link_detection_phase(LinkPhaseInput {
            db: &db,
            link_detector: &link_detector,
            repo_id: &repo_id,
            changed_ids: &changed,
            added_count: 0,
            show_progress: false,
            verbose: false,
            skip_links: false,
            force_relink: true,
            link_batch_size: 5,
            progress: &progress,
            deadline: None,
            doc_offset: 1, // skip first doc
        })
        .await
        .unwrap();

        // Should process only 1 of 2 docs
        assert_eq!(output.docs_link_detected, 1);
        assert_eq!(output.doc_offset, 2);
    }
}
