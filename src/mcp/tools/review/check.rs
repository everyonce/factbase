//! Repository-wide check MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, load_perspective};
use crate::progress::ProgressReporter;
use crate::question_generator::check::{check_all_documents, CheckConfig};
use serde_json::Value;

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

    let check_concurrency = crate::Config::load(None)
        .map(|c| c.processor.check_concurrency)
        .unwrap_or(LINT_CONCURRENCY);

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

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline,
        checked_doc_ids: args
            .get("checked_doc_ids")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default(),
        acquire_write_guard: true,
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

    // Entity discovery: only when deep_check is enabled (requires LLM)
    let suggested_entities = if deep_check {
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

    // Return checked doc IDs so the caller can resume cross-validation
    if !output.checked_doc_ids.is_empty() && output.docs_processed < output.docs_total {
        result["checked_doc_ids"] = serde_json::to_value(&output.checked_doc_ids).unwrap_or_default();
    }

    if !suggested_entities.is_empty() {
        result["suggested_entities"] = serde_json::to_value(&suggested_entities)
            .unwrap_or_default();
    }

    Ok(result)
}
