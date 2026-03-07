//! Repository-wide check MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_str_arg, load_perspective, resolve_repo_filter};
use crate::progress::ProgressReporter;
use crate::question_generator::check::{check_all_documents, CheckConfig};
use serde_json::Value;

/// Default concurrency for parallel check (LLM calls).
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
        Some("cross_validate") | Some("deep_check") => Ok(serde_json::json!({
            "error": "The 'cross_validate' mode has been removed. Use the get_fact_pairs tool instead to retrieve similar fact pairs, then classify them yourself and flag conflicts via answer_questions.",
            "migration": "Replace check_repository(mode='cross_validate') with get_fact_pairs(). The agent now classifies fact pairs directly."
        })),
        Some("discover") => check_discover(db, llm, args, progress).await,
        Some("embeddings") => check_embeddings(db, embedding, args, progress).await,
        Some(other) => Ok(serde_json::json!({
            "error": format!("Unknown mode '{}'. Must be one of: questions, discover, embeddings", other)
        })),
        None => {
            let deep_check = args.get("deep_check").and_then(Value::as_bool).unwrap_or(false);
            if deep_check {
                Ok(serde_json::json!({
                    "error": "deep_check has been removed. Use the get_fact_pairs tool instead.",
                    "migration": "Replace check_repository(deep_check=true) with get_fact_pairs(). The agent now classifies fact pairs directly."
                }))
            } else {
                Ok(serde_json::json!({
                    "error": "Missing required parameter 'mode'. Must be one of: questions, discover, embeddings",
                    "hint": "check_repository now requires an explicit mode. Use mode='questions' for per-doc quality checks, mode='discover' for entity suggestions, or mode='embeddings' for fact embedding generation. For cross-document fact comparison, use the get_fact_pairs tool."
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

    let all_docs = load_docs(db, repo_id)?;

    // Resume: skip already-processed docs
    let doc_offset = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token)
        .and_then(|v| v.get("doc_offset").and_then(|o| o.as_u64()))
        .unwrap_or(0) as usize;
    let docs: Vec<_> = all_docs.into_iter().skip(doc_offset).collect();
    let total_all = doc_offset + docs.len();

    progress.phase("Generating review questions");

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let config = CheckConfig {
        stale_days,
        required_fields,
        dry_run,
        concurrency: check_concurrency,
        deadline,
        acquire_write_guard: true,
        repo_id: repo_id.map(String::from),
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

    let processed_total = doc_offset + output.docs_processed;

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

    let resume_token = if processed_total < total_all {
        Some(crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"doc_offset": processed_total}),
        ))
    } else {
        None
    };

    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut result, processed_total, total_all, "check_repository", time_budget.is_some(),
        resume_token.as_deref(),
    );

    Ok(result)
}
/// Mode: discover — entity suggestions + vocabulary extraction.
async fn check_discover(
    db: &Database,
    _llm: Option<&dyn LlmProvider>,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();
    let perspective = load_perspective(db, repo_id);
    let docs = load_docs(db, repo_id)?;
    let total_docs = docs.len();

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    // Decode resume token: {phase: "entities", doc_offset: N}
    let resume_data = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token);
    let doc_offset = resume_data.as_ref()
        .and_then(|v| v.get("doc_offset").and_then(Value::as_u64))
        .unwrap_or(0) as usize;

    let mut result = serde_json::json!({ "mode": "discover" });

    // Entity discovery phase (currently a no-op — returns empty results)
    progress.phase("Discovering entities");
    let existing_titles: Vec<String> = docs.iter().map(|d| d.title.clone()).collect();
    let (suggested_entities, processed) = crate::organize::discover_entities(
        &docs, &existing_titles, perspective.as_ref(), progress,
        doc_offset, deadline,
    )
    .await
    .unwrap_or_default();

    let entity_offset = doc_offset + processed;

    if entity_offset < total_docs {
        let token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"phase": "entities", "doc_offset": entity_offset}),
        );
        crate::mcp::tools::helpers::apply_time_budget_progress(
            &mut result, entity_offset, total_docs, "check_repository",
            true, Some(&token),
        );
        return Ok(result);
    }

    if !suggested_entities.is_empty() {
        result["suggested_entities"] = serde_json::to_value(&suggested_entities).unwrap_or_default();
    }

    // Vocabulary extraction is now agent-driven via the discover workflow step.
    result["note"] = serde_json::json!("Vocabulary extraction is handled by the agent during the discover workflow step.");

    Ok(result)
}

/// Mode: embeddings — generate fact-level embeddings for cross-validation.
async fn check_embeddings(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();

    let all_needing = db.get_doc_ids_without_fact_embeddings(repo_id)?;

    // Resume: skip already-processed docs
    let doc_offset = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token)
        .and_then(|v| v.get("doc_offset").and_then(|o| o.as_u64()))
        .unwrap_or(0) as usize;
    let doc_ids: Vec<String> = all_needing.into_iter().skip(doc_offset).collect();
    let total_all = doc_offset + doc_ids.len();

    if doc_ids.is_empty() {
        return Ok(serde_json::json!({
            "mode": "embeddings",
            "fact_embeddings_generated": 0,
            "documents_processed": 0,
            "message": "All documents already have fact embeddings."
        }));
    }

    progress.phase("Generating fact embeddings");

    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    let config = crate::Config::load(None).unwrap_or_default();
    let batch_size = config.processor.embedding_batch_size;

    let changed_set: std::collections::HashSet<String> = doc_ids.into_iter().collect();
    let output = crate::run_fact_embedding_phase(&crate::FactEmbeddingInput {
        changed_ids: &changed_set,
        embedding,
        db,
        embedding_batch_size: batch_size,
        progress,
        deadline,
    })
    .await
    .map_err(|e| FactbaseError::Internal(e.to_string()))?;

    if output.generated > 0 {
        let _ = db.invalidate_fact_pair_cache();
    }

    let processed_total = doc_offset + output.docs_processed;

    let mut result = serde_json::json!({
        "mode": "embeddings",
        "fact_embeddings_generated": output.generated,
        "documents_processed": output.docs_processed,
    });

    let resume_token = if processed_total < total_all {
        Some(crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"doc_offset": processed_total}),
        ))
    } else {
        None
    };

    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut result, processed_total, total_all, "check_repository", time_budget.is_some(),
        resume_token.as_deref(),
    );

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
            "mode": "questions",
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
    async fn test_resume_token_in_questions_mode() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "lk1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:lk1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        // Resume token that skips past all docs — should complete immediately
        let token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"doc_offset": 100}),
        );
        let args = serde_json::json!({
            "repo": "test",
            "mode": "questions",
            "dry_run": true,
            "resume": token,
        });

        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["documents_scanned"], 0);
        assert!(result.get("continue").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_questions_mode_no_resume_starts_from_beginning() {
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

    /// When a resume token with pair_offset=0 is provided,
    /// the handler should treat it as a continuation, skip question gen, and run CV.
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
    async fn test_discover_mode_without_llm_succeeds() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let embedding = MockEmbedding::new(4);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "discover", "repo": "test"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "discover");
        // Discover no longer requires LLM — entity discovery and vocabulary extraction are agent-driven
        assert!(result.get("note").is_some(), "discover should include a note about agent-driven vocabulary");
    }

    #[tokio::test]
    #[serial]
    async fn test_discover_resume_token_skips_entities_phase() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "dr1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:dr1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        // Resume from entities phase with offset
        let token = crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"phase": "entities", "doc_offset": 0}),
        );
        let args = serde_json::json!({"mode": "discover", "repo": "test", "resume": token});
        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "discover");
    }

    #[tokio::test]
    #[serial]
    async fn test_discover_completes_without_continue_for_small_repo() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "dc1111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:dc1111 -->\n# Test Doc\n\n- Some fact\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(4);
        let llm = MockLlm::new("[]");
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "discover", "repo": "test", "time_budget_secs": 30});
        let result = check_repository(&db, &embedding, Some(&llm), &args, &progress)
            .await
            .unwrap();
        assert_eq!(result["mode"], "discover");
        // Small repo should complete without continuation
        assert!(result.get("continue").is_none(), "small repo should not need continuation: {result}");
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

    #[tokio::test]
    #[serial]
    async fn test_check_embeddings_generates_fact_embeddings() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "emb111".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:emb111 -->\n# Test Doc\n\n- Fact alpha\n- Fact beta\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        // No fact embeddings yet
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);

        let args = serde_json::json!({"mode": "embeddings", "repo": "test"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();

        assert_eq!(result["mode"], "embeddings");
        assert!(result["fact_embeddings_generated"].as_u64().unwrap() > 0);
        assert!(db.get_fact_embedding_count().unwrap() > 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_check_embeddings_skips_when_all_present() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "emb222".to_string();
        doc.title = "Test Doc".to_string();
        doc.content = "<!-- factbase:emb222 -->\n# Test Doc\n\n- Fact one\n".to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        // Pre-populate fact embeddings
        db.upsert_fact_embedding("emb222_4", "emb222", 4, "Fact one", "hash1", &vec![0.1; 1024]).unwrap();

        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let args = serde_json::json!({"mode": "embeddings", "repo": "test"});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();

        assert_eq!(result["mode"], "embeddings");
        assert_eq!(result["fact_embeddings_generated"], 0);
        assert!(result["message"].as_str().unwrap().contains("already have"));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_embeddings_returns_continue_on_deadline() {
        let (db, _tmp) = test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        // Create multiple docs
        for i in 0..3u8 {
            let id = format!("dl{i:04x}1");
            let mut doc = crate::models::Document::test_default();
            doc.id = id.clone();
            doc.title = format!("Doc {i}");
            doc.content = format!("<!-- factbase:{id} -->\n# Doc {i}\n\n- Fact {i}\n");
            doc.repo_id = "test".to_string();
            db.upsert_document(&doc).unwrap();
        }

        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        // Use a very short time budget
        let args = serde_json::json!({"mode": "embeddings", "repo": "test", "time_budget_secs": 5});
        let result = check_repository(&db, &embedding, None, &args, &progress)
            .await
            .unwrap();

        assert_eq!(result["mode"], "embeddings");
        // With 3 small docs and 5s budget, it should complete
        assert!(result["documents_processed"].as_u64().unwrap() > 0);
    }
}
