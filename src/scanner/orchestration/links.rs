//! Link detection phase for document scanning

use std::collections::HashSet;
use std::time::Instant;
use tracing::info;

use crate::{Database, LinkDetector};
use crate::progress::ProgressReporter;

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
}

/// Output from the link detection phase
pub struct LinkPhaseOutput {
    pub links_detected: usize,
    pub link_detection_ms: u64,
    pub docs_link_detected: usize,
    pub interrupted: bool,
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

    // Process in batches
    for (batch_idx, chunk) in docs_to_scan.chunks(link_batch_size).enumerate() {
        if crate::shutdown::is_shutdown_requested() {
            link_pb.finish_and_clear();
            return Ok(LinkPhaseOutput {
                links_detected,
                link_detection_ms: link_detection_start.elapsed().as_millis() as u64,
                docs_link_detected: batch_idx * link_batch_size,
                interrupted: true,
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
        docs_link_detected: docs_to_scan.len(),
        interrupted: false,
    })
}
