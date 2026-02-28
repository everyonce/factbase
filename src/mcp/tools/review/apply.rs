//! Apply answered review questions via MCP.

use crate::answer_processor::apply_all::{apply_all_review_answers, ApplyConfig, ApplyStatus};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::helpers::WriteGuard;
use crate::mcp::tools::{get_bool_arg, get_str_arg};
use crate::processor::parse_review_queue;
use crate::progress::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Extract doc_id that may be passed as string or number.
fn get_doc_id_arg(args: &Value) -> Option<String> {
    args.get("doc_id").and_then(|v| {
        v.as_str()
            .map(String::from)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    })
}

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

    let doc_id_owned = get_doc_id_arg(args);
    let doc_id_filter = doc_id_owned.as_deref();
    let repo_filter = get_str_arg(args, "repo");
    let dry_run = get_bool_arg(args, "dry_run", false);
    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let config = ApplyConfig {
        doc_id_filter,
        repo_filter,
        dry_run,
        since: None,
        deadline,
    };

    // Acquire write guard for non-dry-run (rewrites doc content on disk+DB)
    let _write_guard = if dry_run { None } else { Some(WriteGuard::try_acquire()?) };

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

    let no_questions_msg = if doc_id_filter.is_some() && result.documents.is_empty() && result.total_errors == 0 {
        let id = doc_id_filter.unwrap();
        match db.get_document(id)? {
            None => format!("Document '{id}' not found."),
            Some(doc) => {
                let q_count = parse_review_queue(&doc.content)
                    .map(|qs| (qs.iter().filter(|q| q.answered).count(), qs.len()))
                    .unwrap_or((0, 0));
                format!(
                    "Document '{id}' has {} question(s) ({} answered) but none could be applied. \
                     Ensure the file exists on disk at the registered repository path.",
                    q_count.1, q_count.0
                )
            }
        }
    } else {
        "No answered questions to apply.".to_string()
    };

    let mut response = serde_json::json!({
        "total_applied": result.total_applied,
        "total_errors": result.total_errors,
        "dry_run": dry_run,
        "documents": documents,
        "message": if dry_run {
            format!("Dry run: {} question(s) would be applied", result.total_applied)
        } else if result.total_applied > 0 {
            format!("Applied {} question(s). Run scan_repository to re-index.", result.total_applied)
        } else if !result.documents.is_empty() {
            format!("Processed {} document(s) (all questions dismissed/deferred). Run scan_repository to re-index.", result.documents.len())
        } else {
            no_questions_msg
        }
    });

    // Add progress/continue fields when deadline was hit
    let processed = result.documents.len();
    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut response, processed, result.total_work, "apply_review_answers", time_budget.is_some(),
    );

    Ok(response)
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

    #[test]
    fn test_get_doc_id_arg_string() {
        let args = serde_json::json!({"doc_id": "abc123"});
        assert_eq!(super::get_doc_id_arg(&args), Some("abc123".to_string()));
    }

    #[test]
    fn test_get_doc_id_arg_number() {
        let args = serde_json::json!({"doc_id": 543601});
        assert_eq!(super::get_doc_id_arg(&args), Some("543601".to_string()));
    }

    #[test]
    fn test_get_doc_id_arg_missing() {
        let args = serde_json::json!({});
        assert_eq!(super::get_doc_id_arg(&args), None);
    }
}
