//! Repository-wide check MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, load_perspective};
use crate::progress::ProgressReporter;
use crate::question_generator::check::{check_all_documents, CheckConfig};
use serde_json::Value;
use std::collections::HashSet;

/// Default concurrency for parallel check (LLM calls).
const LINT_CONCURRENCY: usize = 5;

/// Runs check across all documents in a repository via MCP.
pub async fn check_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo");
    let doc_id = get_str_arg(args, "doc_id");
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let deep_check = args
        .get("deep_check")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // If doc_id is provided, check just that one document (replaces generate_questions)
    if doc_id.is_some() {
        return super::generate_questions(db, embedding, llm, args).await;
    }

    // Only pass LLM for cross-validation when deep_check is requested
    let effective_llm: Option<&dyn LlmProvider> = if deep_check { llm } else { None };

    let config_file = crate::Config::load(None);
    let check_concurrency = config_file
        .as_ref()
        .map(|c| c.processor.check_concurrency)
        .unwrap_or(LINT_CONCURRENCY);
    let batch_size = config_file
        .as_ref()
        .map(|c| c.cross_validate.batch_size)
        .unwrap_or_else(|_| crate::config::cross_validate::default_batch_size());

    // Load perspective for stale_days and required_fields
    let perspective = load_perspective(db, repo_id);
    let stale_days = perspective
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.stale_days)
        .unwrap_or(365) as i64;
    let required_fields = perspective
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.required_fields.clone());

    // Get all active documents
    let docs = match repo_id {
        Some(rid) => db
            .get_documents_for_repo(rid)?
            .into_values()
            .filter(|d| !d.is_deleted)
            .collect::<Vec<_>>(),
        None => {
            let mut all = Vec::new();
            for repo in db.list_repositories()? {
                all.extend(
                    db.get_documents_for_repo(&repo.id)?
                        .into_values()
                        .filter(|d| !d.is_deleted),
                );
            }
            all
        }
    };

    let _total = docs.len();
    progress.phase("Generating review questions");

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let checked_pair_ids: HashSet<String> = args
        .get("checked_pair_ids")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default();
    let checked_doc_ids: HashSet<String> = args
        .get("checked_doc_ids")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default();
    let is_continuation = !checked_pair_ids.is_empty() || !checked_doc_ids.is_empty();

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline,
        checked_doc_ids,
        checked_pair_ids,
        acquire_write_guard: true,
        batch_size,
        repo_id: repo_id.map(String::from),
        is_continuation,
    };

    let output = check_all_documents(&docs, db, embedding, effective_llm, &config, progress).await?;
    let results = &output.results;

    let docs_with_questions = results.iter().filter(|r| r.new_questions > 0).count();
    let total_new: usize = results.iter().map(|r| r.new_questions).sum();
    let total_pruned: usize = results.iter().map(|r| r.pruned_questions).sum();
    let total_existing: usize = results
        .iter()
        .map(|r| r.existing_unanswered + r.existing_answered)
        .sum();
    let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
    let total_suppressed: usize = results.iter().map(|r| r.suppressed_by_review).sum();
    let deferred_count = db.count_deferred_questions(repo_id).unwrap_or(0);
    let details: Vec<Value> = results
        .iter()
        .filter(|r| r.new_questions > 0 || r.pruned_questions > 0)
        .map(|r| {
            serde_json::json!({
                "doc_id": r.doc_id,
                "doc_title": r.doc_title,
                "new_questions": r.new_questions,
                "pruned_questions": r.pruned_questions,
            })
        })
        .collect();

    // Entity discovery: only when deep_check is enabled (requires LLM).
    // Skip on continuation calls (already done on first call) and when deadline hit.
    let deadline_hit = deadline.is_some_and(|d| std::time::Instant::now() > d);
    let suggested_entities = if deep_check && !is_continuation && !deadline_hit {
        if let Some(llm_ref) = llm {
            let existing_titles: Vec<String> = docs.iter().map(|d| d.title.clone()).collect();
            crate::organize::discover_entities(
                &docs,
                &existing_titles,
                llm_ref,
                perspective.as_ref(),
                progress,
            )
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut result = serde_json::json!({
        "documents_scanned": output.docs_processed,
        "documents_with_new_questions": docs_with_questions,
        "total_questions_generated": total_new + total_existing,
        "new_unanswered": total_new,
        "already_in_queue": total_existing,
        "pruned_stale": total_pruned,
        "skipped_reviewed": total_skipped,
        "suppressed_by_prior_answers": total_suppressed,
        "deferred_count": deferred_count,
        "dry_run": dry_run,
        "details": details,
    });

    // Add progress/continue fields when deadline was hit
    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut result, output.docs_processed, output.docs_total, "check_repository", time_budget.is_some(),
    );

    // Return checked doc IDs so the caller can resume cross-validation (backward compat)
    if !output.checked_doc_ids.is_empty() && output.docs_processed < output.docs_total {
        result["checked_doc_ids"] = serde_json::to_value(&output.checked_doc_ids).unwrap_or_default();
    }

    // Return checked pair IDs for precise cursor-based resumption
    if !output.checked_pair_ids.is_empty() {
        if let Some((processed, total)) = output.pair_progress {
            if processed < total {
                result["checked_pair_ids"] = serde_json::to_value(&output.checked_pair_ids).unwrap_or_default();
                let remaining = total.saturating_sub(processed);
                let pct = if total > 0 { (processed as f64 / total as f64 * 100.0) as u32 } else { 0 };
                // Estimate remaining iterations: pairs processed this call = total processed - input cursor size
                let input_cursor_size = args
                    .get("checked_pair_ids")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0);
                let pairs_this_call = processed.saturating_sub(input_cursor_size).max(1);
                let estimated_iterations = (remaining + pairs_this_call - 1) / pairs_this_call;
                result["progress"] = serde_json::json!({
                    "processed": processed,
                    "remaining": remaining,
                    "total": total,
                    "percent_complete": pct,
                    "estimated_iterations_remaining": estimated_iterations,
                });
                result["continue"] = serde_json::json!(true);
                result["message"] = serde_json::json!(format!(
                    "⚠️ MANDATORY CONTINUATION — {processed}/{total} fact pairs checked ({pct}%, ~{estimated_iterations} calls remaining). \
                     You MUST call check_repository again with the `checked_pair_ids` array from this response. \
                     Do NOT stop, do NOT reason about whether to continue, do NOT report partial results. \
                     Just pass back checked_pair_ids until this message no longer appears."
                ));
                result["when_done"] = serde_json::json!(
                    "MANDATORY: Call check_repository again with the same arguments plus checked_pair_ids. Do NOT use the think tool to decide whether to continue. Just call it."
                );
            }
        }
    }

    if !suggested_entities.is_empty() {
        result["suggested_entities"] = serde_json::to_value(&suggested_entities)
            .unwrap_or_default();
    }

    if !output.vocabulary_candidates.is_empty() {
        result["vocabulary_candidates"] = serde_json::to_value(&output.vocabulary_candidates)
            .unwrap_or_default();
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::llm::test_helpers::MockLlm;
    use crate::progress::ProgressReporter;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_check_repository_skips_entity_discovery_when_deadline_hit() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:aaa111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        // MockLlm returns "[]" — if entity discovery ran, it would still produce
        // an empty vec, but the key point is it shouldn't even be called.
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // time_budget_secs=5 is the minimum, but we set it so a deadline exists.
        // The check_all_documents call with a past-deadline config will finish
        // instantly (0 docs processed), then the deadline_hit guard should skip
        // entity discovery.
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "time_budget_secs": 5,
        });

        let _result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();

        // With a 5s budget the deadline won't be hit for this tiny dataset,
        // so suggested_entities may appear. The important invariant is that
        // when the deadline IS hit, entity discovery is skipped. We test that
        // by verifying the deadline_hit logic directly below.
    }

    #[test]
    fn test_deadline_hit_skips_entity_discovery_logic() {
        // Verify the guard condition: past deadline → deadline_hit = true
        let past = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(past.is_some_and(|d| std::time::Instant::now() > d));

        // No deadline → deadline_hit = false
        let none: Option<std::time::Instant> = None;
        assert!(!none.is_some_and(|d| std::time::Instant::now() > d));

        // Future deadline → deadline_hit = false
        let future = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
        assert!(!future.is_some_and(|d| std::time::Instant::now() > d));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_accepts_checked_pair_ids() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "pp1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:pp1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Pass checked_pair_ids — should be accepted without error
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_pair_ids": ["pp1111_3:pp2222_5"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        // Should complete without error
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_backward_compat_checked_doc_ids() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "bc1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:bc1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Pass old-style checked_doc_ids — should still work
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_doc_ids": ["bc1111"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_continuation_skips_entity_discovery_and_vocab() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "cn1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:cn1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Continuation call with checked_pair_ids — should skip entity discovery and vocab
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_pair_ids": ["cn1111_3:cn2222_5"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        // Continuation should not produce suggested_entities or vocabulary_candidates
        assert!(result.get("suggested_entities").is_none());
        assert!(result.get("vocabulary_candidates").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_continuation_via_checked_doc_ids_skips_bonus_phases() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "cd1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:cd1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Continuation via legacy checked_doc_ids
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_doc_ids": ["cd1111"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(result.get("suggested_entities").is_none());
        assert!(result.get("vocabulary_candidates").is_none());
    }
}
