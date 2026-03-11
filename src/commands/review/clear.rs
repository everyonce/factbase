//! Clear unanswered review questions from documents.

use super::args::ReviewArgs;
use crate::commands::setup::Setup;
use factbase::models::QuestionType;
use factbase::processor::{
    content_hash,
    normalize_review_section,
    parse_review_queue,
    unwrap_review_callout,
    wrap_review_callout,
};
use std::fs;

pub fn cmd_review_clear(args: &ReviewArgs) -> anyhow::Result<()> {
    let db = Setup::new().build()?.db;
    let repos = db.list_repositories()?;

    let type_filter: Option<QuestionType> = args
        .r#type
        .as_ref()
        .map(|t| {
            t.parse::<QuestionType>()
                .map_err(|e| anyhow::anyhow!("{e}"))
        })
        .transpose()?;

    let docs = db.get_documents_with_review_queue(args.repo.as_deref())?;
    let repo_paths: std::collections::HashMap<_, _> =
        repos.iter().map(|r| (r.id.as_str(), &r.path)).collect();

    let mut total_cleared = 0usize;
    let mut docs_modified = 0usize;

    for doc in &docs {
        let Some(repo_path) = repo_paths.get(doc.repo_id.as_str()) else {
            continue;
        };
        let abs_path = repo_path.join(&doc.file_path);
        let Ok(content) = fs::read_to_string(&abs_path) else {
            continue;
        };

        let Some(questions) = parse_review_queue(&content) else {
            continue;
        };

        // Count what we'll remove
        let to_remove: usize = questions
            .iter()
            .filter(|q| !q.answered && type_filter.as_ref().is_none_or(|t| q.question_type == *t))
            .count();

        if to_remove == 0 {
            continue;
        }

        if args.dry_run {
            let type_label = type_filter
                .as_ref()
                .map_or("unanswered".to_string(), |t| format!("unanswered {t}"));
            println!(
                "  Would clear {} {} question(s) from {} [{}]",
                to_remove, type_label, doc.title, doc.id
            );
            total_cleared += to_remove;
            docs_modified += 1;
            continue;
        }

        let new_content = clear_unanswered(&content, &type_filter);
        let new_content = normalize_review_section(&new_content);

        fs::write(&abs_path, &new_content)?;
        let new_hash = content_hash(&new_content);
        let _ = db.update_document_content(&doc.id, &new_content, &new_hash);

        total_cleared += to_remove;
        docs_modified += 1;

        if !args.quiet {
            println!(
                "  Cleared {} question(s) from {} [{}]",
                to_remove, doc.title, doc.id
            );
        }
    }

    if !args.quiet {
        let action = if args.dry_run { "Would clear" } else { "Cleared" };
        let type_label = type_filter
            .as_ref()
            .map_or(String::new(), |t| format!(" {t}"));
        println!(
            "\n{action} {total_cleared}{type_label} question(s) from {docs_modified} document(s)"
        );
    }

    Ok(())
}

/// Remove unanswered questions from content, optionally filtered by type.
/// Keeps answered questions and deferred questions (have answers).
fn clear_unanswered(content: &str, type_filter: &Option<QuestionType>) -> String {
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = clear_unanswered_inner(&unwrapped, type_filter);
    if was_callout && result != unwrapped {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn clear_unanswered_inner(content: &str, type_filter: &Option<QuestionType>) -> String {
    let Some(marker_pos) = content.find("<!-- factbase:review -->") else {
        return content.to_string();
    };

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let marker = "<!-- factbase:review -->";
    let queue_content = &after_marker[marker.len()..];

    let mut result_lines: Vec<&str> = Vec::new();
    let mut skip_answer = false;

    for line in queue_content.lines() {
        let trimmed = line.trim_start();
        let is_question = trimmed.starts_with("- [");

        if is_question {
            let is_answered = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");

            if is_answered {
                result_lines.push(line);
                skip_answer = false;
            } else if let Some(ref filter) = type_filter {
                // Only clear matching type
                let tag = format!("`@q[{}]`", filter.as_str());
                if trimmed.contains(&tag) {
                    skip_answer = true;
                } else {
                    result_lines.push(line);
                    skip_answer = false;
                }
            } else {
                // Clear all unanswered
                skip_answer = true;
            }
        } else if skip_answer && trimmed.starts_with('>') {
            continue;
        } else {
            result_lines.push(line);
            skip_answer = false;
        }
    }

    let has_questions = result_lines
        .iter()
        .any(|l| l.trim_start().starts_with("- ["));

    if has_questions {
        format!("{}{}\n{}", before_marker, marker, result_lines.join("\n"))
    } else {
        let mut body = before_marker.to_string();
        loop {
            let trimmed = body.trim_end();
            if trimmed.ends_with("## Review Queue") {
                body = trimmed.trim_end_matches("## Review Queue").to_string();
            } else if trimmed.ends_with("---") {
                body = trimmed.trim_end_matches("---").to_string();
            } else {
                body = trimmed.to_string();
                break;
            }
        }
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_all_unanswered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` question 1\n  > \n\
                       - [x] `@q[stale]` answered\n  > yes\n\
                       - [ ] `@q[missing]` question 2\n  > \n";
        let result = clear_unanswered(content, &None);
        assert!(!result.contains("question 1"));
        assert!(result.contains("answered"));
        assert!(!result.contains("question 2"));
    }

    #[test]
    fn test_clear_by_type() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` temporal q\n  > \n\
                       - [ ] `@q[missing]` missing q\n  > \n";
        let result = clear_unanswered(content, &Some(QuestionType::Temporal));
        assert!(!result.contains("temporal q"));
        assert!(result.contains("missing q"));
    }

    #[test]
    fn test_clear_removes_section_when_empty() {
        let content = "# Doc\n\nContent.\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` only question\n  > \n";
        let result = clear_unanswered(content, &None);
        assert!(!result.contains("Review Queue"));
        assert!(result.contains("Content."));
    }

    #[test]
    fn test_clear_unanswered_callout() {
        let content = "# Doc\n\nContent.\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` question 1\n>   > \n> - [x] `@q[stale]` answered\n>   > yes\n";
        let result = clear_unanswered(content, &None);
        assert!(result.contains("> [!info]- Review Queue"), "should preserve callout format");
        assert!(!result.contains("question 1"), "should remove unanswered");
        assert!(result.contains("answered"), "should keep answered");
    }

    #[test]
    fn test_clear_unanswered_callout_all_removed() {
        let content = "# Doc\n\nContent.\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` only q\n>   > \n";
        let result = clear_unanswered(content, &None);
        assert!(!result.contains("Review Queue"), "should remove entire section");
        assert!(result.contains("Content."));
    }

    #[test]
    fn test_clear_unanswered_callout_by_type() {
        let content = "# Doc\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` temporal q\n>   > \n> - [ ] `@q[missing]` missing q\n>   > \n";
        let result = clear_unanswered(content, &Some(QuestionType::Temporal));
        assert!(result.contains("> [!info]- Review Queue"), "should preserve callout");
        assert!(!result.contains("temporal q"), "should remove temporal");
        assert!(result.contains("missing q"), "should keep missing");
    }
}
