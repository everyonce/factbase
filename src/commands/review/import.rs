//! Review question import logic.

use super::super::setup_database_only;
use super::args::ReviewArgs;
use anyhow::Context;
use factbase::{append_review_questions, QuestionType, ReviewQuestion};
use serde::Deserialize;
use std::fs;
use tracing::warn;

/// Imported question from JSON/YAML file
#[derive(Debug, Clone, Deserialize)]
pub struct ImportedQuestion {
    #[serde(rename = "type")]
    pub question_type: String,
    pub line_ref: Option<usize>,
    pub description: String,
}

/// Document with imported questions from JSON/YAML file
#[derive(Debug, Clone, Deserialize)]
pub struct ImportedDocQuestions {
    pub doc_id: String,
    /// Title from import file (for reference only, document looked up by ID)
    #[serde(default)]
    pub doc_title: Option<String>,
    /// File path from import file (for reference only, document looked up by ID)
    #[serde(default)]
    pub file_path: Option<String>,
    pub questions: Vec<ImportedQuestion>,
}

/// Convert imported question to ReviewQuestion
fn imported_to_review_question(imported: &ImportedQuestion) -> ReviewQuestion {
    ReviewQuestion {
        question_type: imported
            .question_type
            .parse::<QuestionType>()
            .unwrap_or_else(|_| {
                warn!(
                    "Unknown question type '{}', defaulting to Missing",
                    imported.question_type
                );
                QuestionType::Missing
            }),
        line_ref: imported.line_ref,
        description: imported.description.clone(),
        answered: false,
        answer: None,
        line_number: 0, // Will be set when appended to document
    }
}

pub fn cmd_review_import(args: &ReviewArgs, import_path: &str) -> anyhow::Result<()> {
    let db = setup_database_only()?;

    // Read and parse the import file
    let content = fs::read_to_string(import_path)
        .with_context(|| format!("Failed to read import file '{import_path}'"))?;

    // Determine format from file extension
    let imported: Vec<ImportedDocQuestions> =
        if crate::commands::utils::ends_with_ext(import_path, ".yaml") || crate::commands::utils::ends_with_ext(import_path, ".yml") {
            serde_yaml_ng::from_str(&content).context("Failed to parse YAML")?
        } else {
            serde_json::from_str(&content).context("Failed to parse JSON")?
        };

    if imported.is_empty() {
        if !args.quiet {
            println!("No questions to import");
        }
        return Ok(());
    }

    let mut total_imported = 0usize;
    let mut docs_updated = 0usize;
    let mut docs_skipped = 0usize;
    // Pre-allocate for typical case of ~8 errors
    let mut errors: Vec<String> = Vec::with_capacity(8);

    for doc_questions in &imported {
        // Look up document in database
        let Some(doc) = db.get_document(&doc_questions.doc_id)? else {
            errors.push(format!(
                "Document '{}' not found in database",
                doc_questions.doc_id
            ));
            docs_skipped += 1;
            continue;
        };

        // Warn if imported metadata doesn't match database
        if let Some(ref title) = doc_questions.doc_title {
            if title != &doc.title {
                warn!(
                    "Title mismatch for {}: import has '{}', database has '{}'",
                    doc_questions.doc_id, title, doc.title
                );
            }
        }
        if let Some(ref path) = doc_questions.file_path {
            if path != &doc.file_path {
                warn!(
                    "Path mismatch for {}: import has '{}', database has '{}'",
                    doc_questions.doc_id, path, doc.file_path
                );
            }
        }

        if doc.is_deleted {
            errors.push(format!("Document '{}' is deleted", doc_questions.doc_id));
            docs_skipped += 1;
            continue;
        }

        // Find the repository for this document
        let repos = db.list_repositories()?;
        let repo = repos.iter().find(|r| r.id == doc.repo_id);
        let Some(repo) = repo else {
            errors.push(format!(
                "Repository '{}' not found for document '{}'",
                doc.repo_id, doc_questions.doc_id
            ));
            docs_skipped += 1;
            continue;
        };

        // Filter by repo if specified
        if let Some(ref filter_repo) = args.repo {
            if repo.id != *filter_repo {
                continue;
            }
        }

        // Convert imported questions to ReviewQuestions
        let questions: Vec<ReviewQuestion> = doc_questions
            .questions
            .iter()
            .map(imported_to_review_question)
            .collect();

        if questions.is_empty() {
            continue;
        }

        // Construct absolute file path
        let abs_path = repo.path.join(&doc.file_path);

        if args.dry_run {
            if !args.quiet {
                println!(
                    "Would import {} question(s) to {} [{}]",
                    questions.len(),
                    doc.title,
                    doc.id
                );
                for q in &questions {
                    let line_ref = q
                        .line_ref
                        .map(|n| format!("Line {n}: "))
                        .unwrap_or_default();
                    println!("  @q[{:?}] {}{}", q.question_type, line_ref, q.description);
                }
            }
            total_imported += questions.len();
            docs_updated += 1;
            continue;
        }

        // Read current file content
        let current_content = match fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!(
                    "Failed to read file '{}': {}",
                    abs_path.display(),
                    e
                ));
                docs_skipped += 1;
                continue;
            }
        };

        // Append questions to document
        let updated_content = append_review_questions(&current_content, &questions);

        // Write updated content
        if let Err(e) = fs::write(&abs_path, &updated_content) {
            errors.push(format!(
                "Failed to write file '{}': {}",
                abs_path.display(),
                e
            ));
            docs_skipped += 1;
            continue;
        }

        total_imported += questions.len();
        docs_updated += 1;
        if !args.quiet {
            println!(
                "Imported {} question(s) to {} [{}]",
                questions.len(),
                doc.title,
                doc.id
            );
        }
    }

    // Print summary
    if !args.quiet {
        println!();
        if args.dry_run {
            println!("Would import {total_imported} question(s) to {docs_updated} document(s)");
        } else {
            println!("Imported {total_imported} question(s) to {docs_updated} document(s)");
        }

        if docs_skipped > 0 {
            println!("Skipped {docs_skipped} document(s)");
        }

        if !errors.is_empty() {
            println!();
            println!("Errors:");
            for err in &errors {
                println!("  {err}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imported_to_review_question() {
        let imported = ImportedQuestion {
            question_type: "temporal".to_string(),
            line_ref: Some(5),
            description: "When was this true?".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Temporal);
        assert_eq!(result.line_ref, Some(5));
        assert_eq!(result.description, "When was this true?");
        assert!(!result.answered);
        assert!(result.answer.is_none());
        assert_eq!(result.line_number, 0);
    }

    #[test]
    fn test_imported_to_review_question_no_line_ref() {
        let imported = ImportedQuestion {
            question_type: "duplicate".to_string(),
            line_ref: None,
            description: "May be duplicate".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Duplicate);
        assert!(result.line_ref.is_none());
    }

    #[test]
    fn test_imported_to_review_question_conflict() {
        let imported = ImportedQuestion {
            question_type: "conflict".to_string(),
            line_ref: Some(10),
            description: "Contradictory dates".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Conflict);
        assert_eq!(result.line_ref, Some(10));
    }

    #[test]
    fn test_imported_to_review_question_missing() {
        let imported = ImportedQuestion {
            question_type: "missing".to_string(),
            line_ref: None,
            description: "Source needed".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Missing);
    }

    #[test]
    fn test_imported_to_review_question_ambiguous() {
        let imported = ImportedQuestion {
            question_type: "ambiguous".to_string(),
            line_ref: Some(3),
            description: "Which location?".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Ambiguous);
    }

    #[test]
    fn test_imported_to_review_question_stale() {
        let imported = ImportedQuestion {
            question_type: "stale".to_string(),
            line_ref: Some(7),
            description: "Info from 2020".to_string(),
        };

        let result = imported_to_review_question(&imported);

        assert_eq!(result.question_type, QuestionType::Stale);
    }

    #[test]
    fn test_imported_to_review_question_unknown_type() {
        let imported = ImportedQuestion {
            question_type: "invalid_type".to_string(),
            line_ref: None,
            description: "Unknown question".to_string(),
        };

        let result = imported_to_review_question(&imported);

        // Unknown types default to Missing
        assert_eq!(result.question_type, QuestionType::Missing);
        assert_eq!(result.description, "Unknown question");
    }
}
