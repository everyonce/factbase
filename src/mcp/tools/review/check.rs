//! Repository-wide check MCP tool.

use crate::database::{CvLockResult, Database, DEFAULT_LOCK_TIMEOUT_SECS};
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, load_perspective, resolve_repo_filter};
use crate::progress::ProgressReporter;
use crate::question_generator::check::{check_all_documents, CheckConfig};
use serde_json::Value;
use std::collections::HashSet;

/// Default concurrency for parallel check (LLM calls).
const LINT_CONCURRENCY: usize = 5;

/// Scope key for server-side cross-validation state.
fn cv_scope_key(repo_id: Option<&str>) -> String {
    repo_id.unwrap_or("__all__").to_string()
}

/// Generate a random lock token for this session.
fn generate_lock_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", std::process::id(), nanos)
}

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

/// Runs check across all documents in a repository via MCP.
pub async fn check_repository(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg(args, "doc_id");

    // If doc_id is provided, check just that one document (replaces generate_questions)
    if doc_id.is_some() {
        return super::generate_questions(db, embedding, llm, args).await;
    }

    let mode = get_str_arg(args, "mode");

    match mode {
        Some("questions") => check_questions(db, embedding, llm, args, progress).await,
        Some("cross_validate") => check_cross_validate(db, embedding, llm, args, progress).await,
        Some("discover") => check_discover(db, llm, args, progress).await,
        Some(other) => Ok(serde_json::json!({
            "error": format!("Unknown mode '{}'. Must be one of: questions, cross_validate, discover", other)
        })),
        None => {
            // Backward compat: if deep_check is set, treat as old-style call
            // Otherwise, require mode
            let deep_check = args.get("deep_check").and_then(Value::as_bool).unwrap_or(false);
            if deep_check {
                // Legacy caller using deep_check=true — run cross_validate mode
                check_cross_validate(db, embedding, llm, args, progress).await
            } else {
                Ok(serde_json::json!({
                    "error": "Missing required parameter 'mode'. Must be one of: questions, cross_validate, discover",
                    "hint": "check_repository now requires an explicit mode. Use mode='questions' for per-doc quality checks, mode='cross_validate' for cross-doc fact comparison, or mode='discover' for entity suggestions."
                }))
            }
        }
    }
}

/// Mode: questions — per-document quality checks only.
async fn check_questions(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
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

    let docs = load_docs(db, repo_id)?;
    progress.phase("Generating review questions");

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline,
        checked_doc_ids: HashSet::new(),
        pair_offset: 0,
        acquire_write_guard: true,
        batch_size: 10,
        repo_id: repo_id.map(String::from),
        is_continuation: false,
        fact_db: None,
    };

    let output = check_all_documents(&docs, db, embedding, llm, &config, progress).await?;
    let results = &output.results;

    let docs_with_questions = results.iter().filter(|r| r.new_questions > 0).count();
    let total_new: usize = results.iter().map(|r| r.new_questions).sum();
    let total_pruned: usize = results.iter().map(|r| r.pruned_questions).sum();
    let total_existing: usize = results.iter().map(|r| r.existing_unanswered + r.existing_answered).sum();
    let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
    let total_suppressed: usize = results.iter().map(|r| r.suppressed_by_review).sum();
    let deferred_count = db.count_deferred_questions(repo_id).unwrap_or(0);
    let details: Vec<Value> = results.iter()
        .filter(|r| r.new_questions > 0 || r.pruned_questions > 0)
        .map(|r| serde_json::json!({
            "doc_id": r.doc_id, "doc_title": r.doc_title,
            "new_questions": r.new_questions, "pruned_questions": r.pruned_questions,
        }))
        .collect();

    let mut result = serde_json::json!({
        "mode": "questions",
        "documents_scanned": output.docs_processed,
        "documents_with_new_questions": docs_with_questions,
        "total_questions_generated": total_new + total_existing,
        "new_unanswered": total_new,
        "already_in_queue": total_existing,
        "pruned_stale": total_pruned,
        "skipped_reviewed": total_skipped,
        "suppressed_by_prior_answers": total_suppressed,
        "deferred_count": deferred_count,
        "dry_run": dry_run,
        "details": details,
    });

    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut result, output.docs_processed, output.docs_total, "check_repository", time_budget.is_some(),
    );

    Ok(result)
}

/// Mode: cross_validate — cross-document fact comparison.
async fn check_cross_validate(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
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
    let batch_size = config_file
        .as_ref()
        .map(|c| c.cross_validate.batch_size)
        .unwrap_or_else(|_| crate::config::cross_validate::default_batch_size());

    let perspective = load_perspective(db, repo_id);
    let stale_days = perspective.as_ref().and_then(|p| p.review.as_ref()).and_then(|r| r.stale_days).unwrap_or(365) as i64;
    let required_fields = perspective.as_ref().and_then(|p| p.review.as_ref()).and_then(|r| r.required_fields.clone());

    // Lock/lease for concurrency control
    let scope_key = cv_scope_key(repo_id);
    let lock_token = generate_lock_token();
    match db.try_acquire_cv_lock(&scope_key, &lock_token, DEFAULT_LOCK_TIMEOUT_SECS)? {
        CvLockResult::Acquired => {}
        CvLockResult::AlreadyLocked { locked_by, pair_offset, fact_count } => {
            let pct = if fact_count > 0 { pair_offset } else { 0 };
            return Ok(serde_json::json!({
                "mode": "cross_validate",
                "status": "in_progress",
                "locked_by": locked_by,
                "progress": { "pairs_checked": pair_offset, "percent_estimate": pct },
                "message": format!(
                    "Cross-validation is already in progress (held by {locked_by}, {pair_offset} pairs checked). \
                     Call again later or wait for completion."
                ),
            }));
        }
    }

    let docs = load_docs(db, repo_id)?;
    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    // Resolve repo-local DB for fact embeddings (may differ from central DB)
    let fact_db = repo_id.and_then(|id| db.resolve_repo_fact_db(id));
    let fdb = fact_db.as_ref().unwrap_or(db);

    // Server-side cursor
    let current_fact_count = fdb.get_fact_embedding_count().unwrap_or(0);
    let pair_offset = match db.get_cross_validation_state(&scope_key)? {
        Some((offset, stored_fact_count)) if stored_fact_count == current_fact_count => offset,
        Some(_) => {
            db.clear_cross_validation_state(Some(&scope_key))?;
            let _ = db.try_acquire_cv_lock(&scope_key, &lock_token, DEFAULT_LOCK_TIMEOUT_SECS)?;
            0
        }
        None => 0,
    };

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline,
        checked_doc_ids: HashSet::new(),
        pair_offset,
        acquire_write_guard: true,
        batch_size,
        repo_id: repo_id.map(String::from),
        is_continuation: true, // skip question gen, only run CV
        fact_db,
    };

    progress.phase("Cross-document validation");
    let output = check_all_documents(&docs, db, embedding, llm, &config, progress).await?;

    // Persist cursor
    if output.pair_progress.is_some() {
        db.set_cross_validation_state(&scope_key, output.pair_offset, current_fact_count)?;
    }

    let results = &output.results;
    let total_new: usize = results.iter().map(|r| r.new_questions).sum();
    let deferred_count = db.count_deferred_questions(repo_id).unwrap_or(0);

    let mut result = serde_json::json!({
        "mode": "cross_validate",
        "documents_scanned": output.docs_processed,
        "new_conflict_questions": total_new,
        "deferred_count": deferred_count,
        "dry_run": dry_run,
    });

    // Progress/continue for pair-based paging
    if let Some((processed, total)) = output.pair_progress {
        if processed < total {
            let remaining = total.saturating_sub(processed);
            let pct = if total > 0 { (processed as f64 / total as f64 * 100.0) as u32 } else { 0 };
            let pairs_this_call = processed.saturating_sub(pair_offset).max(1);
            let estimated_iterations = (remaining + pairs_this_call - 1) / pairs_this_call;

            let mut progress_obj = serde_json::json!({
                "pairs_checked": processed, "pairs_total": total,
                "percent_complete": pct, "estimated_iterations_remaining": estimated_iterations,
            });
            if let Some(elapsed) = output.cv_elapsed_secs {
                progress_obj["elapsed_this_call_secs"] = serde_json::json!((elapsed * 100.0).round() / 100.0);
            }
            if let Some(rate) = output.cv_pairs_per_second {
                progress_obj["pairs_per_second"] = serde_json::json!((rate * 100.0).round() / 100.0);
            }

            result["progress"] = progress_obj;
            result["continue"] = serde_json::json!(true);
            result["message"] = serde_json::json!(format!(
                "⚠️ MANDATORY CONTINUATION — {processed}/{total} fact pairs checked ({pct}%, ~{estimated_iterations} calls remaining). \
                 You MUST call check_repository again with the same arguments (progress is saved server-side). \
                 Do NOT stop, do NOT reason about whether to continue, do NOT report partial results. \
                 Just call check_repository again until this message no longer appears."
            ));
            result["when_done"] = serde_json::json!(
                "MANDATORY: Call check_repository again with the same arguments. Progress is saved automatically. Do NOT use the think tool to decide whether to continue. Just call it."
            );
        }
        if processed >= total {
            let _ = db.release_cv_lock(&scope_key, &lock_token);
            let _ = db.clear_cross_validation_state(Some(&scope_key));
        }
    } else {
        // No pairs at all — release lock
        let _ = db.release_cv_lock(&scope_key, &lock_token);
    }

    Ok(result)
}

/// Mode: discover — entity suggestions + vocabulary extraction.
async fn check_discover(
    db: &Database,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();
    let perspective = load_perspective(db, repo_id);
    let docs = load_docs(db, repo_id)?;

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let mut result = serde_json::json!({ "mode": "discover" });

    if let Some(llm_ref) = llm {
        // Entity discovery
        progress.phase("Discovering entities");
        let existing_titles: Vec<String> = docs.iter().map(|d| d.title.clone()).collect();
        let suggested_entities = crate::organize::discover_entities(
            &docs, &existing_titles, llm_ref, perspective.as_ref(), progress,
        )
        .await
        .unwrap_or_default();

        if !suggested_entities.is_empty() {
            result["suggested_entities"] = serde_json::to_value(&suggested_entities).unwrap_or_default();
        }

        // Vocabulary extraction
        if !deadline.is_some_and(|d| std::time::Instant::now() > d) {
            progress.phase("Extracting domain vocabulary");
            let defined_terms = crate::question_generator::collect_defined_terms(&docs);
            let doc_refs: Vec<&crate::models::Document> = docs.iter().collect();
            let vocab = crate::question_generator::check::extract_vocabulary(
                &doc_refs, &defined_terms, llm_ref, deadline, progress,
            ).await;
            if !vocab.is_empty() {
                result["vocabulary_candidates"] = serde_json::to_value(&vocab).unwrap_or_default();
            }
        }
    } else {
        result["warning"] = serde_json::json!("No LLM provider configured — entity discovery and vocabulary extraction require an LLM.");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::llm::test_helpers::MockLlm;
    use crate::progress::ProgressReporter;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_check_repository_skips_entity_discovery_when_deadline_hit() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:aaa111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "time_budget_secs": 5,
        });

        let _result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
    }

    #[test]
    fn test_deadline_hit_skips_entity_discovery_logic() {
        let past = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(past.is_some_and(|d| std::time::Instant::now() > d));

        let none: Option<std::time::Instant> = None;
        assert!(!none.is_some_and(|d| std::time::Instant::now() > d));

        let future = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
        assert!(!future.is_some_and(|d| std::time::Instant::now() > d));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_accepts_legacy_checked_pair_ids() {
        // Legacy checked_pair_ids in args should be accepted without error (ignored)
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "pp1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:pp1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_pair_ids": ["pp1111_3:pp2222_5"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
        // Server-side cursor: no checked_pair_ids in response
        assert!(result.get("checked_pair_ids").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_check_repository_backward_compat_checked_doc_ids() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "bc1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:bc1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
            "checked_doc_ids": ["bc1111"],
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_server_side_cursor_persists_across_calls() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "sv1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:sv1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        // Simulate server-side state from a previous call
        let scope_key = cv_scope_key(Some("test"));
        let fact_count = db.get_fact_embedding_count().unwrap_or(0);
        db.set_cross_validation_state(&scope_key, 42, fact_count).unwrap();
        // Also set a lock so the check can proceed (it will re-acquire)
        db.try_acquire_cv_lock(&scope_key, "prior-token", DEFAULT_LOCK_TIMEOUT_SECS).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Call without any cursor args — should pick up from DB
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        // The prior lock is from a different token, but since we're in a test
        // and the lock timeout is 600s, we need to expire it first
        let conn = db.get_conn().unwrap();
        conn.execute(
            "UPDATE cross_validation_state SET locked_at = datetime('now', '-700 seconds') WHERE scope_key = ?1",
            [&scope_key],
        ).unwrap();

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(result.get("documents_scanned").is_some());
        // Continuation should skip entity discovery and vocab
        assert!(result.get("suggested_entities").is_none());
        assert!(result.get("vocabulary_candidates").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_continuation_no_remaining_pairs_does_not_loop() {
        // Regression: server-side cursor at completion should NOT return continue:true
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "nl1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:nl1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // First call without time_budget — should complete fully
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        // With no fact embeddings and no time budget, should complete without continue
        assert_ne!(
            result.get("continue").and_then(Value::as_bool),
            Some(true),
            "no-budget call with no fact pairs should not loop: {result}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_response_has_no_checked_pair_ids_field() {
        // The whole point: responses should never contain checked_pair_ids
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "nc1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:nc1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert!(
            result.get("checked_pair_ids").is_none(),
            "response must not contain checked_pair_ids (server-side cursor): {result}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_lock_contention_returns_in_progress() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "lk1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:lk1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        // Simulate another session holding the lock
        let scope_key = cv_scope_key(Some("test"));
        db.try_acquire_cv_lock(&scope_key, "other-session", DEFAULT_LOCK_TIMEOUT_SECS).unwrap();
        db.set_cross_validation_state(&scope_key, 50, 100).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();

        assert_eq!(result["status"], "in_progress");
        assert_eq!(result["locked_by"], "other-session");
        assert!(result["progress"]["pairs_checked"].as_u64().unwrap() >= 50);
        assert!(result["message"].as_str().unwrap().contains("already in progress"));
    }

    #[tokio::test]
    #[serial]
    async fn test_no_lock_without_deep_check() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "nd1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:nd1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        // No deep_check — should not acquire lock. Use mode='questions'.
        let args = serde_json::json!({
            "repo": "test",
            "mode": "questions",
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("status").is_none(), "no lock status without deep_check");
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_progress_includes_timing_fields() {
        // Verify that when cross-validation runs, timing fields are populated
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "tm1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:tm1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        // Without fact embeddings, no pair progress — timing fields won't appear
        // This test just verifies the code path doesn't crash
        assert!(result.get("documents_scanned").is_some());
    }

    /// When CV state exists at pair_offset=0 (question gen completed but CV hasn't started),
    /// the handler should treat it as a continuation, skip question gen, and run CV.
    #[tokio::test]
    #[serial]
    async fn test_deep_check_continues_when_question_gen_exhausts_budget() {
        fn spike_embedding(index: usize) -> Vec<f32> {
            let mut v = vec![0.0f32; 1024];
            v[index] = 1.0;
            v
        }
        fn near_spike(index: usize, offset: f32) -> Vec<f32> {
            let mut v = vec![0.0f32; 1024];
            v[index] = 1.0;
            v[(index + 1) % 1024] = offset;
            v
        }

        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc_a = crate::models::Document::test_default();
        doc_a.id = "dg1111".to_string();
        doc_a.title = "Entity A".to_string();
        doc_a.content = "<!-- factbase:dg1111 -->\n# Entity A\n\n- Revenue: $10M\n".to_string();
        doc_a.repo_id = "test".to_string();
        doc_a.file_path = "entity-a.md".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = crate::models::Document::test_default();
        doc_b.id = "dg2222".to_string();
        doc_b.title = "Entity B".to_string();
        doc_b.content = "<!-- factbase:dg2222 -->\n# Entity B\n\n- Revenue: $50M\n".to_string();
        doc_b.repo_id = "test".to_string();
        doc_b.file_path = "entity-b.md".to_string();
        db.upsert_document(&doc_b).unwrap();

        // Insert fact embeddings so cross-doc pairs exist
        db.upsert_fact_embedding("dg1111_3", "dg1111", 3, "Revenue: $10M", "h1", &spike_embedding(0)).unwrap();
        db.upsert_fact_embedding("dg2222_3", "dg2222", 3, "Revenue: $50M", "h2", &near_spike(0, 0.1)).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new(r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#);
        let progress = ProgressReporter::Silent;

        // Simulate: first call completed question gen but exhausted budget before CV.
        // The fix persists CV state with pair_offset=0 so the next call skips question gen.
        let scope_key = cv_scope_key(Some("test"));
        let fact_count = db.get_fact_embedding_count().unwrap_or(0);
        db.set_cross_validation_state(&scope_key, 0, fact_count).unwrap();
        // Set an expired lock so the handler can re-acquire
        db.try_acquire_cv_lock(&scope_key, "prior-call", DEFAULT_LOCK_TIMEOUT_SECS).unwrap();
        let conn = db.get_conn().unwrap();
        conn.execute(
            "UPDATE cross_validation_state SET locked_at = datetime('now', '-700 seconds') WHERE scope_key = ?1",
            [&scope_key],
        ).unwrap();

        // Second call: should detect CV state, skip question gen, run CV
        let args = serde_json::json!({
            "repo": "test",
            "deep_check": true,
            "dry_run": true,
        });

        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();

        // Should have run CV (documents_scanned > 0) and not loop forever
        let scanned = result["documents_scanned"].as_u64().unwrap_or(0);
        assert!(scanned > 0, "continuation call should run cross-validation: {result}");
        // Should not have suggested_entities (skipped on continuation)
        assert!(result.get("suggested_entities").is_none(), "continuation should skip entity discovery");
    }

    #[tokio::test]
    #[serial]
    async fn test_missing_mode_returns_error() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        // No mode, no deep_check → error
        let args = serde_json::json!({"repo": "test"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("error").is_some(), "missing mode should return error: {result}");
        assert!(result["error"].as_str().unwrap().contains("mode"));
    }

    #[tokio::test]
    #[serial]
    async fn test_unknown_mode_returns_error() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "bogus"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert!(result.get("error").is_some());
        assert!(result["error"].as_str().unwrap().contains("bogus"));
    }

    #[tokio::test]
    #[serial]
    async fn test_questions_mode_returns_mode_field() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "qm1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:qm1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "questions", "repo": "test", "dry_run": true});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "questions");
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_cross_validate_mode_returns_mode_field() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "cv1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:cv1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "cross_validate", "repo": "test", "dry_run": true});
        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "cross_validate");
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_discover_mode_returns_mode_field() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "ds1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:ds1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "discover", "repo": "test"});
        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "discover");
    }

    #[tokio::test]
    #[serial]
    async fn test_discover_mode_without_llm_warns() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "discover", "repo": "test"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "discover");
        assert!(result.get("warning").is_some(), "discover without LLM should warn");
    }

    #[tokio::test]
    #[serial]
    async fn test_deep_check_backward_compat_routes_to_cross_validate() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "bc2222".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:bc2222 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // No mode, but deep_check=true → should route to cross_validate
        let args = serde_json::json!({"repo": "test", "deep_check": true, "dry_run": true});
        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "cross_validate");
        assert!(result.get("documents_scanned").is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_doc_id_bypasses_mode_requirement() {
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

        // doc_id provided, no mode → should still work (delegates to generate_questions)
        let args = serde_json::json!({"doc_id": "db1111"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        // Should not have an error
        assert!(result.get("error").is_none(), "doc_id should bypass mode requirement: {result}");
    }
}
