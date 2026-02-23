//! Repository-wide lint MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, ProgressSender};
use crate::models::Perspective;
use crate::processor::{append_review_questions, parse_review_queue};
use crate::question_generator::cross_validate::cross_validate_document;
use crate::question_generator::{
    generate_ambiguous_questions, generate_conflict_questions, generate_missing_questions,
    generate_required_field_questions, generate_stale_questions, generate_temporal_questions,
};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::{info, warn};

/// Default concurrency for parallel lint (LLM calls).
const LINT_CONCURRENCY: usize = 5;

/// Runs lint --review across all documents in a repository via MCP.
pub async fn lint_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: Option<ProgressSender>,
) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo");
    let dry_run = args
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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
    let progress_ref = &progress;
    let rf_ref = &required_fields;

    // Process documents in concurrent batches
    let mut all_results = Vec::new();
    for chunk_start in (0..total).step_by(LINT_CONCURRENCY) {
        let chunk_end = (chunk_start + LINT_CONCURRENCY).min(total);
        let chunk = &docs[chunk_start..chunk_end];

        let futs: Vec<_> = chunk
            .iter()
            .enumerate()
            .map(|(ci, doc)| {
                let idx = chunk_start + ci;
                async move {
                    // Log progress (always visible in stderr)
                    info!("Linting [{}/{}] {}", idx + 1, total, doc.title);
                    if let Some(ref tx) = progress_ref {
                        let _ = tx.send(serde_json::json!({
                            "progress": idx,
                            "total": total,
                            "message": format!("Linting {}", doc.title),
                        }));
                    }

                    // Local generators (fast, CPU-only)
                    let mut questions = generate_temporal_questions(&doc.content);
                    questions.extend(generate_conflict_questions(&doc.content));
                    questions.extend(generate_missing_questions(&doc.content));
                    questions.extend(generate_ambiguous_questions(&doc.content));
                    questions.extend(generate_stale_questions(&doc.content, stale_days));

                    if let Some(ref rf) = rf_ref {
                        questions.extend(generate_required_field_questions(
                            &doc.content,
                            Some(doc.doc_type.as_deref().unwrap_or("unknown")),
                            rf,
                        ));
                    }

                    // Cross-document validation (slow, LLM call)
                    if let Some(llm) = llm {
                        match cross_validate_document(&doc.content, &doc.id, db, embedding, llm)
                            .await
                        {
                            Ok(cross) => questions.extend(cross),
                            Err(e) => warn!("Cross-validation failed for {}: {e}", doc.id),
                        }
                    }

                    // Filter existing questions
                    let existing: HashSet<_> = parse_review_queue(&doc.content)
                        .unwrap_or_default()
                        .iter()
                        .map(|q| q.description.clone())
                        .collect();
                    questions.retain(|q| !existing.contains(&q.description));

                    (doc, questions)
                }
            })
            .collect();

        let batch = futures::future::join_all(futs).await;
        all_results.extend(batch);
    }

    // Write results and build summary (sequential for filesystem safety)
    let mut docs_with_questions = 0;
    let mut total_questions = 0;
    let mut details = Vec::new();

    for (doc, questions) in all_results {
        if questions.is_empty() {
            continue;
        }

        let count = questions.len();
        total_questions += count;
        docs_with_questions += 1;

        if !dry_run {
            let updated = append_review_questions(&doc.content, &questions);
            let path = PathBuf::from(&doc.file_path);
            if path.exists() {
                std::fs::write(&path, &updated)?;
            }
        }

        details.push(serde_json::json!({
            "doc_id": doc.id,
            "doc_title": doc.title,
            "questions_added": count,
        }));

        info!("{}: {} new questions", doc.title, count);
    }

    Ok(serde_json::json!({
        "documents_scanned": total,
        "documents_with_new_questions": docs_with_questions,
        "total_questions_generated": total_questions,
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
