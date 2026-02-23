//! Shared apply-all loop for review answer processing.
//!
//! Used by both MCP `apply_review_answers` and CLI `review --apply`.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::organize::fs_helpers::write_file;
use crate::processor::{normalize_review_section, parse_review_queue};
use crate::progress::ProgressReporter;
use crate::{
    apply_changes_to_section, apply_confirmations, apply_source_citations,
    identify_affected_section, interpret_answer, remove_processed_questions, replace_section,
    stamp_reviewed_lines, stamp_reviewed_markers, uncheck_deferred_questions, InterpretedAnswer,
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

    let all_no_changes = interpreted.iter().all(|ia| {
        matches!(
            ia.instruction,
            crate::ChangeInstruction::Dismiss | crate::ChangeInstruction::Defer
        )
    });

    // Partition indices: dismissed get removed, deferred get unchecked
    let dismissed_indices: Vec<usize> = answered
        .iter()
        .zip(interpreted.iter())
        .filter(|(_, ia)| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
        .map(|((i, _), _)| *i)
        .collect();
    let deferred_indices: Vec<usize> = answered
        .iter()
        .zip(interpreted.iter())
        .filter(|(_, ia)| matches!(ia.instruction, crate::ChangeInstruction::Defer))
        .map(|((i, _), _)| *i)
        .collect();

    if all_no_changes {
        if !dry_run {
            let today = chrono::Local::now().date_naive();
            // Only stamp reviewed markers on dismissed questions (not deferred)
            let dismissed_line_refs: Vec<usize> = interpreted
                .iter()
                .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
                .filter_map(|ia| ia.question.line_ref)
                .collect();
            let mut new_content = stamp_reviewed_lines(&content, &dismissed_line_refs, &today);
            new_content = uncheck_deferred_questions(&new_content, &deferred_indices);
            new_content = remove_processed_questions(&new_content, &dismissed_indices);
            new_content = normalize_review_section(&new_content);
            write_file(file_path, &new_content)?;
        }
        return Ok(0);
    }

    if dry_run {
        return Ok(review_questions.len());
    }

    // Check if all active instructions can be handled without LLM
    let active_instructions: Vec<_> = interpreted
        .iter()
        .filter(|ia| {
            !matches!(
                ia.instruction,
                crate::ChangeInstruction::Dismiss | crate::ChangeInstruction::Defer
            )
        })
        .collect();

    let all_deterministic = active_instructions.iter().all(|ia| {
        matches!(
            ia.instruction,
            crate::ChangeInstruction::AddSource { .. }
                | crate::ChangeInstruction::Delete { .. }
                | crate::ChangeInstruction::UpdateTemporal { .. }
                | crate::ChangeInstruction::AddTemporal { .. }
        )
    });

    if all_deterministic {
        let today = chrono::Local::now().date_naive();

        // Apply source citations to full document content
        let source_pairs: Vec<(&str, &str)> = active_instructions
            .iter()
            .filter_map(|ia| match &ia.instruction {
                crate::ChangeInstruction::AddSource {
                    line_text,
                    source_info,
                } => Some((line_text.as_str(), source_info.as_str())),
                _ => None,
            })
            .collect();
        let mut new_content = apply_source_citations(&content, &source_pairs);

        // Apply confirmation temporal tag updates
        let confirmation_updates: Vec<(&str, Option<&str>, &str)> = active_instructions
            .iter()
            .filter_map(|ia| match &ia.instruction {
                crate::ChangeInstruction::UpdateTemporal {
                    line_text,
                    old_tag,
                    new_tag,
                } => Some((line_text.as_str(), Some(old_tag.as_str()), new_tag.as_str())),
                crate::ChangeInstruction::AddTemporal { line_text, tag } => {
                    Some((line_text.as_str(), None, tag.as_str()))
                }
                _ => None,
            })
            .collect();
        new_content = apply_confirmations(&new_content, &confirmation_updates);

        // Apply deletes
        for ia in &active_instructions {
            if let crate::ChangeInstruction::Delete { line_text } = &ia.instruction {
                let lines: Vec<&str> = new_content.lines().collect();
                let filtered: Vec<&str> = lines
                    .into_iter()
                    .filter(|l| !l.contains(line_text.as_str()))
                    .collect();
                new_content = filtered.join("\n");
            }
        }

        // Stamp reviewed markers on all active fact lines
        let active_line_refs: Vec<usize> = active_instructions
            .iter()
            .filter_map(|ia| ia.question.line_ref)
            .collect();
        new_content = stamp_reviewed_lines(&new_content, &active_line_refs, &today);

        // Stamp dismissed fact lines too
        let dismissed_line_refs: Vec<usize> = interpreted
            .iter()
            .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
            .filter_map(|ia| ia.question.line_ref)
            .collect();
        if !dismissed_line_refs.is_empty() {
            new_content = stamp_reviewed_lines(&new_content, &dismissed_line_refs, &today);
        }

        new_content = uncheck_deferred_questions(&new_content, &deferred_indices);
        new_content = remove_processed_questions(&new_content, &dismissed_indices);

        // Remove all active (non-dismiss/defer) question indices too
        let active_indices: Vec<usize> = answered
            .iter()
            .zip(interpreted.iter())
            .filter(|(_, ia)| {
                !matches!(
                    ia.instruction,
                    crate::ChangeInstruction::Dismiss | crate::ChangeInstruction::Defer
                )
            })
            .map(|((i, _), _)| *i)
            .collect();
        new_content = remove_processed_questions(&new_content, &active_indices);

        new_content = normalize_review_section(&new_content);
        write_file(file_path, &new_content)?;
        return Ok(review_questions.len());
    }

    let Some((start, end, section)) = identify_affected_section(&content, &review_questions) else {
        return Err(FactbaseError::internal(
            "Could not identify affected section",
        ));
    };

    let new_section = apply_changes_to_section(llm, &section, &interpreted).await?;
    let today = chrono::Local::now().date_naive();
    let new_section = stamp_reviewed_markers(&new_section, &today);
    let mut new_content = replace_section(&content, start, end, &new_section);

    // Stamp reviewed markers on fact lines for dismissed questions only (not deferred)
    let dismissed_line_refs: Vec<usize> = interpreted
        .iter()
        .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
        .filter_map(|ia| ia.question.line_ref)
        .collect();
    if !dismissed_line_refs.is_empty() {
        new_content = stamp_reviewed_lines(&new_content, &dismissed_line_refs, &today);
    }

    // Uncheck deferred questions first (before removal shifts indices), then remove dismissed
    new_content = uncheck_deferred_questions(&new_content, &deferred_indices);
    new_content = remove_processed_questions(&new_content, &dismissed_indices);

    new_content = normalize_review_section(&new_content);
    write_file(file_path, &new_content)?;
    Ok(review_questions.len())
}
