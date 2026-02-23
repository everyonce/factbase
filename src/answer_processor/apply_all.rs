//! Shared apply-all loop for review answer processing.
//!
//! Used by both MCP `apply_review_answers` and CLI `review --apply`.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::organize::fs_helpers::write_file;
use crate::processor::parse_review_queue;
use crate::progress::ProgressReporter;
use crate::{
    apply_changes_to_section, identify_affected_section, interpret_answer,
    remove_processed_questions, replace_section, InterpretedAnswer,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Result of applying review answers across documents.
#[derive(Debug, Default)]
pub struct ApplyResult {
    pub total_applied: usize,
    pub total_errors: usize,
    pub filtered_count: usize,
    pub documents: Vec<ApplyDocResult>,
}

/// Per-document result.
#[derive(Debug)]
pub struct ApplyDocResult {
    pub doc_id: String,
    pub doc_title: String,
    pub questions_applied: usize,
    pub status: ApplyStatus,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum ApplyStatus {
    Applied,
    DryRun,
    Error,
}

/// Configuration for the apply-all operation.
pub struct ApplyConfig<'a> {
    pub doc_id_filter: Option<&'a str>,
    pub repo_filter: Option<&'a str>,
    pub dry_run: bool,
    pub since: Option<DateTime<Utc>>,
}

/// Apply all answered review questions across documents.
///
/// Core loop shared by MCP and CLI. Loads documents with review queues,
/// filters by optional doc_id/repo/since, and applies answered questions.
pub async fn apply_all_review_answers(
    db: &Database,
    llm: &dyn LlmProvider,
    config: &ApplyConfig<'_>,
    progress: &ProgressReporter,
) -> Result<ApplyResult, FactbaseError> {
    let docs = db.get_documents_with_review_queue(config.repo_filter)?;
    let repos = db.list_repositories()?;
    let repo_paths: HashMap<_, _> = repos.iter().map(|r| (r.id.as_str(), &r.path)).collect();

    let mut result = ApplyResult::default();
    let mut work = Vec::new();

    for doc in &docs {
        if let Some(filter_id) = config.doc_id_filter {
            if doc.id != filter_id {
                continue;
            }
        }
        // Filter by modification time if --since is specified
        if let Some(since) = config.since {
            if let Some(modified) = doc.file_modified_at {
                if modified < since {
                    result.filtered_count += 1;
                    continue;
                }
            }
        }
        let Some(questions) = parse_review_queue(&doc.content) else {
            continue;
        };
        let answered: Vec<_> = questions
            .into_iter()
            .enumerate()
            .filter(|(_, q)| q.answered && q.answer.is_some())
            .collect();
        if answered.is_empty() {
            continue;
        }
        let abs_path = match repo_paths.get(doc.repo_id.as_str()) {
            Some(repo_path) => repo_path.join(&doc.file_path),
            None => continue,
        };
        work.push((doc, answered, abs_path));
    }

    let total = work.len();

    for (i, (doc, answered, abs_path)) in work.iter().enumerate() {
        let count = answered.len();
        progress.report(
            i + 1,
            total,
            &format!("Applying {} question(s) to {}", count, doc.title),
        );

        match apply_one_document(llm, abs_path, answered, config.dry_run).await {
            Ok(applied) => {
                result.total_applied += applied;
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: applied,
                    status: if config.dry_run {
                        ApplyStatus::DryRun
                    } else {
                        ApplyStatus::Applied
                    },
                    error: None,
                });
            }
            Err(e) => {
                result.total_errors += 1;
                warn!(doc_id = %doc.id, error = %e, "Failed to apply review answers");
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: 0,
                    status: ApplyStatus::Error,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    info!(
        applied = result.total_applied,
        errors = result.total_errors,
        "apply_all_review_answers complete"
    );

    Ok(result)
}

async fn apply_one_document(
    llm: &dyn LlmProvider,
    file_path: &Path,
    answered: &[(usize, crate::models::ReviewQuestion)],
    dry_run: bool,
) -> Result<usize, FactbaseError> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| FactbaseError::internal(format!("{}: {}", file_path.display(), e)))?;

    let review_questions: Vec<_> = answered.iter().map(|(_, q)| q.clone()).collect();

    let interpreted: Vec<InterpretedAnswer> = review_questions
        .iter()
        .map(|q| {
            let answer = q.answer.as_deref().unwrap_or("");
            InterpretedAnswer {
                question: q.clone(),
                instruction: interpret_answer(q, answer),
            }
        })
        .collect();

    let all_dismissed = interpreted
        .iter()
        .all(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss));

    if all_dismissed {
        if !dry_run {
            let indices: Vec<usize> = answered.iter().map(|(i, _)| *i).collect();
            let new_content = remove_processed_questions(&content, &indices);
            write_file(file_path, &new_content)?;
        }
        return Ok(0);
    }

    if dry_run {
        return Ok(review_questions.len());
    }

    let Some((start, end, section)) = identify_affected_section(&content, &review_questions) else {
        return Err(FactbaseError::internal(
            "Could not identify affected section",
        ));
    };

    let new_section = apply_changes_to_section(llm, &section, &interpreted).await?;
    let mut new_content = replace_section(&content, start, end, &new_section);

    let indices: Vec<usize> = answered.iter().map(|(i, _)| *i).collect();
    new_content = remove_processed_questions(&new_content, &indices);

    write_file(file_path, &new_content)?;
    Ok(review_questions.len())
}
