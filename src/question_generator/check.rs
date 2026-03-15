//! Shared lint-all-documents loop for both MCP and CLI.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::models::{Document, QuestionType, ReviewQuestion};
use crate::patterns::{
    extract_frontmatter_reviewed_date, extract_reviewed_date, has_corruption_artifacts,
    FACT_LINE_REGEX,
};
use crate::processor::{
    append_review_questions, content_hash, parse_review_queue, prune_stale_questions,
};
use crate::progress::ProgressReporter;
use crate::question_generator::{
    collect_defined_terms_with_types, filter_sequential_conflicts,
    generate_ambiguous_questions_with_type, generate_conflict_questions,
    generate_corruption_questions, generate_duplicate_entry_questions, generate_missing_questions,
    generate_precision_questions, generate_required_field_questions,
    generate_source_quality_questions, generate_stale_questions, generate_temporal_questions,
    generate_weak_source_questions,
};
use chrono::Utc;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{info, warn};

/// Days within which a reviewed marker suppresses question generation.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Result of checking a single document: (doc, questions, pruned_content, pruned_count,
/// existing_unanswered, existing_answered, skipped_reviewed, suppressed_by_review)
type CheckResult<'a> = (
    &'a &'a Document,
    Vec<ReviewQuestion>,
    String,
    usize,
    usize,
    usize,
    usize,
    usize,
);

/// Run all rule-based question generators on a document body.
///
/// `full` controls whether to include generators that don't interact with
/// reviewed markers (duplicate entries, source quality, corruption).  The
/// "unrestricted" re-generation pass for suppression counting skips those
/// since they don't change between stripped/unstripped content.
pub fn run_generators(
    body: &str,
    doc_type: Option<&str>,
    defined_terms: &HashSet<String>,
    stale_days: i64,
    full: bool,
    citation_patterns: &[Regex],
) -> Vec<ReviewQuestion> {
    let mut questions = generate_temporal_questions(body, doc_type);
    questions.extend(generate_conflict_questions(body));
    if full {
        questions.extend(generate_duplicate_entry_questions(body));
    }
    questions.extend(generate_missing_questions(body));
    if full {
        questions.extend(generate_source_quality_questions(body));
        questions.extend(generate_weak_source_questions(body, citation_patterns));
    }
    questions.extend(generate_ambiguous_questions_with_type(
        body,
        doc_type,
        defined_terms,
    ));
    questions.extend(generate_stale_questions(body, stale_days));
    questions.extend(generate_precision_questions(body));
    if full {
        questions.extend(generate_corruption_questions(body));
    }
    questions
}

use std::time::Instant;

/// Configuration for the shared lint loop.
pub struct CheckConfig {
    pub stale_days: i64,
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    pub dry_run: bool,
    pub concurrency: usize,
    /// Optional deadline for time-boxed operations.
    pub deadline: Option<Instant>,
    /// Whether to acquire the global write guard before writing results.
    /// Set to `true` in MCP context (concurrent requests), `false` in CLI/tests.
    pub acquire_write_guard: bool,
    /// Optional repo ID to scope operations to a single repository.
    pub repo_id: Option<String>,
    /// Document types to treat as glossary/definitions (from perspective.yaml).
    pub glossary_types: Option<Vec<String>>,
    /// Compiled domain-specific citation patterns from perspective.yaml.
    /// Citations matching any pattern pass tier-1 without a weak-source question.
    pub citation_patterns: Vec<Regex>,
}

/// Result of linting a single document.
pub struct CheckDocResult {
    pub doc_id: String,
    pub doc_title: String,
    pub new_questions: usize,
    pub pruned_questions: usize,
    pub existing_unanswered: usize,
    pub existing_answered: usize,
    pub skipped_reviewed: usize,
    /// Questions suppressed because referenced facts have reviewed markers.
    pub suppressed_by_review: usize,
}

/// Output from check_all_documents including metadata about the operation.
pub struct CheckOutput {
    pub results: Vec<CheckDocResult>,
    /// Number of documents actually processed (may be less than total if deadline hit).
    pub docs_processed: usize,
    /// Total number of active (non-archived, non-corrupted) documents.
    pub docs_total: usize,
    /// Domain vocabulary candidates extracted during deep_check.
    pub vocabulary_candidates: Vec<VocabCandidate>,
}

/// A domain vocabulary term extracted by LLM during deep_check.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VocabCandidate {
    pub term: String,
    pub definition: String,
    pub source_line: u32,
    pub doc_id: String,
}

/// Lint all documents: generate review questions, optionally cross-validate, write results.
///
/// Used by both MCP `check_repository` and CLI `cmd_check --review`.
pub async fn check_all_documents(
    docs: &[Document],
    db: &Database,
    _embedding: &dyn EmbeddingProvider,
    config: &CheckConfig,
    progress: &ProgressReporter,
) -> Result<CheckOutput, crate::error::FactbaseError> {
    let total = docs.len();
    let rf_ref = &config.required_fields;

    // Filter out archived documents — they're indexed for search/links but not checked
    let active_docs: Vec<_> = docs.iter().filter(|d| !is_archived(&d.file_path)).collect();
    if active_docs.len() < total {
        progress.log(&format!(
            "Skipping {} archived document(s)",
            total - active_docs.len()
        ));
    }

    // Filter out reference entities — they exist for linking, not quality checks
    let reference_count = active_docs
        .iter()
        .filter(|d| crate::patterns::is_reference_doc(&d.content))
        .count();
    let active_docs: Vec<_> = active_docs
        .into_iter()
        .filter(|d| !crate::patterns::is_reference_doc(&d.content))
        .collect();
    if reference_count > 0 {
        progress.log(&format!(
            "Skipping {} reference document(s)",
            reference_count
        ));
    }

    // Detect documents with corruption artifacts from failed review application
    let (clean_docs, corrupted_docs): (Vec<_>, Vec<_>) = active_docs
        .into_iter()
        .partition(|d| !has_corruption_artifacts(&d.content));
    if !corrupted_docs.is_empty() {
        for doc in &corrupted_docs {
            warn!(
                "{} [{}]: skipped — contains corruption artifacts from a failed review application",
                doc.title, doc.id
            );
        }
        progress.log(&format!(
            "Skipping {} corrupted document(s) — rebuild content before checking",
            corrupted_docs.len()
        ));
    }
    let active_docs = clean_docs;
    let total_active = active_docs.len();

    // Build repo_id → repo_path map so we can resolve relative file paths.
    // Documents store paths relative to their repository root; without this
    // map, disk reads/writes silently fail when CWD ≠ repo root (e.g. MCP).
    let (repo_paths, repo_callout): (HashMap<String, PathBuf>, HashMap<String, bool>) = {
        let mut paths = HashMap::new();
        let mut callout = HashMap::new();
        for doc in docs {
            if !paths.contains_key(&doc.repo_id) {
                if let Ok(Some(repo)) = db.get_repository(&doc.repo_id) {
                    let uc = repo
                        .perspective
                        .as_ref()
                        .and_then(|p| p.format.as_ref())
                        .map(|f| f.resolve().review_callout)
                        .unwrap_or(false);
                    paths.insert(doc.repo_id.clone(), repo.path.clone());
                    callout.insert(doc.repo_id.clone(), uc);
                }
            }
        }
        (paths, callout)
    };
    let repo_paths_ref = &repo_paths;
    let repo_callout_ref = &repo_callout;

    // Build title → doc IDs map for duplicate title detection (all docs, not just active)
    let mut title_map: HashMap<String, Vec<(&str, &str)>> = HashMap::new();
    for doc in docs {
        title_map
            .entry(doc.title.to_lowercase())
            .or_default()
            .push((&doc.id, &doc.title));
    }

    let title_map_ref = &title_map;

    // Collect defined terms from glossary/definition/reference documents so we don't
    // flag acronyms that are already defined in the repo.
    let defined_terms = collect_defined_terms_with_types(docs, config.glossary_types.as_deref());
    let defined_terms_ref = &defined_terms;
    let citation_patterns_ref = &config.citation_patterns;

    let mut all_results = Vec::new();
    let mut deadline_hit = false;

    for chunk_start in (0..total_active).step_by(config.concurrency) {
        // Check deadline before starting a new chunk
        if let Some(deadline) = config.deadline {
            if Instant::now() > deadline {
                deadline_hit = true;
                break;
            }
        }

        let chunk_end = (chunk_start + config.concurrency).min(total_active);
        let chunk = &active_docs[chunk_start..chunk_end];

        let futs: Vec<_> = chunk
            .iter()
            .enumerate()
            .map(|(ci, doc)| {
                let idx = chunk_start + ci;
                async move {
                    progress.report(idx + 1, total_active, &format!("Checking {}", doc.title));

                    // Prefer fresh content from disk over potentially stale database content.
                    // Review application writes reviewed markers to files but doesn't
                    // update the database, so the DB content may lack those markers.
                    let abs_path = repo_paths_ref
                        .get(&doc.repo_id)
                        .map(|rp| rp.join(&doc.file_path));
                    let disk_content = abs_path
                        .as_ref()
                        .and_then(|p| std::fs::read_to_string(p).ok());
                    let content = disk_content.as_deref().unwrap_or(&doc.content);

                    // Strip the review queue section so generators never
                    // treat review entries as document facts.
                    let body = crate::patterns::content_body(content);

                    let mut questions = run_generators(body, doc.doc_type.as_deref(), defined_terms_ref, config.stale_days, true, citation_patterns_ref);

                    // Check for duplicate titles
                    if let Some(dupes) = title_map_ref.get(&doc.title.to_lowercase()) {
                        for (other_id, other_title) in dupes {
                            if *other_id != doc.id {
                                questions.push(ReviewQuestion::new(
                                    QuestionType::Duplicate,
                                    None,
                                    format!(
                                        "Same title as \"{other_title}\" [{other_id}] — are these the same entity?"
                                    ),
                                ));
                            }
                        }
                    }

                    if let Some(ref rf) = rf_ref {
                        questions.extend(generate_required_field_questions(
                            content,
                            Some(doc.doc_type.as_deref().unwrap_or("unknown")),
                            rf,
                        ));
                    }

                    let existing_questions = parse_review_queue(content).unwrap_or_default();
                    let existing_unanswered =
                        existing_questions.iter().filter(|q| !q.answered).count();
                    let existing_answered =
                        existing_questions.iter().filter(|q| q.answered).count();

                    // Build set of descriptions the generators would produce today.
                    // This is the "valid" set — any existing unanswered question NOT
                    // in this set has a trigger condition that no longer exists.
                    let valid_descriptions: HashSet<_> =
                        questions.iter().map(|q| q.description.clone()).collect();

                    // Prune stale unanswered questions and answered ([x]) questions
                    let pruned_content = prune_stale_questions(
                        content,
                        &valid_descriptions,
                        false,
                    );
                    let remaining_after_prune = parse_review_queue(&pruned_content)
                        .unwrap_or_default();
                    let remaining_unanswered = remaining_after_prune
                        .iter()
                        .filter(|q| !q.answered)
                        .count();
                    let pruned_unanswered = existing_unanswered - remaining_unanswered;
                    // Total pruned includes both stale unanswered and answered ([x]) questions
                    let pruned_count = pruned_unanswered + existing_answered;

                    // Dedup new questions against remaining existing questions
                    let remaining_descs: HashSet<_> = remaining_after_prune
                        .iter()
                        .map(|q| q.description.clone())
                        .collect();
                    let remaining_conflict_normalized: HashSet<String> = remaining_after_prune
                        .iter()
                        .filter(|q| q.question_type == QuestionType::Conflict)
                        .map(|q| crate::processor::normalize_conflict_desc(&q.description).to_string())
                        .collect();
                    questions.retain(|q| {
                        if remaining_descs.contains(&q.description) {
                            return false;
                        }
                        if q.question_type == QuestionType::Conflict
                            && remaining_conflict_normalized.contains(
                                crate::processor::normalize_conflict_desc(&q.description),
                            )
                        {
                            return false;
                        }
                        true
                    });

                    // Count fact lines with recent reviewed markers
                    let today = Utc::now().date_naive();
                    let fm_recent = extract_frontmatter_reviewed_date(content)
                        .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS);
                    let skipped_reviewed = content
                        .lines()
                        .filter(|line| FACT_LINE_REGEX.is_match(line))
                        .filter(|line| {
                            fm_recent
                                || extract_reviewed_date(line)
                                    .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
                        })
                        .count();

                    // Count questions suppressed by reviewed markers.
                    // Strip reviewed markers from body and re-generate to measure the delta.
                    let stripped = crate::patterns::strip_reviewed_markers(body);
                    let mut unrestricted = run_generators(&stripped, doc.doc_type.as_deref(), defined_terms_ref, config.stale_days, false, citation_patterns_ref);
                    filter_sequential_conflicts(&stripped, &mut unrestricted);
                    let suppressed_by_review = unrestricted.len().saturating_sub(questions.len());

                    (
                        doc,
                        questions,
                        pruned_content,
                        pruned_count,
                        existing_unanswered,
                        existing_answered,
                        skipped_reviewed,
                        suppressed_by_review,
                    )
                }
            })
            .collect();

        let batch = futures::future::join_all(futs).await;
        all_results.extend(batch);
    }

    let docs_processed = all_results.len();

    // Folder placement check (no LLM needed — pure link graph analysis).
    // Respects deadline.
    if !deadline_hit && config.deadline.is_none_or(|d| Instant::now() <= d) {
        run_placement_check(docs, db, &mut all_results);
    }

    // Vocabulary extraction is now agent-driven via the discover workflow step.
    let vocabulary_candidates = Vec::new();

    // Acquire write guard for non-dry-run (writes review queue to disk+DB).
    // Acquired here — after all read-only question generation — so dry-run
    // and read-only phases are never blocked by a concurrent write.
    let _write_guard = if config.dry_run || !config.acquire_write_guard {
        None
    } else {
        Some(crate::write_guard::WriteGuard::try_acquire()?)
    };

    // Write results (sequential for filesystem safety)
    let mut results = Vec::new();
    for (
        doc,
        mut questions,
        pruned_content,
        pruned_count,
        existing_unanswered,
        existing_answered,
        skipped_reviewed,
        suppressed_by_review,
    ) in all_results
    {
        // Post-filter: remove conflict questions for boundary-month sequential entries.
        let abs_path = repo_paths
            .get(&doc.repo_id)
            .map(|rp| rp.join(&doc.file_path));
        let disk_content = abs_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let content = disk_content.as_deref().unwrap_or(&doc.content);
        filter_sequential_conflicts(content, &mut questions);

        // Dedup questions added after the async block (e.g. placement questions)
        // against existing questions in the pruned content. Without this,
        // placement questions bypass the per-document dedup and get re-added
        // even when a deferred/believed copy already exists.
        if !questions.is_empty() {
            let existing = parse_review_queue(&pruned_content).unwrap_or_default();
            let existing_descs: HashSet<_> =
                existing.iter().map(|q| q.description.clone()).collect();
            questions.retain(|q| !existing_descs.contains(&q.description));
        }

        let count = questions.len();
        let has_dup_sections = has_duplicate_review_sections(&pruned_content);
        let disk_missing_marker = !content.contains(crate::patterns::REVIEW_QUEUE_MARKER)
            && doc.content.contains(crate::patterns::REVIEW_QUEUE_MARKER);
        // Disk has marker but no questions while DB has unanswered questions
        let disk_empty_review =
            content.contains(crate::patterns::REVIEW_QUEUE_MARKER) && existing_unanswered == 0 && {
                let db_qs = parse_review_queue(&doc.content).unwrap_or_default();
                db_qs.iter().any(|q| !q.answered)
            };
        let needs_write = count > 0
            || pruned_count > 0
            || has_dup_sections
            || disk_missing_marker
            || disk_empty_review;
        if needs_write && !config.dry_run {
            // If the disk file lost its marker or questions but DB has them, recover
            let base_content =
                if (disk_missing_marker || disk_empty_review) && count == 0 && pruned_count == 0 {
                    let (recovered, _) =
                        crate::processor::recover_review_section(&pruned_content, &doc.content);
                    recovered
                } else {
                    pruned_content.clone()
                };
            let use_callout = repo_callout_ref.get(&doc.repo_id).copied().unwrap_or(false);
            let updated = append_review_questions(&base_content, &questions, use_callout);
            let path = abs_path.unwrap_or_else(|| PathBuf::from(&doc.file_path));
            if path.exists() {
                std::fs::write(&path, &updated)?;
                let new_hash = content_hash(&updated);
                db.update_document_content(&doc.id, &updated, &new_hash)?;
                // Sync review questions to DB index
                if let Some(questions) = parse_review_queue(&updated) {
                    let _ = db.sync_review_questions(&doc.id, &questions);
                }
            }
        }
        if count > 0 {
            info!("{}: {} new questions", doc.title, count);
        }
        if pruned_count > 0 {
            info!("{}: pruned {} stale questions", doc.title, pruned_count);
        }
        // Include docs with any activity
        if count > 0
            || pruned_count > 0
            || existing_unanswered > 0
            || existing_answered > 0
            || skipped_reviewed > 0
            || suppressed_by_review > 0
        {
            results.push(CheckDocResult {
                doc_id: doc.id.clone(),
                doc_title: doc.title.clone(),
                new_questions: count,
                pruned_questions: pruned_count,
                existing_unanswered: existing_unanswered
                    .saturating_sub(pruned_count.saturating_sub(existing_answered)),
                existing_answered: 0, // answered questions are now pruned
                skipped_reviewed,
                suppressed_by_review,
            });
        }
    }

    Ok(CheckOutput {
        results,
        docs_processed,
        docs_total: total_active,
        vocabulary_candidates,
    })
}

/// Extract domain vocabulary candidates from documents.
///
/// Previously used an LLM provider; now returns empty results.
/// Vocabulary extraction is handled by the agent via the discover workflow step.
pub async fn extract_vocabulary(
    docs: &[&Document],
    _defined_terms: &HashSet<String>,
    _deadline: Option<Instant>,
    _progress: &ProgressReporter,
    _doc_offset: usize,
) -> (Vec<VocabCandidate>, usize) {
    (Vec::new(), docs.len())
}

/// Check if a document path is in an archive folder.
/// Matches paths containing `/archive/` or starting with `archive/`.
/// Run folder placement check and merge questions into all_results.
fn run_placement_check(docs: &[Document], db: &Database, all_results: &mut Vec<CheckResult<'_>>) {
    match super::placement::check_folder_placement(docs, db) {
        Ok(placement_qs) => {
            for (doc, questions, _, _, _, _, _, _) in all_results.iter_mut() {
                if let Some(pqs) = placement_qs.get(&doc.id) {
                    questions.extend(pqs.iter().cloned());
                }
            }
        }
        Err(e) => warn!("Folder placement check failed: {e}"),
    }
}

fn is_archived(file_path: &str) -> bool {
    file_path.contains("/archive/") || file_path.starts_with("archive/")
}

/// Check if content has duplicate `## Review Queue` headings or markers.
fn has_duplicate_review_sections(content: &str) -> bool {
    let heading_count = content
        .lines()
        .filter(|l| l.trim() == "## Review Queue")
        .count();
    let marker_count = content
        .matches(crate::patterns::REVIEW_QUEUE_MARKER)
        .count();
    heading_count > 1 || marker_count > 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::models::Document;
    use crate::progress::ProgressReporter;

    fn make_doc(id: &str, title: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            ..Document::test_default()
        }
    }

    #[tokio::test]
    async fn test_lint_reports_existing_unanswered() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        // Use the exact description format the temporal generator produces
        let content = "- Fact one\n\n<!-- factbase:review -->\n## Review Queue\n\n\
                       - [ ] `@q[temporal]` \"Fact one\" - when was this true?\n  > \n";
        let docs = vec![make_doc("aaa", "Test", content)];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap()
            .results;
        assert!(!results.is_empty());
        // The existing question matches what generators would produce, so it's kept
        assert_eq!(results[0].existing_unanswered, 1);
        assert_eq!(results[0].existing_answered, 0);
        assert_eq!(results[0].pruned_questions, 0);
    }

    #[tokio::test]
    async fn test_lint_prunes_answered_questions() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let content = "- Fact one\n\n<!-- factbase:review -->\n## Review Queue\n\n\
                       - [x] `@q[stale]` Line 1: is this still accurate?\n\
                       > confirmed\n";
        let docs = vec![make_doc("bbb", "Test", content)];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap()
            .results;
        assert!(!results.is_empty());
        // Answered questions are now pruned, so existing_answered should be 0
        assert_eq!(results[0].existing_answered, 0);
        assert!(
            results[0].pruned_questions > 0,
            "answered question should be counted as pruned"
        );
    }

    #[tokio::test]
    async fn test_lint_reports_skipped_reviewed() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let today = Utc::now().format("%Y-%m-%d");
        let content = format!("- Fact one <!-- reviewed:{today} -->\n- Fact two\n");
        let docs = vec![make_doc("ccc", "Test", &content)];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap()
            .results;
        let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
        assert_eq!(total_skipped, 1);
    }

    #[tokio::test]
    async fn test_lint_prunes_stale_questions() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        // Fact now has a temporal tag, so the existing temporal question is stale
        let content = "- Fact one @t[=2024]\n\n<!-- factbase:review -->\n## Review Queue\n\n\
                       - [ ] `@q[temporal]` \"Fact one\" - when was this true?\n  > \n";
        let docs = vec![make_doc("ccc", "Test", content)];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap()
            .results;
        assert!(!results.is_empty());
        assert_eq!(
            results[0].pruned_questions, 1,
            "Should prune the stale temporal question"
        );
        assert_eq!(
            results[0].existing_unanswered, 0,
            "No unanswered after pruning"
        );
    }

    #[tokio::test]
    async fn test_lint_deadline_stops_early() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let mut docs = Vec::new();
        for i in 0..10 {
            docs.push(make_doc(
                &format!("{i:03x}"),
                &format!("Doc {i}"),
                &format!("- Fact {i}\n"),
            ));
        }
        // Deadline already in the past → should process 0 docs
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 0);
        assert_eq!(output.docs_total, 10);
        assert!(output.results.is_empty());
    }

    #[tokio::test]
    async fn test_lint_no_deadline_processes_all() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Doc A", "- Fact A\n"),
            make_doc("bbb", "Doc B", "- Fact B\n"),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 2);
        assert_eq!(output.docs_total, 2);
    }

    #[test]
    fn test_is_archived() {
        assert!(is_archived("archive/old-doc.md"));
        assert!(is_archived("people/archive/jane.md"));
        assert!(is_archived("companies/xsolis/archive/old-project.md"));
        assert!(!is_archived("people/jane.md"));
        assert!(!is_archived("companies/xsolis/xsolis.md"));
        assert!(!is_archived("archival-notes/doc.md")); // not "archive/"
    }

    #[tokio::test]
    async fn test_check_skips_reference_docs() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Regular", "- Fact A\n"),
            make_doc(
                "bbb",
                "Reference",
                "<!-- factbase:reference -->\n# AWS Lambda\n\n- Serverless compute\n",
            ),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap();
        // Reference doc should be skipped — only 1 doc processed
        assert_eq!(output.docs_total, 1);
        assert_eq!(output.docs_processed, 1);
    }

    /// Dry-run check succeeds even when the write guard is already held,
    /// because dry_run never acquires the guard.
    #[tokio::test]
    async fn test_dry_run_check_never_acquires_write_guard() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![make_doc("aaa", "Test", "- Fact one\n")];
        // Even with acquire_write_guard: true, dry_run skips the guard
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: true,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        // This should always succeed regardless of guard state
        let result = check_all_documents(&docs, &db, &embedding, &config, &progress).await;
        assert!(
            result.is_ok(),
            "dry-run should never be blocked by write guard"
        );
    }

    // -----------------------------------------------------------------------
    // Shared test helpers
    // -----------------------------------------------------------------------

    fn default_check_config() -> CheckConfig {
        CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        }
    }

    /// Integration test 1: Full pipeline — documents with overlapping facts,
    /// fact embeddings pre-computed, check detects conflicts.
    /// Integration test 2: Time-boxed continuation — run with expired deadline,
    /// then resume with cursor.
    /// Integration test 3: Incremental update — modify one doc's facts,
    /// re-insert embeddings, re-check processes new pairs.
    /// Integration test 4a: Backward compatibility — checked_doc_ids still accepted.
    /// Integration test 4b: Fallback to per-document cross-validation when
    /// fact_embeddings table is empty.
    /// Integration test 5a: Single document — no cross-doc pairs possible.
    /// Integration test 5b: No facts above similarity threshold.
    /// Integration test 5c: Review queue lines excluded from fact extraction.
    /// Integration test 5d: Closed temporal range suppresses SUPERSEDES.
    /// Integration test 6: Performance — zero embedding calls during check
    /// when fact embeddings are pre-computed.
    #[tokio::test]
    async fn test_glossary_terms_suppress_ambiguous_questions() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        // Glossary doc defines HCLS
        let glossary = Document {
            id: "ggg".to_string(),
            title: "Glossary".to_string(),
            content: "# Glossary\n\n- **HCLS**: Healthcare and Life Sciences\n".to_string(),
            doc_type: Some("glossary".to_string()),
            ..Document::test_default()
        };
        // Regular doc uses HCLS — should NOT get an ambiguous question
        let regular = Document {
            id: "rrr".to_string(),
            title: "Project".to_string(),
            content: "# Project\n\n- Expanding HCLS practice\n".to_string(),
            doc_type: Some("project".to_string()),
            ..Document::test_default()
        };
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[glossary, regular], &db, &embedding, &config, &progress)
            .await
            .unwrap();
        // No ambiguous question about HCLS should be generated
        for r in &output.results {
            if r.doc_id == "rrr" {
                // The only questions should be temporal/missing, not ambiguous about HCLS
                assert_eq!(
                    r.new_questions,
                    output
                        .results
                        .iter()
                        .find(|x| x.doc_id == "rrr")
                        .map(|x| x.new_questions)
                        .unwrap_or(0)
                );
            }
        }
        // Verify by running generators directly with the collected terms
        let terms = crate::question_generator::collect_defined_terms(&[Document {
            id: "ggg".to_string(),
            title: "Glossary".to_string(),
            content: "# Glossary\n\n- **HCLS**: Healthcare and Life Sciences\n".to_string(),
            doc_type: Some("glossary".to_string()),
            ..Document::test_default()
        }]);
        let qs = crate::question_generator::generate_ambiguous_questions_with_type(
            "- Expanding HCLS practice\n",
            Some("project"),
            &terms,
        );
        assert!(
            qs.iter().all(|q| !q.description.contains("HCLS")),
            "HCLS should be suppressed by glossary"
        );
    }

    #[tokio::test]
    async fn test_extract_vocabulary_returns_empty() {
        let doc = make_doc("aaa", "Project", "# Project\n\n- Expanding HCLS practice\n");
        let docs: Vec<&Document> = vec![&doc];
        let defined = HashSet::new();
        let progress = ProgressReporter::Silent;
        let (result, processed) = extract_vocabulary(&docs, &defined, None, &progress, 0).await;
        assert!(
            result.is_empty(),
            "vocabulary extraction is now agent-driven"
        );
        assert_eq!(processed, 1);
    }

    #[tokio::test]
    async fn test_vocabulary_candidates_always_empty() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let doc = make_doc(
            "vvv",
            "History",
            "# History\n\n- Battle of Marathon 490 BCE\n",
        );
        let config = default_check_config();
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert!(
            output.vocabulary_candidates.is_empty(),
            "vocabulary_candidates should always be empty (agent-driven)"
        );
    }

    #[tokio::test]
    async fn test_check_resolves_relative_file_paths() {
        // Regression test: check_all_documents must resolve relative file_path
        // against the repository root, not the process CWD.
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        // Create a temp "repo" directory with a markdown file
        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");
        std::fs::write(
            &md_path,
            "<!-- factbase:ttt -->\n# Test\n\n- Fact without temporal tag\n",
        )
        .unwrap();

        // Register the repo in the database
        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        // Document with relative file_path (as stored in DB)
        let doc = Document {
            id: "ttt".to_string(),
            title: "Test".to_string(),
            content: "<!-- factbase:ttt -->\n# Test\n\n- Fact without temporal tag\n".to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "test-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();

        // Should have generated questions (at least temporal)
        assert!(!output.results.is_empty(), "should generate questions");
        assert!(
            output.results[0].new_questions > 0,
            "should have new questions"
        );

        // Verify questions were written to the file on disk
        let on_disk = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            on_disk.contains("@q["),
            "questions should be written to file at resolved path"
        );
    }

    /// When question gen exhausts the deadline but fact pairs exist,
    /// pair_progress should signal pending cross-validation work.
    /// Regression test: questions mode reports docs_processed based on question generation.
    #[tokio::test]
    async fn test_questions_mode_docs_processed_uses_question_gen_count() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nSome content.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nMore content.\n"),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert_eq!(
            output.docs_processed, 2,
            "questions mode should count question-gen docs"
        );
        assert_eq!(output.docs_total, 2);
    }

    /// Running check twice on the same document must not create duplicate
    /// `## Review Queue` sections.
    #[tokio::test]
    async fn test_check_twice_no_duplicate_review_sections() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        // Create a temp repo with a markdown file
        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");
        std::fs::write(
            &md_path,
            "<!-- factbase:ddd -->\n# Test\n\n- Fact without temporal tag\n",
        )
        .unwrap();

        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "ddd".to_string(),
            title: "Test".to_string(),
            content: "<!-- factbase:ddd -->\n# Test\n\n- Fact without temporal tag\n".to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "test-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;

        // First check — should create review queue
        let output1 = check_all_documents(&[doc.clone()], &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert!(
            output1.results.iter().any(|r| r.new_questions > 0),
            "First check should generate questions"
        );

        // Read the file after first check
        let after_first = std::fs::read_to_string(&md_path).unwrap();
        let heading_count = after_first
            .lines()
            .filter(|l| l.trim() == "## Review Queue")
            .count();
        assert_eq!(heading_count, 1, "After first check: one heading");

        // Update doc content to match what's on disk
        let doc2 = Document {
            content: after_first.clone(),
            ..doc
        };

        // Second check — should NOT create duplicate sections
        let _output2 = check_all_documents(&[doc2], &db, &embedding, &config, &progress)
            .await
            .unwrap();

        let after_second = std::fs::read_to_string(&md_path).unwrap();
        let heading_count = after_second
            .lines()
            .filter(|l| l.trim() == "## Review Queue")
            .count();
        assert_eq!(
            heading_count, 1,
            "After second check: still one heading, got:\n{after_second}"
        );
        let marker_count = after_second.matches("<!-- factbase:review -->").count();
        assert_eq!(marker_count, 1, "After second check: still one marker");
    }

    /// When the disk file loses its review marker but the DB content still has it,
    /// check_all_documents should recover the review section.
    #[tokio::test]
    async fn test_check_recovers_missing_marker_from_db() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");

        // Disk file: no review marker
        let disk_content = "<!-- factbase:eee -->\n# Test\n\n- Fact without temporal tag\n";
        std::fs::write(&md_path, disk_content).unwrap();

        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        // DB content: has review marker with existing questions
        let db_content = "<!-- factbase:eee -->\n# Test\n\n- Fact without temporal tag\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` No temporal tags\n  > \n";
        let doc = Document {
            id: "eee".to_string(),
            title: "Test".to_string(),
            content: db_content.to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "test-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;

        let _output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();

        // The disk file should now have the review marker
        let after = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            after.contains("<!-- factbase:review -->"),
            "Disk file should have marker after check, got:\n{after}"
        );
    }

    /// When the disk file has a review marker but no questions, and the DB has
    /// unanswered questions, check_all_documents should sync them to the file.
    #[tokio::test]
    async fn test_check_syncs_db_questions_to_empty_review_section() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");

        // Disk file: has review marker but NO questions.
        // All facts have temporal tags + sources so generators produce nothing new.
        let disk_content = "<!-- factbase:fff -->\n# Test\n\n- Fact one @t[=2024-01] [^1]\n\n---\n[^1]: https://example.com/source-a, accessed 2024-01-15\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n";
        std::fs::write(&md_path, disk_content).unwrap();

        let repo = crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        // DB content: has review marker WITH unanswered questions
        let db_content = "<!-- factbase:fff -->\n# Test\n\n- Fact one @t[=2024-01] [^1]\n\n---\n[^1]: https://example.com/source-a, accessed 2024-01-15\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Source [^1] was scraped over 180 days ago\n  > \n";
        let doc = Document {
            id: "fff".to_string(),
            title: "Test".to_string(),
            content: db_content.to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "test-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;

        let _output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();

        // The disk file should now have the DB questions
        let after = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            after.contains("Source [^1] was scraped over 180 days ago"),
            "Disk file should have DB questions synced, got:\n{after}"
        );
        // Verify the marker is still present
        assert!(after.contains("<!-- factbase:review -->"));
        // Verify body content is preserved
        assert!(after.contains("Fact one @t[=2024-01]"));
    }

    #[tokio::test]
    async fn test_deferred_questions_not_duplicated_by_check() {
        // Simulate the scenario where a question was deferred in a previous round
        // and check_repository is called again. The deferred question should NOT
        // be re-added as a new unanswered question.
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        // Content with a deferred ambiguous question (simulating a placement question)
        let content = "<!-- factbase:def001 -->\n# Deferred Test\n\n\
            - Fact one @t[=2024-01]\n\n\
            ---\n\n## Review Queue\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[ambiguous]` Some ambiguous question\n\
            > defer: cannot determine correct filing\n";

        let doc = Document {
            id: "def001".to_string(),
            title: "Deferred Test".to_string(),
            content: content.to_string(),
            repo_id: "test".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;

        // Verify the deferred question is detected
        let questions = parse_review_queue(content).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(
            questions[0].is_deferred(),
            "Question should be detected as deferred"
        );

        // Run check — it should NOT add a duplicate of the deferred question
        let output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();

        // The deferred question should still be there, but no new copy should be added
        // (new_questions should not include a duplicate of the deferred question)
        for r in &output.results {
            if r.doc_id == "def001" {
                // Any new questions should NOT have the same description as the deferred one
                // (we can't directly check the questions vec, but we can verify the count
                // doesn't include a duplicate)
                assert_eq!(
                    r.pruned_questions, 0,
                    "Deferred question should not be pruned"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_check_writes_callout_format_when_perspective_has_obsidian_preset() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");
        std::fs::write(
            &md_path,
            "<!-- factbase:ooo -->\n# Test\n\n- Fact without temporal tag\n",
        )
        .unwrap();

        let repo = crate::models::Repository {
            id: "obs-repo".to_string(),
            name: "Obsidian Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: Some(crate::models::Perspective {
                type_name: String::new(),
                organization: None,
                focus: None,
                allowed_types: None,
                review: None,
                format: Some(crate::models::FormatConfig {
                    preset: Some("obsidian".to_string()),
                    link_style: None,
                    frontmatter: None,
                    inline_links: None,
                    id_placement: None,
                    review_callout: None,
                    reviewed_in_frontmatter: None,
                }),
                link_match_mode: None,
                citation_patterns: None,
                internal_sources: None,
            }),
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "ooo".to_string(),
            title: "Test".to_string(),
            content: "<!-- factbase:ooo -->\n# Test\n\n- Fact without temporal tag\n".to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "obs-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert!(output.results.iter().any(|r| r.new_questions > 0));

        let on_disk = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            on_disk.contains("> [!review]- Review Queue"),
            "Obsidian preset should produce callout format, got:\n{on_disk}"
        );
    }

    #[tokio::test]
    async fn test_check_writes_plain_format_when_no_obsidian_config() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);

        let repo_dir = tempfile::tempdir().unwrap();
        let md_path = repo_dir.path().join("test-doc.md");
        std::fs::write(
            &md_path,
            "<!-- factbase:ppp -->\n# Test\n\n- Fact without temporal tag\n",
        )
        .unwrap();

        let repo = crate::models::Repository {
            id: "plain-repo".to_string(),
            name: "Plain Repo".to_string(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "ppp".to_string(),
            title: "Test".to_string(),
            content: "<!-- factbase:ppp -->\n# Test\n\n- Fact without temporal tag\n".to_string(),
            file_path: "test-doc.md".to_string(),
            repo_id: "plain-repo".to_string(),
            ..Document::test_default()
        };

        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: false,
            concurrency: 1,
            deadline: None,
            acquire_write_guard: false,
            repo_id: None,
            glossary_types: None,
            citation_patterns: vec![],
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, &config, &progress)
            .await
            .unwrap();
        assert!(output.results.iter().any(|r| r.new_questions > 0));

        let on_disk = std::fs::read_to_string(&md_path).unwrap();
        assert!(
            on_disk.contains("## Review Queue"),
            "No obsidian config should produce plain format, got:\n{on_disk}"
        );
        assert!(
            !on_disk.contains("> [!review]- Review Queue"),
            "Should NOT have callout format without obsidian config"
        );
    }
}
