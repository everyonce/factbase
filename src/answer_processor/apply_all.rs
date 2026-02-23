//! Shared apply-all loop for review answer processing.
//!
//! Used by both MCP `apply_review_answers` and CLI `review --apply`.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::organize::fs_helpers::write_file;
use crate::processor::{content_hash, normalize_review_section, parse_review_queue};
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
        let abs_path = match repo_paths.get(doc.repo_id.as_str()) {
            Some(repo_path) => repo_path.join(&doc.file_path),
            None => continue,
        };
        // Parse questions from the file on disk (not the database) so that
        // line_ref values match the current file content.  If the document was
        // enriched after the last scan the DB content is stale and its line
        // numbers will be wrong, causing the LLM rewrite to target the wrong
        // section and drop recently-added content.
        let disk_content = match fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let Some(questions) = parse_review_queue(&disk_content) else {
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

        let apply_result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            apply_one_document(llm, abs_path, answered, config.dry_run),
        )
        .await;

        match apply_result {
            Ok(Ok(applied)) => {
                // Sync updated file content back to database
                if !config.dry_run {
                    if let Ok(new_content) = fs::read_to_string(abs_path) {
                        let new_hash = content_hash(&new_content);
                        if let Err(e) = db.update_document_content(&doc.id, &new_content, &new_hash) {
                            warn!(doc_id = %doc.id, error = %e, "Failed to sync content to database after apply");
                        }
                    }
                }
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
            Ok(Err(e)) => {
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
            Err(_) => {
                result.total_errors += 1;
                warn!(doc_id = %doc.id, "Timed out applying review answers (120s)");
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: 0,
                    status: ApplyStatus::Error,
                    error: Some("Timed out after 120 seconds".to_string()),
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

    // Split instructions: apply deterministic ones first, then LLM for the rest
    let deterministic: Vec<_> = active_instructions
        .iter()
        .filter(|ia| {
            matches!(
                ia.instruction,
                crate::ChangeInstruction::AddSource { .. }
                    | crate::ChangeInstruction::Delete { .. }
                    | crate::ChangeInstruction::UpdateTemporal { .. }
                    | crate::ChangeInstruction::AddTemporal { .. }
            )
        })
        .collect();
    let needs_llm: Vec<_> = active_instructions
        .iter()
        .filter(|ia| {
            !matches!(
                ia.instruction,
                crate::ChangeInstruction::AddSource { .. }
                    | crate::ChangeInstruction::Delete { .. }
                    | crate::ChangeInstruction::UpdateTemporal { .. }
                    | crate::ChangeInstruction::AddTemporal { .. }
            )
        })
        .collect();

    let today = chrono::Local::now().date_naive();
    let mut new_content = content.clone();

    // Apply deterministic instructions directly (no LLM needed)
    if !deterministic.is_empty() {
        let source_pairs: Vec<(&str, &str)> = deterministic
            .iter()
            .filter_map(|ia| match &ia.instruction {
                crate::ChangeInstruction::AddSource {
                    line_text,
                    source_info,
                } => Some((line_text.as_str(), source_info.as_str())),
                _ => None,
            })
            .collect();
        new_content = apply_source_citations(&new_content, &source_pairs);

        let confirmation_updates: Vec<(&str, Option<&str>, &str)> = deterministic
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

        for ia in &deterministic {
            if let crate::ChangeInstruction::Delete { line_text } = &ia.instruction {
                if line_text.is_empty() {
                    continue;
                }
                let lines: Vec<&str> = new_content.lines().collect();
                let filtered: Vec<&str> = lines
                    .into_iter()
                    .filter(|l| !l.contains(line_text.as_str()))
                    .collect();
                new_content = filtered.join("\n");
            }
        }

        let det_line_refs: Vec<usize> = deterministic
            .iter()
            .filter_map(|ia| ia.question.line_ref)
            .collect();
        new_content = stamp_reviewed_lines(&new_content, &det_line_refs, &today);
    }

    // Apply LLM-dependent instructions (corrections, splits, generic)
    if !needs_llm.is_empty() {
        let llm_questions: Vec<_> = needs_llm.iter().map(|ia| ia.question.clone()).collect();
        let Some((start, end, section)) =
            identify_affected_section(&new_content, &llm_questions)
        else {
            return Err(FactbaseError::internal(
                "Could not identify affected section",
            ));
        };

        let llm_interpreted: Vec<InterpretedAnswer> =
            needs_llm.into_iter().map(|ia| (*ia).clone()).collect();
        let new_section =
            apply_changes_to_section(llm, &section, &llm_interpreted).await?;
        let new_section = stamp_reviewed_markers(&new_section, &today);
        new_content = replace_section(&new_content, start, end, &new_section);
    }

    // Stamp reviewed markers on dismissed fact lines
    let dismissed_line_refs: Vec<usize> = interpreted
        .iter()
        .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
        .filter_map(|ia| ia.question.line_ref)
        .collect();
    if !dismissed_line_refs.is_empty() {
        new_content = stamp_reviewed_lines(&new_content, &dismissed_line_refs, &today);
    }

    // Uncheck deferred questions first (before removal shifts indices), then remove processed
    new_content = uncheck_deferred_questions(&new_content, &deferred_indices);
    // Remove dismissed questions
    new_content = remove_processed_questions(&new_content, &dismissed_indices);
    // Remove all active (non-dismiss/defer) questions that were applied
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
    Ok(review_questions.len())
}
