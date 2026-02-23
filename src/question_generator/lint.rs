//! Shared lint-all-documents loop for both MCP and CLI.

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::llm::LlmProvider;
use crate::models::Document;
use crate::patterns::{extract_reviewed_date, FACT_LINE_REGEX};
use crate::processor::{append_review_questions, content_hash, parse_review_queue};
use crate::progress::ProgressReporter;
use crate::question_generator::cross_validate::cross_validate_document;
use crate::question_generator::{
    generate_ambiguous_questions, generate_conflict_questions, generate_missing_questions,
    generate_required_field_questions, generate_stale_questions, generate_temporal_questions,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{info, warn};

/// Days within which a reviewed marker suppresses question generation.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Configuration for the shared lint loop.
pub struct LintConfig {
    pub stale_days: i64,
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    pub dry_run: bool,
    pub concurrency: usize,
}

/// Result of linting a single document.
pub struct LintDocResult {
    pub doc_id: String,
    pub doc_title: String,
    pub new_questions: usize,
    pub existing_unanswered: usize,
    pub existing_answered: usize,
    pub skipped_reviewed: usize,
}

/// Lint all documents: generate review questions, optionally cross-validate, write results.
///
/// Used by both MCP `lint_repository` and CLI `cmd_lint --review`.
pub async fn lint_all_documents(
    docs: &[Document],
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: Option<&dyn LlmProvider>,
    config: &LintConfig,
    progress: &ProgressReporter,
) -> Result<Vec<LintDocResult>, crate::error::FactbaseError> {
    let total = docs.len();
    let rf_ref = &config.required_fields;

    let mut all_results = Vec::new();
    for chunk_start in (0..total).step_by(config.concurrency) {
        let chunk_end = (chunk_start + config.concurrency).min(total);
        let chunk = &docs[chunk_start..chunk_end];

        let futs: Vec<_> = chunk
            .iter()
            .enumerate()
            .map(|(ci, doc)| {
                let idx = chunk_start + ci;
                async move {
                    progress.report(idx + 1, total, &format!("Linting {}", doc.title));

                    let mut questions = generate_temporal_questions(&doc.content);
                    questions.extend(generate_conflict_questions(&doc.content));
                    questions.extend(generate_missing_questions(&doc.content));
                    questions.extend(generate_ambiguous_questions(&doc.content));
                    questions.extend(generate_stale_questions(&doc.content, config.stale_days));

                    if let Some(ref rf) = rf_ref {
                        questions.extend(generate_required_field_questions(
                            &doc.content,
                            Some(doc.doc_type.as_deref().unwrap_or("unknown")),
                            rf,
                        ));
                    }

                    if let Some(llm) = llm {
                        match cross_validate_document(&doc.content, &doc.id, db, embedding, llm)
                            .await
                        {
                            Ok(cross) => questions.extend(cross),
                            Err(e) => warn!("Cross-validation failed for {}: {e}", doc.id),
                        }
                    }

                    let existing_questions = parse_review_queue(&doc.content).unwrap_or_default();
                    let existing_descs: HashSet<_> = existing_questions
                        .iter()
                        .map(|q| q.description.clone())
                        .collect();
                    let existing_unanswered =
                        existing_questions.iter().filter(|q| !q.answered).count();
                    let existing_answered =
                        existing_questions.iter().filter(|q| q.answered).count();

                    // Count fact lines with recent reviewed markers
                    let today = Utc::now().date_naive();
                    let skipped_reviewed = doc
                        .content
                        .lines()
                        .filter(|line| FACT_LINE_REGEX.is_match(line))
                        .filter(|line| {
                            extract_reviewed_date(line)
                                .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
                        })
                        .count();

                    questions.retain(|q| !existing_descs.contains(&q.description));

                    (
                        doc,
                        questions,
                        existing_unanswered,
                        existing_answered,
                        skipped_reviewed,
                    )
                }
            })
            .collect();

        let batch = futures::future::join_all(futs).await;
        all_results.extend(batch);
    }

    // Write results (sequential for filesystem safety)
    let mut results = Vec::new();
    for (doc, questions, existing_unanswered, existing_answered, skipped_reviewed) in all_results {
        let count = questions.len();
        if count > 0 {
            if !config.dry_run {
                let updated = append_review_questions(&doc.content, &questions);
                let path = PathBuf::from(&doc.file_path);
                if path.exists() {
                    std::fs::write(&path, &updated)?;
                    let new_hash = content_hash(&updated);
                    db.update_document_content(&doc.id, &updated, &new_hash)?;
                }
            }
            info!("{}: {} new questions", doc.title, count);
        }
        // Include docs with existing questions or skipped facts even if no new questions
        if count > 0 || existing_unanswered > 0 || existing_answered > 0 || skipped_reviewed > 0 {
            results.push(LintDocResult {
                doc_id: doc.id.clone(),
                doc_title: doc.title.clone(),
                new_questions: count,
                existing_unanswered,
                existing_answered,
                skipped_reviewed,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::models::Document;
    use crate::progress::ProgressReporter;

    fn make_doc(id: &str, title: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            ..Document::test_default()
        }
    }

    #[tokio::test]
    async fn test_lint_reports_existing_unanswered() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let content = "- Fact one\n\n<!-- factbase:review -->\n## Review Queue\n\n\
                       - [ ] `@q[temporal]` Line 1: when was this true?\n";
        let docs = vec![make_doc("aaa", "Test", content)];
        let config = LintConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
        };
        let progress = ProgressReporter::Silent;
        let results = lint_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].existing_unanswered, 1);
        assert_eq!(results[0].existing_answered, 0);
    }

    #[tokio::test]
    async fn test_lint_reports_existing_answered() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let content = "- Fact one\n\n<!-- factbase:review -->\n## Review Queue\n\n\
                       - [x] `@q[stale]` Line 1: is this still accurate?\n\
                       > confirmed\n";
        let docs = vec![make_doc("bbb", "Test", content)];
        let config = LintConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
        };
        let progress = ProgressReporter::Silent;
        let results = lint_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].existing_answered, 1);
    }

    #[tokio::test]
    async fn test_lint_reports_skipped_reviewed() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(4);
        let today = Utc::now().format("%Y-%m-%d");
        let content = format!("- Fact one <!-- reviewed:{today} -->\n- Fact two\n");
        let docs = vec![make_doc("ccc", "Test", &content)];
        let config = LintConfig {
            stale_days: 365,
            required_fields: None,
            dry_run: true,
            concurrency: 1,
        };
        let progress = ProgressReporter::Silent;
        let results = lint_all_documents(&docs, &db, &embedding, None, &config, &progress)
            .await
            .unwrap();
        let total_skipped: usize = results.iter().map(|r| r.skipped_reviewed).sum();
        assert_eq!(total_skipped, 1);
    }
}
