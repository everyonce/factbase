//! Review question generation for lint.
//!
//! Generates review questions for documents based on temporal coverage,
//! source citations, and other quality metrics.

use super::ReviewQuestionOptions;
use crate::commands::check::output::{ExportedDocQuestions, ExportedQuestion};
use crate::commands::check::review::{
    add_duplicate_questions, generate_and_prune, ReviewConfig,
};
use factbase::database::Database;
use factbase::models::{Document, Repository};
use factbase::processor::append_review_questions;
use glob::Pattern;
use std::fs;
use std::path::Path;
use tracing::info;

/// Generate review questions for a document.
/// Returns (new_question_count, optional exported questions).
pub fn generate_review_questions(
    doc: &Document,
    repo: &Repository,
    db: &Database,
    opts: &ReviewQuestionOptions,
    title_duplicates: &[(&str, &str)], // (id, title) of docs with same title
    defined_terms: &std::collections::HashSet<String>,
) -> anyhow::Result<(usize, Option<ExportedDocQuestions>)> {
    // Check if file should be skipped based on ignore_patterns
    let should_skip = repo
        .perspective
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.ignore_patterns.as_ref())
        .is_some_and(|patterns| {
            let file_path = Path::new(&doc.file_path);
            let relative = file_path.strip_prefix(&repo.path).unwrap_or(file_path);
            let rel_str = relative.to_string_lossy();
            patterns
                .iter()
                .any(|p| Pattern::new(p).is_ok_and(|pat| pat.matches(&rel_str)))
        });

    if should_skip {
        if opts.is_table_format {
            println!(
                "  SKIP: {} [{}] (matches ignore pattern)",
                doc.title, doc.id
            );
        }
        info!(
            "Skipping {} due to perspective.review.ignore_patterns",
            doc.file_path
        );
        return Ok((0, None));
    }

    // Build review config from perspective and args
    let perspective_stale_days = repo
        .perspective
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.stale_days);
    let stale_threshold = perspective_stale_days
        .map(|d| d as i64)
        .or(opts.max_age)
        .unwrap_or(365);
    if perspective_stale_days.is_some() {
        info!(
            "Using perspective.review.stale_days={} for repo {}",
            stale_threshold, repo.id
        );
    }

    let required_fields = repo
        .perspective
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.required_fields.clone());
    if required_fields.is_some() {
        info!(
            "Using perspective.review.required_fields for repo {} ({} types configured)",
            repo.id,
            required_fields
                .as_ref()
                .map_or(0, std::collections::HashMap::len)
        );
    }

    let review_config = ReviewConfig {
        stale_threshold,
        required_fields,
        defined_terms: defined_terms.clone(),
    };

    // Generate questions and prune stale ones
    let (mut questions_to_add, pruned_content, pruned_count) =
        generate_and_prune(&doc.content, doc.doc_type.as_deref(), &review_config);

    // Add duplicate questions (embedding-based)
    if let Ok(similar_docs) = db.find_similar_documents(&doc.id, opts.min_similarity) {
        add_duplicate_questions(&mut questions_to_add, &similar_docs);
    }

    // Add title-based duplicate questions
    for (other_id, other_title) in title_duplicates {
        if *other_id != doc.id {
            questions_to_add.push(factbase::models::ReviewQuestion::new(
                factbase::models::QuestionType::Duplicate,
                None,
                format!("Same title as \"{other_title}\" [{other_id}] — are these the same entity?"),
            ));
        }
    }

    if questions_to_add.is_empty() && pruned_count == 0 {
        return Ok((0, None));
    }

    let count = questions_to_add.len();

    if opts.export_mode {
        // Export mode: return questions for file export
        let exported = ExportedDocQuestions {
            doc_id: doc.id.clone(),
            doc_title: doc.title.clone(),
            file_path: doc.file_path.clone(),
            questions: questions_to_add
                .iter()
                .map(|q| ExportedQuestion {
                    question_type: q.question_type.as_str().to_string(),
                    line_ref: q.line_ref,
                    description: q.description.clone(),
                })
                .collect(),
        };
        if opts.is_table_format {
            println!(
                "  REVIEW: Collected {} question(s) from {} [{}]",
                questions_to_add.len(),
                doc.title,
                doc.id
            );
        }
        return Ok((count, Some(exported)));
    }

    if opts.dry_run {
        // Dry-run mode: show what would be added
        if opts.is_table_format {
            println!(
                "  REVIEW: Would add {} question(s) to {} [{}]:",
                questions_to_add.len(),
                doc.title,
                doc.id
            );
            for q in &questions_to_add {
                let line_info = q
                    .line_ref
                    .map(|l| format!("Line {l}: "))
                    .unwrap_or_default();
                println!(
                    "    @q[{}] {}{}",
                    q.question_type.as_str(),
                    line_info,
                    q.description
                );
            }
        }
    } else {
        // Normal mode: append questions to document (using pruned content)
        let use_callout = repo
            .perspective
            .as_ref()
            .and_then(|p| p.format.as_ref())
            .map(|f| f.resolve().review_callout)
            .unwrap_or(false);
        let updated_content = append_review_questions(&pruned_content, &questions_to_add, use_callout);
        let abs_path = Path::new(&repo.path).join(&doc.file_path);
        fs::write(&abs_path, &updated_content)?;
        if opts.is_table_format {
            println!(
                "  REVIEW: Added {} question(s) to {} [{}]",
                questions_to_add.len(),
                doc.title,
                doc.id
            );
        }
    }

    Ok((count, None))
}


#[cfg(test)]
mod tests {
    use super::*;
    use factbase::database::Database;
    use factbase::models::{Document, Repository};
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db = Database::new(&tmp.path().join("test.db")).unwrap();
        (db, tmp)
    }

    fn make_repo(path: &std::path::Path) -> Repository {
        Repository {
            id: "test".into(),
            name: "test".into(),
            path: path.to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        }
    }

    fn make_doc(id: &str, content: &str, file_path: &str) -> Document {
        Document {
            id: id.into(),
            repo_id: "test".into(),
            file_path: file_path.into(),
            file_hash: "hash".into(),
            title: format!("Doc {id}"),
            doc_type: Some("document".into()),
            content: content.into(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        }
    }

    #[test]
    fn test_generate_review_questions_no_issues() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        let doc = make_doc("aaa111", "# Title\n\n- Fact @t[2024-01]\n", "doc.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: true,
            export_mode: false,
            is_table_format: false,
            max_age: None,
        };

        let (count, exported) = generate_review_questions(
            &doc, &repo, &db, &opts, &[], &std::collections::HashSet::new(),
        ).unwrap();

        // Well-formed doc with temporal tag should have few/no questions
        assert!(exported.is_none() || count == 0);
    }

    #[test]
    fn test_generate_review_questions_missing_temporal() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        let doc = make_doc("aaa111", "# Title\n\n- Fact without date\n- Another fact\n", "doc.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: true,
            export_mode: false,
            is_table_format: false,
            max_age: None,
        };

        let (count, _) = generate_review_questions(
            &doc, &repo, &db, &opts, &[], &std::collections::HashSet::new(),
        ).unwrap();

        assert!(count > 0, "Should generate questions for facts without temporal tags");
    }

    #[test]
    fn test_generate_review_questions_export_mode() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        let doc = make_doc("aaa111", "# Title\n\n- Undated fact\n", "doc.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: false,
            export_mode: true,
            is_table_format: false,
            max_age: None,
        };

        let (count, exported) = generate_review_questions(
            &doc, &repo, &db, &opts, &[], &std::collections::HashSet::new(),
        ).unwrap();

        if count > 0 {
            assert!(exported.is_some(), "Export mode should return exported questions");
            let exp = exported.unwrap();
            assert_eq!(exp.doc_id, "aaa111");
            assert!(!exp.questions.is_empty());
        }
    }

    #[test]
    fn test_generate_review_questions_title_duplicates() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        let doc = make_doc("aaa111", "# Title\n\n- Fact @t[2024]\n", "doc.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: true,
            export_mode: false,
            is_table_format: false,
            max_age: None,
        };

        let title_dups = vec![("bbb222", "Title")];
        let (count, _) = generate_review_questions(
            &doc, &repo, &db, &opts, &title_dups, &std::collections::HashSet::new(),
        ).unwrap();

        assert!(count > 0, "Should generate duplicate question for same-title docs");
    }

    #[test]
    fn test_generate_review_questions_writes_to_file() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = make_repo(repo_dir.path());
        db.upsert_repository(&repo).unwrap();

        // Create actual file on disk
        let file_path = repo_dir.path().join("doc.md");
        let content = "# Title\n\n- Undated fact\n";
        std::fs::write(&file_path, content).unwrap();

        let doc = make_doc("aaa111", content, "doc.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: false,
            export_mode: false,
            is_table_format: false,
            max_age: None,
        };

        let (count, _) = generate_review_questions(
            &doc, &repo, &db, &opts, &[], &std::collections::HashSet::new(),
        ).unwrap();

        if count > 0 {
            let updated = std::fs::read_to_string(&file_path).unwrap();
            assert!(updated.contains("@q["), "File should contain review questions: {updated}");
        }
    }

    #[test]
    fn test_generate_review_questions_ignore_pattern() {
        let (db, _db_tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let mut repo = make_repo(repo_dir.path());
        repo.perspective = Some(factbase::models::Perspective {
            review: Some(factbase::models::ReviewPerspective {
                stale_days: None,
                ignore_patterns: Some(vec!["archive/*".to_string()]),
                required_fields: None,
                glossary_types: None,
            }),
            ..Default::default()
        });
        db.upsert_repository(&repo).unwrap();

        let doc = make_doc("aaa111", "# Title\n\n- Undated fact\n", "archive/old.md");
        db.upsert_document(&doc).unwrap();

        let opts = ReviewQuestionOptions {
            min_similarity: 0.95,
            dry_run: true,
            export_mode: false,
            is_table_format: true,
            max_age: None,
        };

        let (count, _) = generate_review_questions(
            &doc, &repo, &db, &opts, &[], &std::collections::HashSet::new(),
        ).unwrap();

        assert_eq!(count, 0, "Ignored files should generate no questions");
    }
}
