//! Review queue retrieval MCP tool.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_bool_arg, get_str_arg};
use crate::models::QuestionType;
use crate::processor::parse_review_queue;
use serde_json::Value;
use tracing::instrument;

use super::format_question_json;

/// Parses a type filter string into a QuestionType.
///
/// Returns None for invalid or unrecognized type strings.
fn parse_type_filter(type_str: &str) -> Option<QuestionType> {
    match type_str.to_lowercase().as_str() {
        "temporal" => Some(QuestionType::Temporal),
        "conflict" => Some(QuestionType::Conflict),
        "missing" => Some(QuestionType::Missing),
        "ambiguous" => Some(QuestionType::Ambiguous),
        "stale" => Some(QuestionType::Stale),
        "duplicate" => Some(QuestionType::Duplicate),
        _ => None,
    }
}

/// Gets pending review questions across documents.
///
/// Parses review queues from document content and aggregates questions.
///
/// # Arguments (from JSON)
/// - `repo` (optional): Filter by repository ID
/// - `doc_id` (optional): Filter by specific document ID
/// - `type` (optional): Filter by question type (temporal, conflict, missing, etc.)
/// - `include_context` (optional): Include surrounding lines from the document for each question (default: false)
///
/// # Returns
/// JSON with `questions` array (doc_id, doc_title, type, description, answered, answer),
/// `total`, `answered`, and `unanswered` counts.
#[instrument(name = "mcp_get_review_queue", skip(db, args))]
pub fn get_review_queue(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_filter = get_str_arg(args, "repo").map(String::from);
    let doc_id_filter = get_str_arg(args, "doc_id").map(String::from);
    let type_filter = get_str_arg(args, "type").map(String::from);
    let include_context = get_bool_arg(args, "include_context", false);

    // Parse type filter into QuestionType
    let question_type_filter: Option<QuestionType> =
        type_filter.as_ref().and_then(|t| parse_type_filter(t));

    let mut all_questions: Vec<Value> = Vec::new();
    let mut total_answered = 0;
    let mut total_unanswered = 0;

    // Get repositories to scan
    let repos = if let Some(ref repo_id) = repo_filter {
        db.get_repository(repo_id)?
            .map(|r| vec![r])
            .unwrap_or_default()
    } else {
        db.list_repositories()?
    };

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;

        for (_id, doc) in docs {
            // Skip deleted documents
            if doc.is_deleted {
                continue;
            }

            // Skip if doc_id filter doesn't match
            if let Some(ref filter_id) = doc_id_filter {
                if &doc.id != filter_id {
                    continue;
                }
            }

            // Parse review queue from document
            if let Some(questions) = parse_review_queue(&doc.content) {
                for q in questions {
                    // Apply type filter
                    if let Some(ref filter_type) = question_type_filter {
                        if &q.question_type != filter_type {
                            continue;
                        }
                    }

                    if q.answered {
                        total_answered += 1;
                    } else {
                        total_unanswered += 1;
                    }

                    let mut qjson = format_question_json(&q, Some((&doc.id, &doc.title)));

                    if include_context {
                        if let Some(line_ref) = q.line_ref {
                            let lines: Vec<&str> = doc.content.lines().collect();
                            let start = line_ref.saturating_sub(3); // 2 lines before (0-indexed)
                            let end = (line_ref + 2).min(lines.len()); // 2 lines after
                            let context: Vec<&str> = lines[start..end].to_vec();
                            qjson["context"] = serde_json::json!({
                                "lines": context,
                                "start_line": start + 1,
                            });
                        }
                    }

                    all_questions.push(qjson);
                }
            }
        }
    }

    Ok(serde_json::json!({
        "questions": all_questions,
        "total": all_questions.len(),
        "answered": total_answered,
        "unanswered": total_unanswered
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ReviewQuestion;

    #[test]
    fn test_parse_type_filter_valid_types() {
        assert_eq!(parse_type_filter("temporal"), Some(QuestionType::Temporal));
        assert_eq!(parse_type_filter("conflict"), Some(QuestionType::Conflict));
        assert_eq!(parse_type_filter("missing"), Some(QuestionType::Missing));
        assert_eq!(
            parse_type_filter("ambiguous"),
            Some(QuestionType::Ambiguous)
        );
        assert_eq!(parse_type_filter("stale"), Some(QuestionType::Stale));
        assert_eq!(
            parse_type_filter("duplicate"),
            Some(QuestionType::Duplicate)
        );
    }

    #[test]
    fn test_parse_type_filter_case_insensitive() {
        assert_eq!(parse_type_filter("TEMPORAL"), Some(QuestionType::Temporal));
        assert_eq!(parse_type_filter("Conflict"), Some(QuestionType::Conflict));
        assert_eq!(parse_type_filter("MiSsInG"), Some(QuestionType::Missing));
    }

    #[test]
    fn test_parse_type_filter_invalid_returns_none() {
        assert_eq!(parse_type_filter("invalid"), None);
        assert_eq!(parse_type_filter(""), None);
        assert_eq!(parse_type_filter("temp"), None);
        assert_eq!(parse_type_filter("temporalx"), None);
    }

    #[test]
    fn test_format_question_json_with_doc_context() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "When was this role held?".to_string(),
            answered: false,
            answer: None,
            line_number: 10,
        };

        let json = format_question_json(&q, Some(("abc123", "Test Doc")));

        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["doc_title"], "Test Doc");
        assert_eq!(json["type"], "temporal");
        assert_eq!(json["line_ref"], 5);
        assert_eq!(json["description"], "When was this role held?");
        assert_eq!(json["answered"], false);
        assert!(json["answer"].is_null());
    }

    #[test]
    fn test_format_question_json_with_answer() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: None,
            description: "What is the source?".to_string(),
            answered: true,
            answer: Some("LinkedIn profile".to_string()),
            line_number: 20,
        };

        let json = format_question_json(&q, Some(("def456", "Another Doc")));

        assert_eq!(json["doc_id"], "def456");
        assert_eq!(json["type"], "missing");
        assert!(json["line_ref"].is_null());
        assert_eq!(json["answered"], true);
        assert_eq!(json["answer"], "LinkedIn profile");
    }
}
