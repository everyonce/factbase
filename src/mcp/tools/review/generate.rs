//! Review question generation MCP tool.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::mcp::tools::{get_bool_arg, get_str_arg_required};
use crate::processor::{append_review_questions, parse_review_queue};
use crate::question_generator::cross_validate::cross_validate_document;
use crate::question_generator::{
    generate_ambiguous_questions, generate_conflict_questions, generate_duplicate_questions,
    generate_missing_questions, generate_stale_questions, generate_temporal_questions,
};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tracing::{instrument, warn};

use super::format_question_json;

/// Generates review questions for a document.
///
/// Analyzes document content for missing temporal tags, conflicts,
/// missing sources, ambiguous facts, stale information, and duplicates.
/// When embedding and LLM providers are available, also runs cross-document
/// fact validation to detect conflicts with other documents.
/// Appends new questions to the document's review queue.
///
/// # Arguments (from JSON)
/// - `doc_id` (required): Document ID (6-char hex)
/// - `dry_run` (optional): Preview questions without modifying file (default: false)
///
/// # Returns
/// JSON with `doc_id`, `doc_title`, `questions_generated`, `questions` array,
/// `dry_run` flag, and `message`.
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist or is deleted
#[instrument(name = "mcp_generate_questions", skip(db, embedding, llm, args))]
pub async fn generate_questions(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let dry_run = get_bool_arg(args, "dry_run", false);

    // Get the document
    let doc = db.require_document(&doc_id)?;

    // Skip deleted documents
    if doc.is_deleted {
        return Err(FactbaseError::not_found(format!(
            "Document has been deleted: {}",
            doc_id
        )));
    }

    // Generate all question types
    let mut new_questions = generate_temporal_questions(&doc.content);
    new_questions.extend(generate_conflict_questions(&doc.content));
    new_questions.extend(generate_missing_questions(&doc.content));
    new_questions.extend(generate_ambiguous_questions(&doc.content));
    new_questions.extend(generate_stale_questions(&doc.content, 365)); // Default 365 days

    // Generate duplicate questions
    if let Ok(similar_docs) = db.find_similar_documents(&doc.id, 0.95) {
        new_questions.extend(generate_duplicate_questions(&similar_docs));
    }

    // Cross-document fact validation (when LLM is available)
    if let Some(llm) = llm {
        match cross_validate_document(&doc.content, &doc.id, db, embedding, llm).await {
            Ok(cross_questions) => new_questions.extend(cross_questions),
            Err(e) => warn!("Cross-validation failed for {}: {e}", doc_id),
        }
    }

    // Check for existing review queue to avoid duplicates
    let existing_questions = parse_review_queue(&doc.content).unwrap_or_default();
    let existing_descriptions: HashSet<_> =
        existing_questions.iter().map(|q| &q.description).collect();

    // Filter out questions that already exist
    let questions_to_add = filter_duplicate_questions(new_questions, &existing_descriptions);

    // Format questions for response
    let questions_json: Vec<Value> = questions_to_add
        .iter()
        .map(|q| format_question_json(q, None))
        .collect();

    // If not dry_run, write questions to file
    if !dry_run && !questions_to_add.is_empty() {
        let updated_content = append_review_questions(&doc.content, &questions_to_add);
        let file_path = PathBuf::from(&doc.file_path);
        if !file_path.exists() {
            return Err(FactbaseError::not_found(format!(
                "File not found: {}",
                file_path.display()
            )));
        }
        fs::write(&file_path, &updated_content)?;
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

/// Filters out questions that already exist in the review queue.
/// Returns only new questions not present in existing_descriptions.
fn filter_duplicate_questions(
    new_questions: impl IntoIterator<Item = crate::models::ReviewQuestion>,
    existing_descriptions: &HashSet<&String>,
) -> Vec<crate::models::ReviewQuestion> {
    new_questions
        .into_iter()
        .filter(|q| !existing_descriptions.contains(&q.description))
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

        let filtered = filter_duplicate_questions(new_questions, &existing_set);
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

        let filtered = filter_duplicate_questions(new_questions, &existing_set);
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
