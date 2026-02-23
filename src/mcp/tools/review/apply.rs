//! Apply answered review questions via MCP.

use crate::answer_processor::apply_all::{apply_all_review_answers, ApplyConfig, ApplyStatus};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_bool_arg, get_str_arg};
use crate::progress::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Apply answered review questions, rewriting document content via LLM.
///
/// This is the MCP equivalent of `factbase review --apply`.
#[instrument(name = "mcp_apply_review_answers", skip(db, llm, args, progress))]
pub async fn apply_review_answers(
    db: &Database,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let llm = llm
        .ok_or_else(|| FactbaseError::internal("LLM provider required for apply_review_answers"))?;

    let doc_id_filter = get_str_arg(args, "doc_id");
    let repo_filter = get_str_arg(args, "repo");
    let dry_run = get_bool_arg(args, "dry_run", false);

    let config = ApplyConfig {
        doc_id_filter,
        repo_filter,
        dry_run,
        since: None,
    };

    let result = apply_all_review_answers(db, llm, &config, progress).await?;

    let documents: Vec<Value> = result
        .documents
        .iter()
        .map(|d| {
            let mut json = serde_json::json!({
                "doc_id": d.doc_id,
                "doc_title": d.doc_title,
            });
            let obj = json
                .as_object_mut()
                .expect("json! macro with object literal returns a JSON object");
            match d.status {
                ApplyStatus::Applied => {
                    obj.insert("questions_applied".into(), d.questions_applied.into());
                    obj.insert("status".into(), "applied".into());
                }
                ApplyStatus::DryRun => {
                    obj.insert("questions_applied".into(), d.questions_applied.into());
                    obj.insert("status".into(), "dry_run".into());
                }
                ApplyStatus::Error => {
                    obj.insert("status".into(), "error".into());
                    if let Some(ref e) = d.error {
                        obj.insert("error".into(), e.clone().into());
                    }
                }
            }
            json
        })
        .collect();

    Ok(serde_json::json!({
        "total_applied": result.total_applied,
        "total_errors": result.total_errors,
        "dry_run": dry_run,
        "documents": documents,
        "message": if dry_run {
            format!("Dry run: {} question(s) would be applied", result.total_applied)
        } else if result.total_applied > 0 {
            format!("Applied {} question(s). Run scan_repository to re-index.", result.total_applied)
        } else {
            "No answered questions to apply.".to_string()
        }
    }))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn test_write_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "original").unwrap();
        crate::organize::fs_helpers::write_file(&path, "updated").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "updated");
        assert!(!path.with_extension("md.tmp").exists());
    }
}
