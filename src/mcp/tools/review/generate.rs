//! Review question generation MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::helpers::resolve_doc_path;
use crate::mcp::tools::{get_bool_arg, get_str_arg, resolve_repo_filter};
use crate::patterns::has_corruption_artifacts;
use crate::processor::{append_review_questions, content_hash, parse_review_queue};
use crate::question_generator::{
    collect_defined_terms, filter_sequential_conflicts,
    generate_ambiguous_questions_with_type, generate_conflict_questions,
    generate_duplicate_questions, generate_duplicate_entry_questions, generate_missing_questions,
    generate_precision_questions, generate_stale_questions, generate_temporal_questions,
};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use tracing::{instrument, warn};

use super::format_question_json;

/// Generates review questions for one or all documents.
///
/// When `doc_id` is provided, checks a single document. When omitted,
/// iterates all documents with time-boxing support.
///
/// Analyzes document content for missing temporal tags, conflicts,
/// missing sources, ambiguous facts, stale information, and duplicates.
/// When embedding provider is available, also runs cross-document
/// fact validation to detect conflicts with other documents.
/// Appends new questions to the document's review queue.
///
/// # Arguments (from JSON)
/// - `doc_id` (optional): Document ID (6-char hex). If omitted, checks all documents.
/// - `dry_run` (optional): Preview questions without modifying file (default: false)
/// - `time_budget_secs` (optional): Time budget in seconds (5-600) for multi-doc mode.
///
/// # Returns
/// JSON with results. For multi-doc mode, may include `continue: true` if
/// time budget was reached before processing all documents.
#[instrument(name = "mcp_generate_questions", skip(db, embedding, args))]
pub async fn generate_questions(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let doc_id_opt = get_str_arg(args, "doc_id");
    let dry_run = get_bool_arg(args, "dry_run", false);

    match doc_id_opt {
        Some(id) => generate_questions_single(db, embedding, id, dry_run).await,
        None => generate_questions_all(db, embedding, args, dry_run).await,
    }
}

/// Single-doc mode: generate questions for one document.
async fn generate_questions_single(
    db: &Database,
    _embedding: &dyn EmbeddingProvider,
    doc_id: &str,
    dry_run: bool,
) -> Result<Value, FactbaseError> {

    // Get the document
    let doc = db.require_document(&doc_id)?;

    // Skip deleted documents
    if doc.is_deleted {
        return Err(FactbaseError::not_found(format!(
            "Document has been deleted: {doc_id}"
        )));
    }

    // Skip documents with corruption artifacts from failed apply_review_answers
    if has_corruption_artifacts(&doc.content) {
        return Ok(serde_json::json!({
            "doc_id": doc_id,
            "doc_title": doc.title,
            "questions_generated": 0,
            "questions": [],
            "dry_run": dry_run,
            "corrupted": true,
            "message": "Document contains corruption artifacts from a failed apply_review_answers run — rebuild content before checking"
        }));
    }

    // Prefer fresh content from disk over potentially stale database content.
    // apply_review_answers writes reviewed markers to files but doesn't always
    // update the database, so the DB content may lack those markers.
    let file_path = resolve_doc_path(db, &doc)?;
    let disk_content = fs::read_to_string(&file_path).ok();
    let content = disk_content.as_deref().unwrap_or(&doc.content);

    // Strip the review queue section so generators never treat review
    // entries as document facts.
    let body = crate::patterns::content_body(content);

    // Collect defined terms from glossary/definition/reference documents
    let all_docs = {
        let mut docs = Vec::new();
        for repo in db.list_repositories()? {
            docs.extend(db.get_documents_for_repo(&repo.id)?.into_values());
        }
        docs
    };
    let defined_terms = collect_defined_terms(&all_docs);

    // Generate all question types
    let mut new_questions = generate_temporal_questions(body);
    new_questions.extend(generate_conflict_questions(body));
    new_questions.extend(generate_duplicate_entry_questions(body));
    new_questions.extend(generate_missing_questions(body));
    new_questions.extend(generate_ambiguous_questions_with_type(body, doc.doc_type.as_deref(), &defined_terms));
    new_questions.extend(generate_stale_questions(body, 365)); // Default 365 days
    new_questions.extend(generate_precision_questions(body));

    // Generate duplicate questions
    if let Ok(similar_docs) = db.find_similar_documents(&doc.id, 0.95) {
        new_questions.extend(generate_duplicate_questions(&similar_docs));
    }

    // Cross-document fact validation is now handled by the agent via get_fact_pairs tool.

    // Post-filter: remove conflict questions for boundary-month sequential entries
    filter_sequential_conflicts(body, &mut new_questions);

    // Check for existing review queue to avoid duplicates
    let existing_questions = parse_review_queue(content).unwrap_or_default();
    let existing_descriptions: HashSet<_> =
        existing_questions.iter().map(|q| &q.description).collect();
    let existing_conflict_normalized: HashSet<_> = existing_questions
        .iter()
        .filter(|q| q.question_type == crate::models::QuestionType::Conflict)
        .map(|q| crate::processor::normalize_conflict_desc(&q.description))
        .collect();

    // Filter out questions that already exist
    let questions_to_add = filter_duplicate_questions(
        new_questions,
        &existing_descriptions,
        &existing_conflict_normalized,
    );

    // Format questions for response
    let questions_json: Vec<Value> = questions_to_add
        .iter()
        .map(|q| format_question_json(q, None))
        .collect();

    // If not dry_run, write questions to file and sync DB
    if !dry_run && !questions_to_add.is_empty() {
        let updated_content = append_review_questions(content, &questions_to_add);
        fs::write(&file_path, &updated_content)?;
        let new_hash = content_hash(&updated_content);
        db.update_document_content(&doc_id, &updated_content, &new_hash)?;
    }

    Ok(serde_json::json!({
        "doc_id": doc_id,
        "doc_title": doc.title,
        "questions_generated": questions_to_add.len(),
        "questions": questions_json,
        "dry_run": dry_run,
        "message": if dry_run {
            "Dry run - no changes made"
        } else if questions_to_add.is_empty() {
            "No new questions to add"
        } else {
            "Questions added to Review Queue"
        }
    }))
}

/// Multi-doc mode: generate questions for all documents with time-boxing.
async fn generate_questions_all(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    args: &Value,
    dry_run: bool,
) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();
    let time_budget = crate::mcp::tools::helpers::resolve_time_budget(args);
    let deadline = crate::mcp::tools::helpers::make_deadline(time_budget);

    // Get all active documents
    let all_docs: Vec<_> = match repo_id {
        Some(rid) => db
            .get_documents_for_repo(rid)?
            .into_values()
            .filter(|d| !d.is_deleted)
            .collect(),
        None => {
            let mut all = Vec::new();
            for repo in db.list_repositories()? {
                all.extend(
                    db.get_documents_for_repo(&repo.id)?
                        .into_values()
                        .filter(|d| !d.is_deleted),
                );
            }
            all
        }
    };

    // Resume: skip already-processed docs
    let doc_offset = get_str_arg(args, "resume")
        .and_then(crate::mcp::tools::helpers::decode_resume_token)
        .and_then(|v| v.get("doc_offset").and_then(|o| o.as_u64()))
        .unwrap_or(0) as usize;
    let docs: Vec<_> = all_docs.into_iter().skip(doc_offset).collect();
    let total = doc_offset + docs.len();

    let mut docs_processed = 0;
    let mut total_generated = 0;
    let mut details = Vec::new();

    for doc in &docs {
        if let Some(dl) = deadline {
            if std::time::Instant::now() > dl {
                break;
            }
        }

        match generate_questions_single(db, embedding, &doc.id, dry_run).await {
            Ok(result) => {
                let count = result["questions_generated"].as_u64().unwrap_or(0);
                if count > 0 {
                    details.push(serde_json::json!({
                        "doc_id": doc.id,
                        "doc_title": doc.title,
                        "questions_generated": count,
                    }));
                }
                total_generated += count as usize;
            }
            Err(e) => {
                warn!("generate_questions failed for {}: {e}", doc.id);
            }
        }
        docs_processed += 1;
    }

    let processed_total = doc_offset + docs_processed;

    let mut result = serde_json::json!({
        "documents_processed": docs_processed,
        "total_questions_generated": total_generated,
        "dry_run": dry_run,
        "details": details,
    });

    let resume_token = if processed_total < total {
        Some(crate::mcp::tools::helpers::encode_resume_token(
            &serde_json::json!({"doc_offset": processed_total}),
        ))
    } else {
        None
    };

    crate::mcp::tools::helpers::apply_time_budget_progress(
        &mut result,
        processed_total,
        total,
        "generate_questions",
        time_budget.is_some(),
        resume_token.as_deref(),
    );

    Ok(result)
}

/// Filters out questions that already exist in the review queue.
/// Returns only new questions not present in existing_descriptions.
fn filter_duplicate_questions(
    new_questions: impl IntoIterator<Item = crate::models::ReviewQuestion>,
    existing_descriptions: &HashSet<&String>,
    existing_conflict_normalized: &HashSet<&str>,
) -> Vec<crate::models::ReviewQuestion> {
    new_questions
        .into_iter()
        .filter(|q| {
            if existing_descriptions.contains(&q.description) {
                return false;
            }
            if q.question_type == crate::models::QuestionType::Conflict
                && existing_conflict_normalized.contains(
                    crate::processor::normalize_conflict_desc(&q.description),
                )
            {
                return false;
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuestionType, ReviewQuestion};

    #[test]
    fn test_filter_duplicate_questions_removes_existing() {
        let existing = [
            "When was this role held?".to_string(),
            "What is the source?".to_string(),
        ];
        let existing_set: HashSet<_> = existing.iter().collect();

        let new_questions = vec![
            ReviewQuestion {
                question_type: QuestionType::Temporal,
                line_ref: Some(5),
                description: "When was this role held?".to_string(), // duplicate
                answered: false,
                answer: None,
                line_number: 1,
            },
            ReviewQuestion {
                question_type: QuestionType::Missing,
                line_ref: Some(10),
                description: "New question here".to_string(), // new
                answered: false,
                answer: None,
                line_number: 2,
            },
        ];

        let filtered = filter_duplicate_questions(new_questions, &existing_set, &HashSet::new());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].description, "New question here");
    }

    #[test]
    fn test_filter_duplicate_questions_keeps_all_when_no_existing() {
        let existing_set: HashSet<&String> = HashSet::new();

        let new_questions = vec![
            ReviewQuestion {
                question_type: QuestionType::Temporal,
                line_ref: Some(5),
                description: "Question 1".to_string(),
                answered: false,
                answer: None,
                line_number: 1,
            },
            ReviewQuestion {
                question_type: QuestionType::Conflict,
                line_ref: Some(10),
                description: "Question 2".to_string(),
                answered: false,
                answer: None,
                line_number: 2,
            },
        ];

        let filtered = filter_duplicate_questions(new_questions, &existing_set, &HashSet::new());
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_format_question_json_includes_all_fields() {
        let question = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "When was this role held?".to_string(),
            answered: false,
            answer: None,
            line_number: 1,
        };

        let json = format_question_json(&question, None);
        assert_eq!(json["type"], "temporal");
        assert_eq!(json["line_ref"], 5);
        assert_eq!(json["description"], "When was this role held?");
    }

    #[test]
    fn test_format_question_json_handles_null_line_ref() {
        let question = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: None,
            description: "What is the source?".to_string(),
            answered: false,
            answer: None,
            line_number: 1,
        };

        let json = format_question_json(&question, None);
        assert_eq!(json["type"], "missing");
        assert!(json["line_ref"].is_null());
        assert_eq!(json["description"], "What is the source?");
    }
}
