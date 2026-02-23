//! Shared lint-all-documents loop for both MCP and CLI.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::llm::LlmProvider;
use crate::models::Document;
use crate::processor::{append_review_questions, content_hash, parse_review_queue};
use crate::progress::ProgressReporter;
use crate::question_generator::cross_validate::cross_validate_document;
use crate::question_generator::{
    generate_ambiguous_questions, generate_conflict_questions, generate_missing_questions,
    generate_required_field_questions, generate_stale_questions, generate_temporal_questions,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{info, warn};

/// Configuration for the shared lint loop.
pub struct LintConfig {
    pub stale_days: i64,
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    pub dry_run: bool,
    pub concurrency: usize,
}

/// Result of linting a single document.
pub struct LintDocResult {
    pub doc_id: String,
    pub doc_title: String,
    pub questions_added: usize,
}

/// Lint all documents: generate review questions, optionally cross-validate, write results.
///
/// Used by both MCP `lint_repository` and CLI `cmd_lint --review`.
pub async fn lint_all_documents(
    docs: &[Document],
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    config: &LintConfig,
    progress: &ProgressReporter,
) -> Result<Vec<LintDocResult>, crate::error::FactbaseError> {
    let total = docs.len();
    let rf_ref = &config.required_fields;

    let mut all_results = Vec::new();
    for chunk_start in (0..total).step_by(config.concurrency) {
        let chunk_end = (chunk_start + config.concurrency).min(total);
        let chunk = &docs[chunk_start..chunk_end];

        let futs: Vec<_> = chunk
            .iter()
            .enumerate()
            .map(|(ci, doc)| {
                let idx = chunk_start + ci;
                async move {
                    progress.report(idx + 1, total, &format!("Linting {}", doc.title));

                    let mut questions = generate_temporal_questions(&doc.content);
                    questions.extend(generate_conflict_questions(&doc.content));
                    questions.extend(generate_missing_questions(&doc.content));
                    questions.extend(generate_ambiguous_questions(&doc.content));
                    questions.extend(generate_stale_questions(&doc.content, config.stale_days));

                    if let Some(ref rf) = rf_ref {
                        questions.extend(generate_required_field_questions(
                            &doc.content,
                            Some(doc.doc_type.as_deref().unwrap_or("unknown")),
                            rf,
                        ));
                    }

                    if let Some(llm) = llm {
                        match cross_validate_document(&doc.content, &doc.id, db, embedding, llm)
                            .await
                        {
                            Ok(cross) => questions.extend(cross),
                            Err(e) => warn!("Cross-validation failed for {}: {e}", doc.id),
                        }
                    }

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

    // Write results (sequential for filesystem safety)
    let mut results = Vec::new();
    for (doc, questions) in all_results {
        if questions.is_empty() {
            continue;
        }
        let count = questions.len();
        if !config.dry_run {
            let updated = append_review_questions(&doc.content, &questions);
            let path = PathBuf::from(&doc.file_path);
            if path.exists() {
                std::fs::write(&path, &updated)?;
                let new_hash = content_hash(&updated);
                db.update_document_content(&doc.id, &updated, &new_hash)?;
            }
        }
        info!("{}: {} new questions", doc.title, count);
        results.push(LintDocResult {
            doc_id: doc.id.clone(),
            doc_title: doc.title.clone(),
            questions_added: count,
        });
    }

    Ok(results)
}
