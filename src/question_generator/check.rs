//! Shared lint-all-documents loop for both MCP and CLI.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::llm::LlmProvider;
use crate::models::{Document, QuestionType, ReviewQuestion};
use crate::patterns::{extract_reviewed_date, has_corruption_artifacts, FACT_LINE_REGEX};
use crate::processor::{
    append_review_questions, content_hash, parse_review_queue, prune_stale_questions,
};
use crate::progress::ProgressReporter;
use crate::question_generator::cross_validate::cross_validate_document;
use crate::question_generator::{
    extract_defined_terms, filter_sequential_conflicts, generate_ambiguous_questions_with_type,
    generate_conflict_questions, generate_corruption_questions, generate_duplicate_entry_questions,
    generate_missing_questions,
    generate_required_field_questions, generate_source_quality_questions,
    generate_stale_questions, generate_temporal_questions,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{info, warn};

/// Days within which a reviewed marker suppresses question generation.
const REVIEWED_SKIP_DAYS: i64 = 180;

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
) -> Vec<ReviewQuestion> {
    let mut questions = generate_temporal_questions(body);
    questions.extend(generate_conflict_questions(body));
    if full {
        questions.extend(generate_duplicate_entry_questions(body));
    }
    questions.extend(generate_missing_questions(body));
    if full {
        questions.extend(generate_source_quality_questions(body));
    }
    questions.extend(generate_ambiguous_questions_with_type(body, doc_type, defined_terms));
    questions.extend(generate_stale_questions(body, stale_days));
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
    /// Doc IDs already cross-validated in a previous call (skip them).
    pub checked_doc_ids: HashSet<String>,
    /// Whether to acquire the global write guard before writing results.
    /// Set to `true` in MCP context (concurrent requests), `false` in CLI/tests.
    pub acquire_write_guard: bool,
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
    /// Doc IDs that completed cross-validation (for cursor-based resumption).
    pub checked_doc_ids: Vec<String>,
}

/// Lint all documents: generate review questions, optionally cross-validate, write results.
///
/// Used by both MCP `check_repository` and CLI `cmd_check --review`.
pub async fn check_all_documents(
    docs: &[Document],
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    config: &CheckConfig,
    progress: &ProgressReporter,
) -> Result<CheckOutput, crate::error::FactbaseError> {
    let total = docs.len();
    let rf_ref = &config.required_fields;

    // Filter out archived documents — they're indexed for search/links but not checked
    let active_docs: Vec<_> = docs
        .iter()
        .filter(|d| !is_archived(&d.file_path))
        .collect();
    if active_docs.len() < total {
        progress.log(&format!(
            "Skipping {} archived document(s)",
            total - active_docs.len()
        ));
    }

    // Filter out reference entities — they exist for linking, not quality checks
    let reference_count = active_docs.iter().filter(|d| crate::patterns::is_reference_doc(&d.content)).count();
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

    // Detect documents with corruption artifacts from failed apply_review_answers
    let (clean_docs, corrupted_docs): (Vec<_>, Vec<_>) = active_docs
        .into_iter()
        .partition(|d| !has_corruption_artifacts(&d.content));
    if !corrupted_docs.is_empty() {
        for doc in &corrupted_docs {
            warn!(
                "{} [{}]: skipped — contains corruption artifacts from a failed apply_review_answers run",
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

    // Build title → doc IDs map for duplicate title detection (all docs, not just active)
    let mut title_map: HashMap<String, Vec<(&str, &str)>> = HashMap::new();
    for doc in docs {
        title_map
            .entry(doc.title.to_lowercase())
            .or_default()
            .push((&doc.id, &doc.title));
    }

    let title_map_ref = &title_map;

    // Collect defined terms from definitions/glossary documents so we don't
    // flag acronyms that are already defined in the repo.
    let mut defined_terms = HashSet::new();
    for doc in docs {
        let is_def = doc.doc_type.as_deref().is_some_and(|t| {
            let l = t.to_lowercase();
            l == "definition" || l == "glossary"
        }) || doc.content.lines().take(3).any(|l| {
            let lower = l.to_lowercase();
            lower.contains("# glossary") || lower.contains("# definitions")
        });
        if is_def {
            defined_terms.extend(extract_defined_terms(&doc.content));
        }
    }
    let defined_terms_ref = &defined_terms;

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
                    // apply_review_answers writes reviewed markers to files but doesn't
                    // update the database, so the DB content may lack those markers.
                    let disk_content = std::fs::read_to_string(&doc.file_path).ok();
                    let content = disk_content.as_deref().unwrap_or(&doc.content);

                    // Strip the review queue section so generators never
                    // treat review entries as document facts.
                    let body = crate::patterns::content_body(content);

                    let mut questions = run_generators(body, doc.doc_type.as_deref(), defined_terms_ref, config.stale_days, true);

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

                    // Cross-validation is done in a separate sequential pass below
                    // to avoid overwhelming the Bedrock API with concurrent calls

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

                    // Prune stale unanswered questions from the document
                    let had_deep_check = llm.is_some();
                    let pruned_content = prune_stale_questions(
                        content,
                        &valid_descriptions,
                        had_deep_check,
                    );
                    let pruned_count = existing_unanswered
                        - parse_review_queue(&pruned_content)
                            .unwrap_or_default()
                            .iter()
                            .filter(|q| !q.answered)
                            .count();

                    // Dedup new questions against remaining existing questions
                    let remaining = parse_review_queue(&pruned_content)
                        .unwrap_or_default();
                    let remaining_descs: HashSet<_> = remaining
                        .iter()
                        .map(|q| q.description.clone())
                        .collect();
                    let remaining_conflict_normalized: HashSet<String> = remaining
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
                    let skipped_reviewed = content
                        .lines()
                        .filter(|line| FACT_LINE_REGEX.is_match(line))
                        .filter(|line| {
                            extract_reviewed_date(line)
                                .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
                        })
                        .count();

                    // Count questions suppressed by reviewed markers.
                    // Strip reviewed markers from body and re-generate to measure the delta.
                    let stripped = crate::patterns::strip_reviewed_markers(body);
                    let mut unrestricted = run_generators(&stripped, doc.doc_type.as_deref(), defined_terms_ref, config.stale_days, false);
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

    // Sequential cross-validation pass (one doc at a time to avoid API throttling)
    let mut cross_validated_ids: Vec<String> = config.checked_doc_ids.iter().cloned().collect();
    if llm.is_some() && !deadline_hit {
        progress.phase("Cross-document validation");
        for (i, (doc, questions, _, _, _, _, _, _)) in all_results.iter_mut().enumerate() {
            if let Some(deadline) = config.deadline {
                if Instant::now() > deadline {
                    break;
                }
            }
            // Skip docs already cross-validated in a previous call
            if config.checked_doc_ids.contains(&doc.id) {
                continue;
            }
            if i % 5 == 0 {
                progress.report(i + 1, total_active, "Cross-validating");
            }
            if let Some(llm) = llm {
                match cross_validate_document(&doc.content, &doc.id, doc.doc_type.as_deref(), db, embedding, llm, config.deadline).await {
                    Ok(cross) => {
                        questions.extend(cross);
                        // Only mark as fully checked if deadline wasn't hit mid-document
                        if !config.deadline.is_some_and(|d| Instant::now() > d) {
                            cross_validated_ids.push(doc.id.clone());
                        }
                    }
                    Err(e) => warn!("Cross-validation failed for {}: {e}", doc.id),
                }
            }
        }
    }

    let docs_processed = if llm.is_some() {
        cross_validated_ids.len()
    } else {
        all_results.len()
    };

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
    for (doc, mut questions, pruned_content, pruned_count, existing_unanswered, existing_answered, skipped_reviewed, suppressed_by_review) in all_results {
        // Post-filter: remove conflict questions for boundary-month sequential entries.
        // This catches conflicts from any generator (rule-based or LLM cross-validation).
        let disk_content = std::fs::read_to_string(&doc.file_path).ok();
        let content = disk_content.as_deref().unwrap_or(&doc.content);
        filter_sequential_conflicts(content, &mut questions);

        let count = questions.len();
        let needs_write = count > 0 || pruned_count > 0;
        if needs_write && !config.dry_run {
            let updated = append_review_questions(&pruned_content, &questions);
            let path = PathBuf::from(&doc.file_path);
            if path.exists() {
                std::fs::write(&path, &updated)?;
                let new_hash = content_hash(&updated);
                db.update_document_content(&doc.id, &updated, &new_hash)?;
            }
        }
        if count > 0 {
            info!("{}: {} new questions", doc.title, count);
        }
        if pruned_count > 0 {
            info!("{}: pruned {} stale questions", doc.title, pruned_count);
        }
        // Include docs with any activity
        if count > 0 || pruned_count > 0 || existing_unanswered > 0 || existing_answered > 0 || skipped_reviewed > 0 || suppressed_by_review > 0 {
            results.push(CheckDocResult {
                doc_id: doc.id.clone(),
                doc_title: doc.title.clone(),
                new_questions: count,
                pruned_questions: pruned_count,
                existing_unanswered: existing_unanswered - pruned_count,
                existing_answered,
                skipped_reviewed,
                suppressed_by_review,
            });
        }
    }

    Ok(CheckOutput {
        results,
        docs_processed,
        docs_total: total_active,
        checked_doc_ids: cross_validated_ids,
    })
}

/// Check if a document path is in an archive folder.
/// Matches paths containing `/archive/` or starting with `archive/`.
fn is_archived(file_path: &str) -> bool {
    file_path.contains("/archive/") || file_path.starts_with("archive/")
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap().results;
        assert!(!results.is_empty());
        // The existing question matches what generators would produce, so it's kept
        assert_eq!(results[0].existing_unanswered, 1);
        assert_eq!(results[0].existing_answered, 0);
        assert_eq!(results[0].pruned_questions, 0);
    }

    #[tokio::test]
    async fn test_lint_reports_existing_answered() {
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap().results;
        assert!(!results.is_empty());
        assert_eq!(results[0].existing_answered, 1);
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap().results;
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let results = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap().results;
        assert!(!results.is_empty());
        assert_eq!(results[0].pruned_questions, 1, "Should prune the stale temporal question");
        assert_eq!(results[0].existing_unanswered, 0, "No unanswered after pruning");
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
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
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
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
    async fn test_lint_deadline_returns_checked_doc_ids() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Doc A", "- Fact A\n"),
            make_doc("bbb", "Doc B", "- Fact B\n"),
        ];
        // No deadline, no LLM → checked_doc_ids should be empty (no cross-validation)
        let config = CheckConfig {
                    stale_days: 365,
                    required_fields: None,
                    dry_run: true,
                    concurrency: 1,
                    deadline: None,
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(output.checked_doc_ids.is_empty(), "no LLM means no cross-validation tracking");
    }

    #[tokio::test]
    async fn test_lint_checked_doc_ids_skip_cross_validation() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Doc A", "- Fact A\n"),
            make_doc("bbb", "Doc B", "- Fact B\n"),
        ];
        // Pass aaa as already checked — it should be skipped in cross-validation
        let mut checked = HashSet::new();
        checked.insert("aaa".to_string());
        let config = CheckConfig {
                    stale_days: 365,
                    required_fields: None,
                    dry_run: true,
                    concurrency: 2,
                    deadline: None,
                    checked_doc_ids: checked.clone(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        // Without LLM, cross-validation is skipped entirely, so checked_doc_ids
        // just carries forward the input set
        assert!(output.checked_doc_ids.contains(&"aaa".to_string()));
    }

    #[tokio::test]
    async fn test_check_skips_reference_docs() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let docs = vec![
            make_doc("aaa", "Regular", "- Fact A\n"),
            make_doc("bbb", "Reference", "<!-- factbase:reference -->\n# AWS Lambda\n\n- Serverless compute\n"),
        ];
        let config = CheckConfig {
                    stale_days: 365,
                    required_fields: None,
                    dry_run: true,
                    concurrency: 2,
                    deadline: None,
                    checked_doc_ids: HashSet::new(),
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        // Reference doc should be skipped — only 1 doc processed
        assert_eq!(output.docs_total, 1);
        assert_eq!(output.docs_processed, 1);
    }

    #[tokio::test]
    async fn test_deep_check_docs_processed_reflects_cross_validate_progress() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        // Use fact-free content so cross_validate_document returns Ok immediately
        // (avoids needing sqlite-vec in the test DB)
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nNo facts here.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nJust prose.\n"),
            make_doc("ccc", "Doc C", "# Doc C\n\nMore prose.\n"),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 3,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            acquire_write_guard: false,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 3, "all docs should be cross-validated");
        assert_eq!(output.docs_total, 3);
    }

    #[tokio::test]
    async fn test_deep_check_past_deadline_docs_processed_zero() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nNo facts.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nNo facts.\n"),
        ];
        // Past deadline + LLM → Phase 1 runs 0 docs, Phase 2 skipped
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
            checked_doc_ids: HashSet::new(),
            acquire_write_guard: false,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 0, "past deadline means no cross-validation");
        assert_eq!(output.docs_total, 2);
    }

    #[tokio::test]
    async fn test_deep_check_with_prior_checked_ids_counts_correctly() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nNo facts.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nNo facts.\n"),
        ];
        // aaa already checked from prior call; no deadline → bbb gets cross-validated
        let mut checked = HashSet::new();
        checked.insert("aaa".to_string());
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: None,
            checked_doc_ids: checked,
            acquire_write_guard: false,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        // aaa carried forward + bbb newly validated = 2
        assert_eq!(output.docs_processed, 2, "prior + new cross-validated");
        assert_eq!(output.docs_total, 2);
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
            checked_doc_ids: HashSet::new(),
            acquire_write_guard: true,
        };
        let progress = ProgressReporter::Silent;
        // This should always succeed regardless of guard state
        let result = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await;
        assert!(result.is_ok(), "dry-run should never be blocked by write guard");
    }
}
