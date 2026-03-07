//! Repository-wide check MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_str_arg, load_perspective, resolve_repo_filter};
use crate::progress::ProgressReporter;
use crate::question_generator::check::{check_all_documents, CheckConfig};
use serde_json::Value;

/// Default concurrency for parallel check.
const LINT_CONCURRENCY: usize = 5;

/// Load active (non-deleted) documents for the given repo scope.
fn load_docs(db: &Database, repo_id: Option<&str>) -> Result<Vec<crate::models::Document>, FactbaseError> {
    match repo_id {
        Some(rid) => Ok(db
            .get_documents_for_repo(rid)?
            .into_values()
            .filter(|d| !d.is_deleted)
            .collect()),
        None => {
            let mut all = Vec::new();
            for repo in db.list_repositories()? {
                all.extend(
                    db.get_documents_for_repo(&repo.id)?
                        .into_values()
                        .filter(|d| !d.is_deleted),
                );
            }
            Ok(all)
        }
    }
}

/// Runs rule-based quality checks across documents in a repository.
///
/// When `doc_id` is provided, checks just that one document.
/// Otherwise checks all documents in the repository.
///
/// This is a single-mode tool — no `mode` parameter needed.
/// For cross-document fact comparison, use `get_fact_pairs`.
/// For fact embeddings, use `scan_repository`.
pub async fn check_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg(args, "doc_id");

    // If doc_id is provided, check just that one document
    if doc_id.is_some() {
        return super::generate_questions(db, embedding, args).await;
    }

    // Handle deprecated mode parameter gracefully
    if let Some(mode) = get_str_arg(args, "mode") {
        match mode {
            "questions" => {} // This is what we do now — proceed
            "cross_validate" | "deep_check" => return Ok(serde_json::json!({
                "error": "The 'cross_validate' mode has been removed. Use the get_fact_pairs tool instead.",
                "migration": "Replace check_repository(mode='cross_validate') with get_fact_pairs()."
            })),
            "discover" => return Ok(serde_json::json!({
                "error": "The 'discover' mode has been removed. Entity discovery is now agent-driven via the update workflow.",
                "migration": "Use the update workflow's discover step, or manually scan documents for entity candidates."
            })),
            "embeddings" => return Ok(serde_json::json!({
                "error": "The 'embeddings' mode has been removed. Fact embeddings are generated during scan_repository.",
                "migration": "Call scan_repository to generate both document and fact embeddings."
            })),
            other => return Ok(serde_json::json!({
                "error": format!("Unknown mode '{}'. check_repository no longer requires a mode parameter — it runs rule-based quality checks directly.", other)
            })),
        }
    }

    check_questions(db, embedding, args, progress).await
}

/// Run per-document rule-based quality checks.
async fn check_questions(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();
    let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);

    let config_file = crate::Config::load(None);
    let check_concurrency = config_file
        .as_ref()
        .map(|c| c.processor.check_concurrency)
        .unwrap_or(LINT_CONCURRENCY);

    let perspective = load_perspective(db, repo_id);
    let stale_days = perspective.as_ref().and_then(|p| p.review.as_ref()).and_then(|r| r.stale_days).unwrap_or(365) as i64;
    let required_fields = perspective.as_ref().and_then(|p| p.review.as_ref()).and_then(|r| r.required_fields.clone());

    let all_docs = load_docs(db, repo_id)?;

    progress.phase("Generating review questions");

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline: None, // No time-boxing — rule-based checks are fast
        acquire_write_guard: true,
        repo_id: repo_id.map(String::from),
    };

    let output = check_all_documents(&all_docs, db, embedding, &config, progress).await?;
    let results = &output.results;

    let docs_with_questions = results.iter().filter(|r| r.new_questions > 0).count();
    let docs_clean = results.iter().filter(|r| r.new_questions == 0 && r.existing_unanswered == 0).count();
    let total_new: usize = results.iter().map(|r| r.new_questions).sum();
    let total_pruned: usize = results.iter().map(|r| r.pruned_questions).sum();
    let total_existing: usize = results.iter().map(|r| r.existing_unanswered + r.existing_answered).sum();
    let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
    let total_suppressed: usize = results.iter().map(|r| r.suppressed_by_review).sum();
    let deferred_count = db.count_deferred_questions(repo_id).unwrap_or(0);

    // Build question type breakdown by parsing generated questions from documents
    let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for doc in &all_docs {
        if let Some(questions) = crate::processor::parse_review_queue(&doc.content) {
            for q in &questions {
                if !q.answered {
                    *type_counts.entry(q.question_type.as_str().to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let details: Vec<Value> = results.iter()
        .filter(|r| r.new_questions > 0 || r.pruned_questions > 0)
        .map(|r| serde_json::json!({
            "doc_id": r.doc_id, "doc_title": r.doc_title,
            "new_questions": r.new_questions, "pruned_questions": r.pruned_questions,
        }))
        .collect();

    Ok(serde_json::json!({
        "documents_scanned": output.docs_processed,
        "documents_with_new_questions": docs_with_questions,
        "documents_clean": docs_clean,
        "total_questions_generated": total_new + total_existing,
        "new_unanswered": total_new,
        "already_in_queue": total_existing,
        "pruned_stale": total_pruned,
        "skipped_reviewed": total_skipped,
        "suppressed_by_prior_answers": total_suppressed,
        "deferred_count": deferred_count,
        "questions_by_type": type_counts,
        "dry_run": dry_run,
        "details": details,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::progress::ProgressReporter;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_check_repository_runs_quality_checks() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:aaa111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
        // No mode field in output
        assert!(result.get("mode").is_none());
        // No paging fields
        assert!(result.get("continue").is_none());
        assert!(result.get("resume").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_deprecated_mode_questions_still_works() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "questions", "repo": "test", "dry_run": true});
        let result = check_repository(&db, &embedding, &args, &progress)
            .await
            .unwrap();
        // Should still work (questions is what we do now)
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_deprecated_modes_return_migration() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        for mode in &["cross_validate", "discover", "embeddings"] {
            let args = serde_json::json!({"mode": mode});
            let result = check_repository(&db, &embedding, &args, &progress)
                .await
                .unwrap();
            assert!(result.get("error").is_some(), "deprecated mode '{mode}' should return error");
            assert!(result.get("migration").is_some(), "deprecated mode '{mode}' should return migration hint");
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_unknown_mode_returns_error() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "bogus"});
        let result = check_repository(&db, &embedding, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("error").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_doc_id_bypasses_mode() {
        let (db, _tmp) = test_db();
        let repo_dir = tempfile::TempDir::new().unwrap();
        let repo_path = repo_dir.path();
        crate::database::tests::test_repo_in_db(&db, "test", repo_path);

        let doc_path = repo_path.join("test.md");
        std::fs::write(&doc_path, "<!-- factbase:db1111 -->\n# Test Doc\n\n- Some fact\n").unwrap();

        let mut doc = crate::models::Document::test_default();
        doc.id = "db1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:db1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        doc.file_path = "test.md".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"doc_id": "db1111"});
        let result = check_repository(&db, &embedding, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("error").is_none(), "doc_id should work without mode: {result}");
    }

    #[tokio::test]
    #[serial]
    async fn test_no_mode_no_doc_id_runs_checks() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        // No mode, no doc_id → should run quality checks (not error)
        let args = serde_json::json!({"repo": "test"});
        let result = check_repository(&db, &embedding, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
    }
}
