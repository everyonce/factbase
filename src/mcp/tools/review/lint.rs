//! Repository-wide lint MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::get_str_arg;
use crate::models::Perspective;
use crate::progress::ProgressReporter;
use crate::question_generator::lint::{lint_all_documents, LintConfig};
use serde_json::Value;

/// Default concurrency for parallel lint (LLM calls).
const LINT_CONCURRENCY: usize = 5;

/// Runs lint --review across all documents in a repository via MCP.
pub async fn lint_repository(
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

    // If doc_id is provided, lint just that one document (replaces generate_questions)
    if doc_id.is_some() {
        return super::generate_questions(db, embedding, llm, args).await;
    }

    let lint_concurrency = crate::Config::load(None)
        .map(|c| c.processor.lint_concurrency)
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

    let total = docs.len();
    progress.phase("Generating review questions");

    let config = LintConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: lint_concurrency,
    };

    let results = lint_all_documents(&docs, db, embedding, llm, &config, progress).await?;

    let docs_with_questions = results.iter().filter(|r| r.new_questions > 0).count();
    let total_new: usize = results.iter().map(|r| r.new_questions).sum();
    let total_existing: usize = results
        .iter()
        .map(|r| r.existing_unanswered + r.existing_answered)
        .sum();
    let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
    let deferred_count: usize = docs
        .iter()
        .filter_map(|d| crate::processor::parse_review_queue(&d.content))
        .flatten()
        .filter(|q| !q.answered && q.answer.is_some())
        .count();
    let details: Vec<Value> = results
        .iter()
        .filter(|r| r.new_questions > 0)
        .map(|r| {
            serde_json::json!({
                "doc_id": r.doc_id,
                "doc_title": r.doc_title,
                "new_questions": r.new_questions,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "documents_scanned": total,
        "documents_with_new_questions": docs_with_questions,
        "total_questions_generated": total_new + total_existing,
        "new_unanswered": total_new,
        "already_in_queue": total_existing,
        "skipped_reviewed": total_skipped,
        "deferred_count": deferred_count,
        "dry_run": dry_run,
        "details": details,
    }))
}

fn load_perspective(db: &Database, repo_id: Option<&str>) -> Option<Perspective> {
    let repos = db.list_repositories().ok()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.and_then(|r| r.perspective)
}
