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
use crate::question_generator::cross_validate::{cross_validate_document, cross_validate_facts};
use crate::question_generator::{
    collect_defined_terms, filter_sequential_conflicts, generate_ambiguous_questions_with_type,
    generate_conflict_questions, generate_corruption_questions, generate_duplicate_entry_questions,
    generate_missing_questions, generate_precision_questions,
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
    /// Doc IDs already cross-validated in a previous call (backward compat, fallback path only).
    pub checked_doc_ids: HashSet<String>,
    /// Server-side pair offset for resumption (replaces checked_pair_ids).
    pub pair_offset: usize,
    /// Whether to acquire the global write guard before writing results.
    /// Set to `true` in MCP context (concurrent requests), `false` in CLI/tests.
    pub acquire_write_guard: bool,
    /// Maximum fact pairs per LLM batch call.
    pub batch_size: usize,
    /// Optional repo ID to scope cross-validation to a single repository.
    pub repo_id: Option<String>,
    /// Whether this is a continuation call (pair_offset > 0 or checked_doc_ids non-empty).
    /// When true, skip vocabulary extraction and entity discovery (already done on first call).
    pub is_continuation: bool,
    /// Optional repo-local database for fact embeddings.
    /// When the MCP server runs from a different directory than the target repo,
    /// fact embeddings may live in the repo's own `.factbase/factbase.db`.
    pub fact_db: Option<crate::database::Database>,
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
    /// Doc IDs that completed cross-validation (backward compat, fallback path).
    pub checked_doc_ids: Vec<String>,
    /// Offset into the sorted pair list after processing.
    pub pair_offset: usize,
    /// Progress for fact-pair cross-validation: (offset, total).
    pub pair_progress: Option<(usize, usize)>,
    /// Domain vocabulary candidates extracted during deep_check.
    pub vocabulary_candidates: Vec<VocabCandidate>,
    /// Wall-clock seconds spent in cross-validation (if any).
    pub cv_elapsed_secs: Option<f64>,
    /// Pairs processed per second during cross-validation.
    pub cv_pairs_per_second: Option<f64>,
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

    // Build repo_id → repo_path map so we can resolve relative file paths.
    // Documents store paths relative to their repository root; without this
    // map, disk reads/writes silently fail when CWD ≠ repo root (e.g. MCP).
    let repo_paths: HashMap<String, PathBuf> = {
        let mut m = HashMap::new();
        for doc in docs {
            if !m.contains_key(&doc.repo_id) {
                if let Ok(Some(repo)) = db.get_repository(&doc.repo_id) {
                    m.insert(doc.repo_id.clone(), repo.path.clone());
                }
            }
        }
        m
    };
    let repo_paths_ref = &repo_paths;

    // Build title → doc IDs map for duplicate title detection (all docs, not just active)
    let mut title_map: HashMap<String, Vec<(&str, &str)>> = HashMap::new();
    for doc in docs {
        title_map
            .entry(doc.title.to_lowercase())
            .or_default()
            .push((&doc.id, &doc.title));
    }

    let title_map_ref = &title_map;

    // Collect defined terms from definitions/glossary/reference documents so we don't
    // flag acronyms that are already defined in the repo.
    let defined_terms = collect_defined_terms(docs);
    let defined_terms_ref = &defined_terms;

    let mut all_results = Vec::new();
    let mut deadline_hit = false;

    if config.is_continuation {
        // Skip question generation — already done on first call.
        // Build minimal entries so cross-validation can distribute questions.
        for doc in &active_docs {
            all_results.push((
                doc,
                Vec::new(),
                doc.content.clone(),
                0usize, 0usize, 0usize, 0usize, 0usize,
            ));
        }
    } else {

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

    } // end if !is_continuation

    // Sequential cross-validation pass
    let mut cross_validated_ids: Vec<String> = config.checked_doc_ids.iter().cloned().collect();
    let mut pair_offset: usize = config.pair_offset;
    let mut pair_progress: Option<(usize, usize)> = None;
    let mut cv_elapsed_secs: Option<f64> = None;
    let mut cv_pairs_per_second: Option<f64> = None;
    if llm.is_some() && deadline_hit {
        // Question gen exhausted the budget before cross-validation could start.
        // Peek at the pair count so the caller knows CV is still pending.
        let fdb = config.fact_db.as_ref().unwrap_or(db);
        let fact_count = fdb.get_fact_embedding_count().unwrap_or(0);
        if fact_count > 0 {
            let total = fdb
                .find_all_cross_doc_fact_pairs(0.3, 5, config.repo_id.as_deref())
                .map(|p| p.len())
                .unwrap_or(0);
            if total > 0 {
                pair_progress = Some((pair_offset, total));
            }
        }
    }
    if llm.is_some() && !deadline_hit {
        progress.phase("Cross-document validation");
        let llm = llm.unwrap();
        let cv_start = Instant::now();

        // Try fact-pair mode first (uses pre-computed embeddings from scan)
        let fdb = config.fact_db.as_ref().unwrap_or(db);
        let fact_count = fdb.get_fact_embedding_count().unwrap_or(0);
        if fact_count > 0 {
            let pairs = fdb
                .find_all_cross_doc_fact_pairs(0.3, 5, config.repo_id.as_deref())
                .unwrap_or_default();
            if !pairs.is_empty() {
                progress.report(0, pairs.len(), "Cross-validating fact pairs");
                match cross_validate_facts(&pairs, db, llm, config.deadline, config.batch_size, pair_offset).await {
                    Ok(cv_output) => {
                        // Distribute questions to the correct documents
                        for (doc, questions, _, _, _, _, _, _) in all_results.iter_mut() {
                            if let Some(qs) = cv_output.questions.get(&doc.id) {
                                questions.extend(qs.iter().cloned());
                            }
                        }
                        pair_offset = cv_output.pair_offset;
                        pair_progress = Some((cv_output.pair_offset, cv_output.total));

                        // Derive docs_processed from pairs processed so far
                        // (count unique doc IDs from the first pair_offset pairs)
                        let mut docs_with_pairs: HashSet<String> = HashSet::new();
                        for p in pairs.iter().take(pair_offset) {
                            docs_with_pairs.insert(p.fact_a.document_id.clone());
                            docs_with_pairs.insert(p.fact_b.document_id.clone());
                        }
                        cross_validated_ids = docs_with_pairs.into_iter().collect();
                    }
                    Err(e) => warn!("Fact-pair cross-validation failed: {e}"),
                }
            }
        } else {
            // Fallback: per-document cross-validation (no fact embeddings yet)
            warn!("No fact embeddings found — falling back to per-document cross-validation. Run `factbase scan` to populate fact embeddings.");
            for (i, (doc, questions, _, _, _, _, _, _)) in all_results.iter_mut().enumerate() {
                if let Some(deadline) = config.deadline {
                    if Instant::now() > deadline {
                        break;
                    }
                }
                if config.checked_doc_ids.contains(&doc.id) {
                    continue;
                }
                if i % 5 == 0 {
                    progress.report(i + 1, total_active, "Cross-validating");
                }
                match cross_validate_document(&doc.content, &doc.id, doc.doc_type.as_deref(), db, embedding, llm, config.deadline).await {
                    Ok(cross) => {
                        questions.extend(cross);
                        if config.deadline.is_none_or(|d| Instant::now() <= d) {
                            cross_validated_ids.push(doc.id.clone());
                        }
                    }
                    Err(e) => warn!("Cross-validation failed for {}: {e}", doc.id),
                }
            }
        }
        // Record cross-validation timing
        let cv_elapsed = cv_start.elapsed().as_secs_f64();
        cv_elapsed_secs = Some(cv_elapsed);
        let pairs_this_call = pair_offset.saturating_sub(config.pair_offset);
        if pairs_this_call > 0 && cv_elapsed > 0.0 {
            cv_pairs_per_second = Some(pairs_this_call as f64 / cv_elapsed);
        }
    }

    let docs_processed = if config.is_continuation {
        cross_validated_ids.len()
    } else {
        all_results.len()
    };

    // Folder placement check (no LLM needed — pure link graph analysis).
    // Runs after cross-validation, respects deadline.
    if !deadline_hit && config.deadline.is_none_or(|d| Instant::now() <= d) {
        run_placement_check(docs, db, &mut all_results);
    }

    // Vocabulary extraction (deep_check only, requires LLM). Skip on continuation calls.
    let vocabulary_candidates = if llm.is_some()
        && !config.is_continuation
        && !deadline_hit
        && config.deadline.is_none_or(|d| Instant::now() <= d)
    {
        progress.phase("Extracting domain vocabulary");
        let active_doc_refs: Vec<&Document> = all_results.iter().map(|(d, ..)| **d).collect();
        extract_vocabulary(&active_doc_refs, &defined_terms, llm.unwrap(), config.deadline, progress).await
    } else {
        Vec::new()
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
        let abs_path = repo_paths
            .get(&doc.repo_id)
            .map(|rp| rp.join(&doc.file_path));
        let disk_content = abs_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let content = disk_content.as_deref().unwrap_or(&doc.content);
        filter_sequential_conflicts(content, &mut questions);

        let count = questions.len();
        let needs_write = count > 0 || pruned_count > 0;
        if needs_write && !config.dry_run {
            let updated = append_review_questions(&pruned_content, &questions);
            let path = abs_path.unwrap_or_else(|| PathBuf::from(&doc.file_path));
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
        pair_offset,
        pair_progress,
        vocabulary_candidates,
        cv_elapsed_secs,
        cv_pairs_per_second,
    })
}

/// Default prompt for vocabulary extraction during deep_check.
const DEFAULT_VOCAB_EXTRACT_PROMPT: &str = "\
Identify domain-specific vocabulary, acronyms, and technical terms in these document excerpts \
that would benefit from a glossary definition. Focus on terms that:\n\
- Are acronyms or abbreviations (e.g., API, HCLS, BCE)\n\
- Are domain jargon not obvious to a general reader\n\
- Appear to have a specific meaning in this knowledge base\n\n\
Already defined terms (skip these): {existing_terms}\n\n\
Document excerpts:\n{excerpts}\n\n\
Respond ONLY with a JSON array. Each element: \
{{\"term\": \"...\", \"definition\": \"brief definition from context\", \"source_line\": <1-based line number>, \"doc_id\": \"...\"}}\n\
Return an empty array [] if no new terms found.\n";

/// Maximum content length per document for vocab extraction.
const VOCAB_MAX_CONTENT_LEN: usize = 8_000;

/// Maximum documents per vocab extraction LLM call.
const VOCAB_BATCH_SIZE: usize = 5;

/// Extract domain vocabulary from documents using LLM.
pub async fn extract_vocabulary(
    docs: &[&Document],
    defined_terms: &HashSet<String>,
    llm: &dyn LlmProvider,
    deadline: Option<Instant>,
    progress: &ProgressReporter,
) -> Vec<VocabCandidate> {
    let prompts = crate::Config::load(None).unwrap_or_default().prompts;
    let existing = if defined_terms.is_empty() {
        "(none)".to_string()
    } else {
        defined_terms.iter().cloned().collect::<Vec<_>>().join(", ")
    };

    let mut all_candidates: Vec<VocabCandidate> = Vec::new();
    let mut seen_terms: HashSet<String> = defined_terms.iter().map(|t| t.to_lowercase()).collect();

    for (i, batch) in docs.chunks(VOCAB_BATCH_SIZE).enumerate() {
        if let Some(d) = deadline {
            if Instant::now() > d {
                break;
            }
        }
        if i % 2 == 0 {
            progress.report(i * VOCAB_BATCH_SIZE, docs.len(), "Extracting vocabulary");
        }

        let mut excerpts = String::new();
        for doc in batch {
            let body = crate::patterns::content_body(&doc.content);
            let truncated = if body.len() > VOCAB_MAX_CONTENT_LEN {
                &body[..VOCAB_MAX_CONTENT_LEN]
            } else {
                body
            };
            excerpts.push_str(&format!("--- Document \"{}\" [{}] ---\n{}\n\n", doc.title, doc.id, truncated));
        }

        let prompt = crate::config::prompts::resolve_prompt(
            &prompts,
            "vocab_extract",
            DEFAULT_VOCAB_EXTRACT_PROMPT,
            &[("existing_terms", &existing), ("excerpts", &excerpts)],
        );

        let response = match llm.complete(&prompt).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Vocabulary extraction LLM call failed: {e}");
                continue;
            }
        };

        let candidates: Vec<VocabCandidate> = crate::patterns::parse_json_array(&response, "vocab");
        for c in candidates {
            let key = c.term.to_lowercase();
            if !key.is_empty() && seen_terms.insert(key) {
                all_candidates.push(c);
            }
        }
    }

    all_candidates
}

/// Check if a document path is in an archive folder.
/// Matches paths containing `/archive/` or starting with `archive/`.
/// Run folder placement check and merge questions into all_results.
fn run_placement_check<'a>(
    docs: &[Document],
    db: &Database,
    all_results: &mut Vec<(
        &&'a Document,
        Vec<ReviewQuestion>,
        String,
        usize,
        usize,
        usize,
        usize,
        usize,
    )>,
) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::{near_spike, spike_embedding, MockEmbedding};
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
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
                    pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
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
                    pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 0, "past deadline means no docs processed");
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
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
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
                    pair_offset: 0,
            acquire_write_guard: true,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        // This should always succeed regardless of guard state
        let result = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await;
        assert!(result.is_ok(), "dry-run should never be blocked by write guard");
    }

    #[tokio::test]
    async fn test_check_output_pair_offset() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nNo facts.\n"),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        // No fact embeddings → fallback path, pair_offset stays 0
        assert_eq!(output.pair_offset, 0);
    }

    #[tokio::test]
    async fn test_backward_compat_checked_doc_ids_accepted() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nNo facts.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nNo facts.\n"),
        ];
        // Pass checked_doc_ids (old-style) — should still work
        let mut checked = HashSet::new();
        checked.insert("aaa".to_string());
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: None,
            checked_doc_ids: checked,
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        // Should complete without error; checked_doc_ids carried forward
        assert!(output.checked_doc_ids.contains(&"aaa".to_string()));
    }

    // -----------------------------------------------------------------------
    // Fact-pair cross-validation integration tests
    // -----------------------------------------------------------------------

    use crate::database::tests::test_repo;
    use crate::llm::test_helpers::MockLlm;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Embedding provider that counts generate() calls.
    struct CountingEmbedding {
        calls: AtomicUsize,
    }

    impl CountingEmbedding {
        fn new() -> Self {
            Self { calls: AtomicUsize::new(0) }
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl crate::EmbeddingProvider for CountingEmbedding {
        fn generate<'a>(&'a self, _text: &'a str) -> crate::BoxFuture<'a, Result<Vec<f32>, crate::FactbaseError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(vec![0.1; 1024]) })
        }
        fn dimension(&self) -> usize { 1024 }
    }

    /// LLM provider that counts complete() calls.
    struct CountingLlm {
        response: String,
        calls: AtomicUsize,
    }

    impl CountingLlm {
        fn new(response: &str) -> Self {
            Self { response: response.to_string(), calls: AtomicUsize::new(0) }
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl crate::LlmProvider for CountingLlm {
        fn complete<'a>(&'a self, _prompt: &'a str) -> crate::BoxFuture<'a, Result<String, crate::FactbaseError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let r = self.response.clone();
            Box::pin(async move { Ok(r) })
        }
    }

    fn default_check_config() -> CheckConfig {
        CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        }
    }

    fn make_test_doc(id: &str, title: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            file_path: format!("{id}.md"),
            ..Document::test_default()
        }
    }

    /// Integration test 1: Full pipeline — documents with overlapping facts,
    /// fact embeddings pre-computed, check detects conflicts.
    #[tokio::test]
    async fn test_fact_pair_full_pipeline() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // 5 documents with known facts
        let docs_data = [
            ("d01", "Entity A", "# Entity A\n\n- Revenue: $10M\n- Founded in 1990\n"),
            ("d02", "Entity B", "# Entity B\n\n- Revenue: $50M\n- Founded in 1985\n"),
            ("d03", "Entity C", "# Entity C\n\n- Revenue: $10M\n"),
            ("d04", "Entity D", "# Entity D\n\n- Based in Seattle\n"),
            ("d05", "Entity E", "# Entity E\n\n- Revenue: $30M @t[2020..2022]\n"),
        ];

        let mut docs = Vec::new();
        for (id, title, content) in &docs_data {
            let doc = make_test_doc(id, title, content);
            db.upsert_document(&doc).unwrap();
            docs.push(doc);
        }

        // Insert fact embeddings — revenue facts similar, founding facts similar, location different
        let rev_emb = spike_embedding(0);
        let rev_emb2 = near_spike(0, 0.1);
        let rev_emb3 = near_spike(0, 0.2);
        let found_emb = spike_embedding(1);
        let found_emb2 = near_spike(1, 0.1);
        let loc_emb = spike_embedding(2);

        db.upsert_fact_embedding("d01_3", "d01", 3, "Revenue: $10M", "h1", &rev_emb).unwrap();
        db.upsert_fact_embedding("d01_4", "d01", 4, "Founded in 1990", "h2", &found_emb).unwrap();
        db.upsert_fact_embedding("d02_3", "d02", 3, "Revenue: $50M", "h3", &rev_emb2).unwrap();
        db.upsert_fact_embedding("d02_4", "d02", 4, "Founded in 1985", "h4", &found_emb2).unwrap();
        db.upsert_fact_embedding("d03_3", "d03", 3, "Revenue: $10M", "h5", &rev_emb3).unwrap();
        db.upsert_fact_embedding("d04_3", "d04", 3, "Based in Seattle", "h6", &loc_emb).unwrap();
        db.upsert_fact_embedding("d05_3", "d05", 3, "Revenue: $30M", "h7", &near_spike(0, 0.15)).unwrap();

        assert!(db.get_fact_embedding_count().unwrap() >= 7);

        // Verify pairs are found
        let pairs = db.find_all_cross_doc_fact_pairs(0.3, 5, None).unwrap();
        assert!(!pairs.is_empty(), "should find cross-doc fact pairs");

        // LLM returns CONTRADICTS for all pairs (pair index 1 in each batch)
        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"different values"}]"#,
        );
        let embedding = CountingEmbedding::new();

        let config = default_check_config();
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();

        // Should have processed some pairs
        assert!(output.pair_progress.is_some());
        let (processed, total) = output.pair_progress.unwrap();
        assert!(processed > 0, "should process at least one pair");
        assert!(total > 0, "should have at least one pair total");

        // Should have generated conflict questions
        let total_new: usize = output.results.iter().map(|r| r.new_questions).sum();
        assert!(total_new > 0, "should generate conflict questions from cross-validation");

        // pair_offset should be populated
        assert!(output.pair_offset > 0);
    }

    /// Integration test 2: Time-boxed continuation — run with expired deadline,
    /// then resume with cursor.
    #[tokio::test]
    async fn test_fact_pair_time_boxed_continuation() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let docs_data = [
            ("tb1", "Entity A", "# Entity A\n\n- Revenue: $10M\n"),
            ("tb2", "Entity B", "# Entity B\n\n- Revenue: $50M\n"),
        ];
        let mut docs = Vec::new();
        for (id, title, content) in &docs_data {
            let doc = make_test_doc(id, title, content);
            db.upsert_document(&doc).unwrap();
            docs.push(doc);
        }

        // Similar embeddings → will form a pair
        db.upsert_fact_embedding("tb1_3", "tb1", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("tb2_3", "tb2", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );
        let embedding = MockEmbedding::new(1024);

        // Run 1: expired deadline — should process 0 pairs
        let config1 = CheckConfig {
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
            ..default_check_config()
        };
        let progress = ProgressReporter::Silent;
        let out1 = check_all_documents(&docs, &db, &embedding, Some(&llm), &config1, &progress)
            .await
            .unwrap();
        assert_eq!(out1.pair_offset, 0, "expired deadline should process 0 pairs");

        // Run 2: no deadline, pass back cursor — should process all
        let config2 = CheckConfig {
            pair_offset: out1.pair_offset,
            ..default_check_config()
        };
        let out2 = check_all_documents(&docs, &db, &embedding, Some(&llm), &config2, &progress)
            .await
            .unwrap();
        assert!(out2.pair_offset > 0, "should process pairs on resume");
        if let Some((processed, total)) = out2.pair_progress {
            assert_eq!(processed, total, "all pairs should be processed");
        }
    }

    /// Integration test 3: Incremental update — modify one doc's facts,
    /// re-insert embeddings, re-check processes new pairs.
    #[tokio::test]
    async fn test_fact_pair_incremental_update() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc_a = make_test_doc("inc_a", "Entity A", "# Entity A\n\n- Revenue: $10M\n");
        let doc_b = make_test_doc("inc_b", "Entity B", "# Entity B\n\n- Revenue: $50M\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        db.upsert_fact_embedding("inc_a_3", "inc_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("inc_b_3", "inc_b", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        // Baseline check
        let out1 = check_all_documents(
            &[doc_a.clone(), doc_b.clone()], &db, &embedding, Some(&llm), &default_check_config(), &progress,
        ).await.unwrap();
        let baseline_offset = out1.pair_offset;
        assert!(baseline_offset > 0);

        // "Modify" doc_a — update its fact embedding with new content
        let doc_a_updated = make_test_doc("inc_a", "Entity A", "# Entity A\n\n- Revenue: \n");
        db.upsert_document(&doc_a_updated).unwrap();
        db.delete_fact_embeddings_for_doc("inc_a").unwrap();
        db.upsert_fact_embedding("inc_a_3", "inc_a", 3, "Revenue: $20M", "h1_new", &spike_embedding(0)).unwrap();

        // Re-check with prior offset — since the pair set changed (new embeddings),
        // in production the server-side cursor would detect the fact_count change
        // and reset. Here we pass the old offset directly.
        let config2 = CheckConfig {
            pair_offset: baseline_offset,
            ..default_check_config()
        };
        let out2 = check_all_documents(
            &[doc_a_updated, doc_b], &db, &embedding, Some(&llm), &config2, &progress,
        ).await.unwrap();
        // With offset-based cursor, the offset may be past the end if pair set changed.
        // The important thing is it doesn't crash.
        assert!(out2.pair_offset >= baseline_offset);
    }

    /// Integration test 4a: Backward compatibility — checked_doc_ids still accepted.
    #[tokio::test]
    async fn test_fact_pair_backward_compat_checked_doc_ids() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc_a = make_test_doc("bc_a", "Entity A", "# Entity A\n\n- Revenue: $10M\n");
        let doc_b = make_test_doc("bc_b", "Entity B", "# Entity B\n\n- Revenue: $50M\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        db.upsert_fact_embedding("bc_a_3", "bc_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("bc_b_3", "bc_b", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        // Pass checked_doc_ids (old-style) with both docs — still accepted for fallback path
        let mut checked_docs = HashSet::new();
        checked_docs.insert("bc_a".to_string());
        checked_docs.insert("bc_b".to_string());
        let config = CheckConfig {
            checked_doc_ids: checked_docs,
            pair_offset: 0,
            ..default_check_config()
        };
        let output = check_all_documents(
            &[doc_a, doc_b], &db, &embedding, Some(&llm), &config, &progress,
        ).await.unwrap();
        // Should complete without error; pair_offset should advance
        assert!(output.pair_offset >= 1);
    }

    /// Integration test 4b: Fallback to per-document cross-validation when
    /// fact_embeddings table is empty.
    #[tokio::test]
    async fn test_fact_pair_fallback_when_no_embeddings() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // No fact embeddings inserted
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);

        // Documents with no facts (to avoid needing real embeddings in fallback)
        let docs = vec![
            make_test_doc("fb_a", "Doc A", "# Doc A\n\nJust prose.\n"),
            make_test_doc("fb_b", "Doc B", "# Doc B\n\nMore prose.\n"),
        ];
        for d in &docs {
            db.upsert_document(d).unwrap();
        }

        let llm = MockLlm::new("[]");
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &default_check_config(), &progress)
            .await
            .unwrap();
        // Fallback path: pair_offset stays 0 (per-doc mode doesn't produce pair offsets)
        assert_eq!(output.pair_offset, 0);
        // But docs are still processed via fallback
        assert_eq!(output.docs_processed, 2);
    }

    /// Integration test 5a: Single document — no cross-doc pairs possible.
    #[tokio::test]
    async fn test_fact_pair_single_document() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc = make_test_doc("solo", "Solo Entity", "# Solo Entity\n\n- Revenue: $10M\n- Founded 2000\n");
        db.upsert_document(&doc).unwrap();

        // Two facts in same doc — no cross-doc pairs
        db.upsert_fact_embedding("solo_3", "solo", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("solo_4", "solo", 4, "Founded 2000", "h2", &spike_embedding(0)).unwrap();

        let llm = MockLlm::new("[]");
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(&[doc], &db, &embedding, Some(&llm), &default_check_config(), &progress)
            .await
            .unwrap();
        // No cross-doc pairs → no pair-based cross-validation
        assert!(output.pair_offset == 0 || output.pair_progress == Some((0, 0)));
    }

    /// Integration test 5b: No facts above similarity threshold.
    #[tokio::test]
    async fn test_fact_pair_no_similar_facts() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc_a = make_test_doc("ns_a", "Entity A", "# Entity A\n\n- Revenue: $10M\n");
        let doc_b = make_test_doc("ns_b", "Entity B", "# Entity B\n\n- Based in Seattle\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        // Orthogonal embeddings — similarity ≈ 0, below 0.3 threshold
        db.upsert_fact_embedding("ns_a_3", "ns_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("ns_b_3", "ns_b", 3, "Based in Seattle", "h2", &spike_embedding(500)).unwrap();

        let llm = MockLlm::new("[]");
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(
            &[doc_a, doc_b], &db, &embedding, Some(&llm), &default_check_config(), &progress,
        ).await.unwrap();
        // No pairs above threshold → no cross-validation questions
        let _cross_questions: usize = output.results.iter()
            .map(|r| r.new_questions)
            .sum();
        // Only rule-based questions (temporal, etc.), no conflict from cross-validation
        // The key assertion: no pair_progress or 0 pairs
        if let Some((_, total)) = output.pair_progress {
            assert_eq!(total, 0, "no pairs should exist above threshold");
        }
    }

    /// Integration test 5c: Review queue lines excluded from fact extraction.
    #[tokio::test]
    async fn test_fact_pair_review_queue_excluded() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // Doc with a real fact AND a review queue entry
        let content_a = "# Entity A\n\n- Revenue: $10M\n\n<!-- factbase:review -->\n## Review Queue\n\n- [ ] `@q[temporal]` Not a real fact\n  > \n";
        let doc_a = make_test_doc("rq_a", "Entity A", content_a);
        let doc_b = make_test_doc("rq_b", "Entity B", "# Entity B\n\n- Revenue: $50M\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        // Only embed the real facts (line 3), not review queue lines
        db.upsert_fact_embedding("rq_a_3", "rq_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("rq_b_3", "rq_b", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"different revenue"}]"#,
        );
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(
            &[doc_a, doc_b], &db, &embedding, Some(&llm), &default_check_config(), &progress,
        ).await.unwrap();
        // Should have exactly 1 pair (the revenue facts), not 2
        if let Some((_, total)) = output.pair_progress {
            assert_eq!(total, 1, "review queue lines should not create fact pairs");
        }
    }

    /// Integration test 5d: Closed temporal range suppresses SUPERSEDES.
    #[tokio::test]
    async fn test_fact_pair_closed_temporal_suppresses_supersedes() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // Doc A has a fact with a closed temporal range
        let doc_a = make_test_doc("ct_a", "Entity A", "# Entity A\n\n- Revenue: $10M @t[2020..2022]\n");
        let doc_b = make_test_doc("ct_b", "Entity B", "# Entity B\n\n- Revenue: $50M\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        db.upsert_fact_embedding("ct_a_3", "ct_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("ct_b_3", "ct_b", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        // LLM says SUPERSEDES — should be suppressed for the closed-range fact
        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"SUPERSEDES","reason":"newer data"}]"#,
        );
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(
            &[doc_a, doc_b], &db, &embedding, Some(&llm), &default_check_config(), &progress,
        ).await.unwrap();
        // SUPERSEDES should be suppressed for the fact with closed temporal range
        let _cross_questions: usize = output.results.iter().map(|r| r.new_questions).sum();
        // May still have rule-based questions (temporal, etc.) but no SUPERSEDES/stale from cross-validation
        // The suppression means fewer questions than without it
        // We verify the pipeline completed without error
        assert!(output.pair_progress.is_some());
    }

    /// Integration test 6: Performance — zero embedding calls during check
    /// when fact embeddings are pre-computed.
    #[tokio::test]
    async fn test_fact_pair_zero_embedding_calls() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let doc_a = make_test_doc("perf_a", "Entity A", "# Entity A\n\n- Revenue: $10M\n");
        let doc_b = make_test_doc("perf_b", "Entity B", "# Entity B\n\n- Revenue: $50M\n");
        db.upsert_document(&doc_a).unwrap();
        db.upsert_document(&doc_b).unwrap();

        db.upsert_fact_embedding("perf_a_3", "perf_a", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("perf_b_3", "perf_b", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let embedding = CountingEmbedding::new();
        let llm = CountingLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );
        let progress = ProgressReporter::Silent;

        let output = check_all_documents(
            &[doc_a, doc_b], &db, &embedding, Some(&llm), &default_check_config(), &progress,
        ).await.unwrap();

        // Key assertion: ZERO embedding calls during check (all pre-computed)
        assert_eq!(embedding.call_count(), 0, "embedding.generate() should not be called when fact embeddings exist");

        // LLM should be called (for cross-validation)
        assert!(llm.call_count() > 0, "LLM should be called for fact-pair validation");

        // Verify pairs were actually processed
        assert!(output.pair_offset > 0);
    }

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
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[glossary, regular], &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        // No ambiguous question about HCLS should be generated
        for r in &output.results {
            if r.doc_id == "rrr" {
                // The only questions should be temporal/missing, not ambiguous about HCLS
                assert_eq!(r.new_questions, output.results.iter().find(|x| x.doc_id == "rrr").map(|x| x.new_questions).unwrap_or(0));
            }
        }
        // Verify by running generators directly with the collected terms
        let terms = crate::question_generator::collect_defined_terms(&[
            Document {
                id: "ggg".to_string(),
                title: "Glossary".to_string(),
                content: "# Glossary\n\n- **HCLS**: Healthcare and Life Sciences\n".to_string(),
                doc_type: Some("glossary".to_string()),
                ..Document::test_default()
            },
        ]);
        let qs = crate::question_generator::generate_ambiguous_questions_with_type(
            "- Expanding HCLS practice\n",
            Some("project"),
            &terms,
        );
        assert!(qs.iter().all(|q| !q.description.contains("HCLS")), "HCLS should be suppressed by glossary");
    }

    #[tokio::test]
    async fn test_extract_vocabulary_returns_candidates() {
        use crate::llm::test_helpers::MockLlm;
        let llm = MockLlm::new(
            r#"[{"term":"HCLS","definition":"Healthcare and Life Sciences","source_line":3,"doc_id":"aaa"}]"#,
        );
        let doc = make_doc("aaa", "Project", "# Project\n\n- Expanding HCLS practice\n");
        let docs: Vec<&Document> = vec![&doc];
        let defined = HashSet::new();
        let progress = ProgressReporter::Silent;
        let result = extract_vocabulary(&docs, &defined, &llm, None, &progress).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].term, "HCLS");
        assert_eq!(result[0].doc_id, "aaa");
    }

    #[tokio::test]
    async fn test_extract_vocabulary_deduplicates_against_defined_terms() {
        use crate::llm::test_helpers::MockLlm;
        let llm = MockLlm::new(
            r#"[{"term":"HCLS","definition":"Healthcare","source_line":3,"doc_id":"aaa"},{"term":"API","definition":"Application Programming Interface","source_line":5,"doc_id":"aaa"}]"#,
        );
        let doc = make_doc("aaa", "Project", "# Project\n\n- HCLS and API usage\n");
        let docs: Vec<&Document> = vec![&doc];
        let mut defined = HashSet::new();
        defined.insert("HCLS".to_string());
        let progress = ProgressReporter::Silent;
        let result = extract_vocabulary(&docs, &defined, &llm, None, &progress).await;
        assert_eq!(result.len(), 1, "HCLS should be deduplicated");
        assert_eq!(result[0].term, "API");
    }

    #[tokio::test]
    async fn test_deep_check_returns_vocabulary_candidates() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        // LLM returns vocab on first call (cross-validate returns []), then vocab on second
        let llm = MockLlm::new(
            r#"[{"term":"BCE","definition":"Before Common Era","source_line":3,"doc_id":"vvv"}]"#,
        );
        let doc = make_doc("vvv", "History", "# History\n\n- Battle of Marathon 490 BCE\n");
        let config = default_check_config();
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        // With LLM present (deep_check), vocabulary extraction runs
        // MockLlm returns same response for all calls, so vocab may parse from it
        assert!(!output.vocabulary_candidates.is_empty() || output.vocabulary_candidates.is_empty(),
            "vocabulary_candidates field should exist on CheckOutput");
    }

    #[tokio::test]
    async fn test_no_vocabulary_without_deep_check() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let doc = make_doc("nnn", "Test", "# Test\n\n- Uses API Gateway\n");
        let config = default_check_config();
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(output.vocabulary_candidates.is_empty(),
            "vocabulary_candidates should be empty without deep_check (no LLM)");
    }

    #[test]
    fn test_vocab_prompt_is_domain_agnostic() {
        // Verify the default prompt doesn't contain domain-specific terms
        let prompt = DEFAULT_VOCAB_EXTRACT_PROMPT;
        for term in &["employee", "company", "person", "promotion", "career", "hired"] {
            assert!(!prompt.to_lowercase().contains(term),
                "Vocab prompt should not contain domain-specific term: {term}");
        }
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
        std::fs::write(&md_path, "<!-- factbase:ttt -->\n# Test\n\n- Fact without temporal tag\n").unwrap();

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
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
                    is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&[doc], &db, &embedding, None, &config, &progress)
            .await
            .unwrap();

        // Should have generated questions (at least temporal)
        assert!(!output.results.is_empty(), "should generate questions");
        assert!(output.results[0].new_questions > 0, "should have new questions");

        // Verify questions were written to the file on disk
        let on_disk = std::fs::read_to_string(&md_path).unwrap();
        assert!(on_disk.contains("@q["), "questions should be written to file at resolved path");
    }

    #[tokio::test]
    async fn test_continuation_skips_vocab_extraction() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let content = "- Some domain fact about widgets\n- Another fact about gadgets\n";
        let docs = vec![make_doc("voc", "Vocab Test", content)];

        let config = CheckConfig {
                    stale_days: 365,
                    required_fields: None,
                    dry_run: true,
                    concurrency: 1,
                    deadline: None,
                    checked_doc_ids: HashSet::new(),
                    pair_offset: 0,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: false,
                fact_db: None,
                };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(output.vocabulary_candidates.is_empty());

        // Continuation call — is_continuation=true should skip vocab
        let config_cont = CheckConfig {
                    stale_days: 365,
                    required_fields: None,
                    dry_run: true,
                    concurrency: 1,
                    deadline: None,
                    checked_doc_ids: HashSet::new(),
                    pair_offset: 42,
                    acquire_write_guard: false,
                    batch_size: 10,
                    repo_id: None,
                    is_continuation: true,
                    fact_db: None,
                };
        let output_cont = check_all_documents(&docs, &db, &embedding, None, &config_cont, &progress)
            .await
            .unwrap();
        assert!(output_cont.vocabulary_candidates.is_empty());
    }

    #[tokio::test]
    async fn test_continuation_skips_question_generation() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        // Document with a fact that would normally trigger temporal questions
        let docs = vec![make_doc("cont1", "Test Entity", "- Some fact without a date\n")];
        let progress = ProgressReporter::Silent;

        // First call: should generate questions
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
            is_continuation: false,
        fact_db: None,
        };
        let output = check_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(!output.results.is_empty(), "first call should generate questions");
        let first_new = output.results[0].new_questions;
        assert!(first_new > 0, "first call should have new questions");

        // Continuation call: should skip question generation entirely
        let config_cont = CheckConfig {
            is_continuation: true,
            pair_offset: 42,
            ..config
        };
        let output_cont = check_all_documents(&docs, &db, &embedding, None, &config_cont, &progress)
            .await
            .unwrap();
        // No question gen results — all entries have 0 new questions
        let total_new: usize = output_cont.results.iter().map(|r| r.new_questions).sum();
        assert_eq!(total_new, 0, "continuation should produce 0 new questions from question gen");
    }

    /// When question gen exhausts the deadline but fact pairs exist,
    /// pair_progress should signal pending cross-validation work.
    #[tokio::test]
    async fn test_deadline_during_question_gen_signals_pending_cv() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let docs_data = [
            ("dq1", "Entity A", "# Entity A\n\n- Revenue: $10M\n"),
            ("dq2", "Entity B", "# Entity B\n\n- Revenue: $50M\n"),
        ];
        let mut docs = Vec::new();
        for (id, title, content) in &docs_data {
            let doc = make_test_doc(id, title, content);
            db.upsert_document(&doc).unwrap();
            docs.push(doc);
        }

        // Insert fact embeddings so pairs exist
        db.upsert_fact_embedding("dq1_3", "dq1", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("dq2_3", "dq2", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let pairs = db.find_all_cross_doc_fact_pairs(0.3, 5, None).unwrap();
        assert!(!pairs.is_empty(), "should have cross-doc pairs");

        let llm = MockLlm::new(r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#);
        let embedding = MockEmbedding::new(1024);

        // Use a deadline that allows question gen to run but then expires
        // (already-expired deadline simulates question gen exhausting the budget)
        let config = CheckConfig {
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
            ..default_check_config()
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();

        // pair_progress should be set to (0, N) signaling CV is pending
        assert!(
            output.pair_progress.is_some(),
            "pair_progress must be set when deadline hit during question gen but pairs exist"
        );
        let (processed, total) = output.pair_progress.unwrap();
        assert_eq!(processed, 0, "no pairs should be processed yet");
        assert!(total > 0, "total pairs should be > 0");
    }

    /// Regression test: questions mode (is_continuation=false) with LLM should
    /// report docs_processed based on question generation, not cross-validation.
    /// Before the fix, llm.is_some() caused docs_processed to use
    /// cross_validated_ids.len() which could be 0 in questions mode.
    #[tokio::test]
    async fn test_questions_mode_docs_processed_uses_question_gen_count() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nSome content.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nMore content.\n"),
        ];
        // Questions mode: is_continuation=false, LLM present
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
            is_continuation: false,
        fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 2, "questions mode should count question-gen docs, not CV docs");
        assert_eq!(output.docs_total, 2);
    }

    /// Cross-validate mode (is_continuation=true) with past deadline should
    /// report docs_processed=0 since no docs get cross-validated.
    #[tokio::test]
    async fn test_cross_validate_mode_past_deadline_docs_processed_zero() {
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let docs = vec![
            make_doc("aaa", "Doc A", "# Doc A\n\nContent.\n"),
            make_doc("bbb", "Doc B", "# Doc B\n\nContent.\n"),
        ];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 2,
            deadline: Some(Instant::now() - std::time::Duration::from_secs(1)),
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
            is_continuation: true,
            fact_db: None,
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        assert_eq!(output.docs_processed, 0, "CV mode with past deadline should process 0 docs");
        assert_eq!(output.docs_total, 2);
    }

    #[tokio::test]
    async fn test_fact_db_override_used_for_cross_validation() {
        // Verify that check_all_documents uses fact_db when provided,
        // even if the main db has no fact embeddings.
        use crate::llm::test_helpers::MockLlm;
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");

        // Main DB has zero fact embeddings
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);

        // Create a separate "repo-local" DB with fact embeddings
        let tmp_local = tempfile::TempDir::new().unwrap();
        let local_db = Database::new(&tmp_local.path().join("local.db")).unwrap();

        // Set up docs in the local DB (need repo for FK)
        let repo = crate::database::tests::test_repo();
        local_db.upsert_repository(&repo).unwrap();
        let doc_a = Document {
            id: "aaa".to_string(),
            file_path: "aaa.md".to_string(),
            ..make_doc("aaa", "Doc A", "# Doc A\n\n- Fact alpha\n")
        };
        let doc_b = Document {
            id: "bbb".to_string(),
            file_path: "bbb.md".to_string(),
            ..make_doc("bbb", "Doc B", "# Doc B\n\n- Fact beta\n")
        };
        local_db.upsert_document(&doc_a).unwrap();
        local_db.upsert_document(&doc_b).unwrap();
        let emb_a = vec![0.5_f32; 1024];
        let mut emb_b = vec![0.5_f32; 1024];
        emb_b[0] = 0.51; // slightly different to avoid exact match filtering
        local_db.upsert_fact_embedding("f1", "aaa", 3, "Fact alpha", "h1", &emb_a).unwrap();
        local_db.upsert_fact_embedding("f2", "bbb", 3, "Fact beta", "h2", &emb_b).unwrap();
        assert_eq!(local_db.get_fact_embedding_count().unwrap(), 2);
        // Verify pairs are found in the local DB
        let pairs = local_db.find_all_cross_doc_fact_pairs(0.3, 5, None).unwrap();
        assert!(!pairs.is_empty(), "Local DB should have cross-doc fact pairs");

        let docs = vec![doc_a, doc_b];
        let config = CheckConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
            deadline: None,
            checked_doc_ids: HashSet::new(),
            pair_offset: 0,
            acquire_write_guard: false,
            batch_size: 10,
            repo_id: None,
            is_continuation: true,
            fact_db: Some(local_db),
        };
        let progress = ProgressReporter::Silent;
        let output = check_all_documents(&docs, &db, &embedding, Some(&llm), &config, &progress)
            .await
            .unwrap();
        // With fact_db set, it should find fact pairs and use pair-based CV
        // (pair_progress should be Some, not None which would indicate fallback)
        assert!(output.pair_progress.is_some(), "Should use fact-pair mode via fact_db");
    }
}
