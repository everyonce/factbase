//! Pure business logic helpers for review operations.

use crate::error::FactbaseError;
use crate::models::{QuestionType, ReviewQuestion};
use serde_json::Value;

/// Parses a type filter string into a QuestionType.
pub fn parse_type_filter(type_str: &str) -> Option<QuestionType> {
    match type_str.to_lowercase().as_str() {
        "temporal" => Some(QuestionType::Temporal),
        "conflict" => Some(QuestionType::Conflict),
        "missing" => Some(QuestionType::Missing),
        "ambiguous" => Some(QuestionType::Ambiguous),
        "stale" => Some(QuestionType::Stale),
        "duplicate" => Some(QuestionType::Duplicate),
        "corruption" => Some(QuestionType::Corruption),
        "precision" => Some(QuestionType::Precision),
        "weak-source" | "weaksource" => Some(QuestionType::WeakSource),
        _ => None,
    }
}

/// Formats a review question as JSON with optional document context.
pub fn format_question_json(q: &ReviewQuestion, doc_context: Option<(&str, &str)>) -> Value {
    let mut json = q.to_json();
    if let Some((doc_id, doc_title)) = doc_context {
        let obj = json
            .as_object_mut()
            .expect("to_json() returns a JSON object");
        obj.insert("doc_id".to_string(), Value::String(doc_id.to_string()));
        obj.insert(
            "doc_title".to_string(),
            Value::String(doc_title.to_string()),
        );
        obj.insert("answered".to_string(), Value::Bool(q.answered));
        obj.insert("answer".to_string(), serde_json::json!(q.answer));
    }
    json
}

/// Counts unanswered question types and believed questions across documents.
pub fn count_question_types(
    docs: &[crate::models::Document],
) -> (std::collections::HashMap<QuestionType, usize>, usize) {
    use crate::processor::parse_review_queue;
    let mut counts: std::collections::HashMap<QuestionType, usize> =
        std::collections::HashMap::new();
    let mut believed = 0usize;
    for doc in docs {
        if let Some(questions) = parse_review_queue(&doc.content) {
            for q in &questions {
                if q.answered {
                    continue;
                }
                if q.is_deferred() {
                    if q.is_believed() {
                        believed += 1;
                    }
                    continue;
                }
                *counts.entry(q.question_type).or_insert(0) += 1;
            }
        }
    }
    (counts, believed)
}

/// Counts review queue questions into unanswered/deferred/believed buckets.
pub fn count_queue_questions(
    questions: &[ReviewQuestion],
    unanswered: &mut usize,
    deferred: &mut usize,
    believed: &mut usize,
) {
    for q in questions {
        if q.answered {
            // skip — already applied
        } else if q.is_deferred() {
            if q.is_believed() {
                *believed += 1;
            }
            *deferred += 1;
        } else {
            *unanswered += 1;
        }
    }
}

/// Resolve confidence from args: "believed" answers are stored as deferred.
pub fn resolve_confidence(
    answer: &str,
    confidence: Option<&str>,
) -> Result<(bool, String), FactbaseError> {
    let lower = answer.to_lowercase();
    let explicit_defer = lower.starts_with("defer:");
    if explicit_defer {
        let note = answer["defer:".len()..].trim();
        if note.is_empty() {
            return Err(FactbaseError::parse(
                "defer: requires a note explaining why (e.g., 'defer: no matching records found')",
            ));
        }
        return Ok((true, note.to_string()));
    }
    match confidence {
        Some("believed") => Ok((true, format!("believed: {answer}"))),
        _ => Ok((false, answer.to_string())),
    }
}

/// Modifies a question in the review queue content, marking it answered or deferred.
pub fn modify_question_in_queue(
    queue_content: &str,
    question_index: usize,
    answer: &str,
    defer: bool,
) -> Option<String> {
    let mut new_queue_lines: Vec<String> = Vec::new();
    let mut current_question_idx = 0;
    let mut lines = queue_content.lines().peekable();
    let mut modified = false;

    while let Some(line) = lines.next() {
        if line.trim().starts_with("- [") && line.contains("`@q[") {
            if current_question_idx == question_index {
                if defer {
                    new_queue_lines.push(line.to_string());
                } else {
                    let modified_line = line.replacen("- [ ]", "- [x]", 1);
                    new_queue_lines.push(modified_line);
                }
                // Skip existing empty lines or blockquotes after this question
                while let Some(&next) = lines.peek() {
                    let trimmed = next.trim();
                    if trimmed.is_empty() || trimmed.starts_with('>') {
                        lines.next();
                    } else {
                        break;
                    }
                }
                new_queue_lines.push(format!("> {answer}"));
                modified = true;
            } else {
                new_queue_lines.push(line.to_string());
            }
            current_question_idx += 1;
        } else {
            new_queue_lines.push(line.to_string());
        }
    }

    if modified {
        Some(new_queue_lines.join("\n"))
    } else {
        None
    }
}

/// Resolve a document's absolute file path by joining the repository root
/// with the document's relative `file_path`.
pub fn resolve_doc_path(
    db: &crate::database::Database,
    doc: &crate::models::Document,
) -> Result<std::path::PathBuf, FactbaseError> {
    let repo = db.get_repository(&doc.repo_id)?.ok_or_else(|| {
        FactbaseError::not_found(format!(
            "Repository '{}' not found for document {}",
            doc.repo_id, doc.id
        ))
    })?;
    Ok(repo.path.join(&doc.file_path))
}

/// Resolve an optional repo filter (name or ID) to the canonical repo ID.
pub fn resolve_repo_filter(
    db: &crate::database::Database,
    repo: Option<&str>,
) -> Result<Option<String>, FactbaseError> {
    match repo {
        Some(r) => Ok(Some(db.resolve_repo_id(r)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_type_filter_valid() {
        assert_eq!(parse_type_filter("temporal"), Some(QuestionType::Temporal));
        assert_eq!(parse_type_filter("conflict"), Some(QuestionType::Conflict));
        assert_eq!(parse_type_filter("MISSING"), Some(QuestionType::Missing));
    }

    #[test]
    fn test_parse_type_filter_invalid() {
        assert_eq!(parse_type_filter("invalid"), None);
        assert_eq!(parse_type_filter(""), None);
    }

    #[test]
    fn test_resolve_confidence_verified() {
        let (defer, text) = resolve_confidence("@t[2020]", None).unwrap();
        assert!(!defer);
        assert_eq!(text, "@t[2020]");
    }

    #[test]
    fn test_resolve_confidence_believed() {
        let (defer, text) = resolve_confidence("Still accurate", Some("believed")).unwrap();
        assert!(defer);
        assert!(text.starts_with("believed:"));
    }

    #[test]
    fn test_resolve_confidence_defer_prefix() {
        let (defer, text) = resolve_confidence("defer: needs research", None).unwrap();
        assert!(defer);
        assert_eq!(text, "needs research");
    }

    #[test]
    fn test_resolve_confidence_defer_empty_errors() {
        assert!(resolve_confidence("defer:", None).is_err());
    }

    #[test]
    fn test_modify_question_marks_answered() {
        let queue = "\n- [ ] `@q[temporal]` When?\n  > \n- [ ] `@q[missing]` Source?\n";
        let result = modify_question_in_queue(queue, 0, "2020", false).unwrap();
        assert!(result.contains("- [x] `@q[temporal]`"));
        assert!(result.contains("> 2020"));
        assert!(result.contains("- [ ] `@q[missing]`"));
    }

    #[test]
    fn test_modify_question_defer_keeps_unchecked() {
        let queue = "\n- [ ] `@q[conflict]` Issue\n  > \n";
        let result = modify_question_in_queue(queue, 0, "needs research", true).unwrap();
        assert!(result.contains("- [ ] `@q[conflict]`"));
        assert!(result.contains("> needs research"));
    }

    #[test]
    fn test_modify_question_invalid_index() {
        let queue = "\n- [ ] `@q[temporal]` When?\n";
        assert!(modify_question_in_queue(queue, 5, "Answer", false).is_none());
    }

    #[test]
    fn test_format_question_json_with_context() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "When?".to_string(),
            answered: false,
            answer: None,
            line_number: 10,
            confidence: None,
            confidence_reason: None,
        };
        let json = format_question_json(&q, Some(("abc123", "Test")));
        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["doc_title"], "Test");
    }

    #[test]
    fn test_resolve_repo_filter_none() {
        let (db, _tmp) = crate::database::tests::test_db();
        assert!(resolve_repo_filter(&db, None).unwrap().is_none());
    }
}
