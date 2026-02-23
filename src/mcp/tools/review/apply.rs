//! Apply answered review questions via MCP.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_bool_arg, get_str_arg};
use crate::processor::parse_review_queue;
use crate::{
    apply_changes_to_section, identify_affected_section, interpret_answer,
    remove_processed_questions, replace_section, InterpretedAnswer,
};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tracing::{info, instrument, warn};

/// Apply answered review questions, rewriting document content via LLM.
///
/// This is the MCP equivalent of `factbase review --apply`.
#[instrument(name = "mcp_apply_review_answers", skip(db, llm, args))]
pub async fn apply_review_answers(
    db: &Database,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let llm = llm.ok_or_else(|| {
        FactbaseError::internal("LLM provider required for apply_review_answers")
    })?;

    let doc_id_filter = get_str_arg(args, "doc_id");
    let repo_filter = get_str_arg(args, "repo");
    let dry_run = get_bool_arg(args, "dry_run", false);

    let repos = if let Some(repo_id) = repo_filter {
        db.list_repositories()?
            .into_iter()
            .filter(|r| r.id == repo_id)
            .collect()
    } else {
        db.list_repositories()?
    };

    let mut results: Vec<Value> = Vec::new();
    let mut total_applied = 0usize;
    let mut total_errors = 0usize;

    for repo in &repos {
        let docs = db.get_documents_for_repo(&repo.id)?;

        for doc in docs.values() {
            if doc.is_deleted {
                continue;
            }
            if let Some(filter_id) = doc_id_filter {
                if doc.id != filter_id {
                    continue;
                }
            }

            let questions = match parse_review_queue(&doc.content) {
                Some(q) => q,
                None => continue,
            };

            let answered: Vec<_> = questions
                .into_iter()
                .enumerate()
                .filter(|(_, q)| q.answered && q.answer.is_some())
                .collect();

            if answered.is_empty() {
                continue;
            }

            let abs_path = repo.path.join(&doc.file_path);
            match apply_one_document(llm, &abs_path, &answered, dry_run).await {
                Ok(count) => {
                    total_applied += count;
                    results.push(serde_json::json!({
                        "doc_id": doc.id,
                        "doc_title": doc.title,
                        "questions_applied": count,
                        "status": if dry_run { "dry_run" } else { "applied" }
                    }));
                }
                Err(e) => {
                    total_errors += 1;
                    warn!(doc_id = %doc.id, error = %e, "Failed to apply review answers");
                    results.push(serde_json::json!({
                        "doc_id": doc.id,
                        "doc_title": doc.title,
                        "status": "error",
                        "error": e.to_string()
                    }));
                }
            }
        }
    }

    info!(applied = total_applied, errors = total_errors, "apply_review_answers complete");

    Ok(serde_json::json!({
        "total_applied": total_applied,
        "total_errors": total_errors,
        "dry_run": dry_run,
        "documents": results,
        "message": if dry_run {
            format!("Dry run: {} question(s) would be applied", total_applied)
        } else if total_applied > 0 {
            format!("Applied {} question(s). Run scan_repository to re-index.", total_applied)
        } else {
            "No answered questions to apply.".to_string()
        }
    }))
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

    // Interpret all answers
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

    // Check if all dismissed
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

    // Identify affected section and apply via LLM
    let Some((start, end, section)) = identify_affected_section(&content, &review_questions) else {
        return Err(FactbaseError::internal("Could not identify affected section"));
    };

    let new_section = apply_changes_to_section(llm, &section, &interpreted).await?;
    let mut new_content = replace_section(&content, start, end, &new_section);

    let indices: Vec<usize> = answered.iter().map(|(i, _)| *i).collect();
    new_content = remove_processed_questions(&new_content, &indices);

    write_file(file_path, &new_content)?;
    Ok(review_questions.len())
}

fn write_file(path: &Path, content: &str) -> Result<(), FactbaseError> {
    let temp_path = path.with_extension("md.tmp");
    fs::write(&temp_path, content).map_err(FactbaseError::from)?;
    fs::rename(&temp_path, path).map_err(FactbaseError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "original").unwrap();
        write_file(&path, "updated").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "updated");
        assert!(!path.with_extension("md.tmp").exists());
    }
}
