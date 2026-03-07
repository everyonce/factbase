//! Shared apply-all loop for review answer processing.
//!
//! Used by both MCP `apply_review_answers` and CLI `review --apply`.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::QuestionType;
use crate::organize::fs_helpers::write_file;
use crate::processor::{content_hash, normalize_review_section, parse_review_queue};
use crate::progress::ProgressReporter;use crate::{
    apply_changes_to_section, apply_confirmations, apply_source_citations, dedup_titles,
    identify_affected_section, interpret_answer, remove_processed_questions, replace_section,
    stamp_reviewed_by_text, stamp_reviewed_lines, stamp_reviewed_markers,
    stamp_sequential_by_text, stamp_sequential_lines, uncheck_deferred_questions,
    InterpretedAnswer,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Result of applying review answers across documents.
#[derive(Debug, Default)]
pub struct ApplyResult {
    pub total_applied: usize,
    pub total_errors: usize,
    pub filtered_count: usize,
    pub documents: Vec<ApplyDocResult>,
    /// Total number of documents with work to do.
    pub total_work: usize,
}

/// Per-document result.
#[derive(Debug)]
pub struct ApplyDocResult {
    pub doc_id: String,
    pub doc_title: String,
    pub questions_applied: usize,
    pub status: ApplyStatus,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum ApplyStatus {
    Applied,
    DryRun,
    Error,
}

/// Configuration for the apply-all operation.
pub struct ApplyConfig<'a> {
    pub doc_id_filter: Option<&'a str>,
    pub repo_filter: Option<&'a str>,
    pub dry_run: bool,
    pub since: Option<DateTime<Utc>>,
    /// Optional deadline for time-boxed operations.
    pub deadline: Option<std::time::Instant>,
    /// Whether to acquire the global write guard before writing results.
    /// Set to `true` in MCP context (concurrent requests), `false` in CLI/tests.
    pub acquire_write_guard: bool,
}

/// Apply all answered review questions across documents.
///
/// Core loop shared by MCP and CLI. Loads documents with review queues,
/// filters by optional doc_id/repo/since, and applies answered questions.
pub async fn apply_all_review_answers(
    db: &Database,
    config: &ApplyConfig<'_>,
    progress: &ProgressReporter,
) -> Result<ApplyResult, FactbaseError> {
    let mut docs = db.get_documents_with_review_queue(config.repo_filter)?;

    // When a specific doc_id is requested, ensure it's in the candidate list
    // even if has_review_queue is FALSE in the DB (the flag can be stale when
    // the file was edited externally or the DB wasn't synced after check/answer).
    if let Some(filter_id) = config.doc_id_filter {
        if !docs.iter().any(|d| d.id == filter_id) {
            if let Some(doc) = db.get_document(filter_id)? {
                if !doc.is_deleted {
                    docs.push(doc);
                }
            }
        }
    }

    let repos = db.list_repositories()?;
    let repo_paths: HashMap<_, _> = repos.iter().map(|r| (r.id.as_str(), &r.path)).collect();

    // Load glossary terms for stripping redundant reviewed markers
    let glossary_terms = {
        let types = ["definition", "glossary", "reference"];
        let mut terms = std::collections::HashSet::new();
        for t in &types {
            if let Ok(gdocs) = db.list_documents(Some(t), None, None, 100) {
                for gdoc in &gdocs {
                    terms.extend(crate::extract_defined_terms(&gdoc.content));
                }
            }
        }
        terms
    };

    let mut result = ApplyResult::default();
    let mut work = Vec::new();

    for doc in &docs {
        if let Some(filter_id) = config.doc_id_filter {
            if doc.id != filter_id {
                continue;
            }
        }
        // Filter by modification time if --since is specified
        if let Some(since) = config.since {
            if let Some(modified) = doc.file_modified_at {
                if modified < since {
                    result.filtered_count += 1;
                    continue;
                }
            }
        }
        let abs_path = match repo_paths.get(doc.repo_id.as_str()) {
            Some(repo_path) => repo_path.join(&doc.file_path),
            None => {
                let msg = format!("Repository '{}' not found", doc.repo_id);
                warn!(doc_id = %doc.id, repo_id = %doc.repo_id, "Skipping document: repository not found");
                if config.doc_id_filter.is_some() {
                    result.total_errors += 1;
                    result.documents.push(ApplyDocResult {
                        doc_id: doc.id.clone(),
                        doc_title: doc.title.clone(),
                        questions_applied: 0,
                        status: ApplyStatus::Error,
                        error: Some(msg),
                    });
                }
                continue;
            }
        };
        // Check both disk and DB for answered questions and use whichever
        // has more.  This keeps get_review_queue (DB-based) and apply
        // consistent: if the DB has answers (e.g. written by answer_questions),
        // we use them even when the disk copy diverges.  Conversely, if the
        // file was edited externally, disk answers win.
        let disk_content = fs::read_to_string(&abs_path).ok();
        let disk_answered = disk_content
            .as_deref()
            .and_then(parse_review_queue)
            .map(|qs| {
                qs.into_iter()
                    .enumerate()
                    .filter(|(_, q)| q.answered && q.answer.is_some())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let db_answered = parse_review_queue(&doc.content)
            .map(|qs| {
                qs.into_iter()
                    .enumerate()
                    .filter(|(_, q)| q.answered && q.answer.is_some())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Use whichever source found more answered questions
        let (answered, need_sync) = if disk_answered.len() >= db_answered.len() {
            (disk_answered, false)
        } else {
            (db_answered, true)
        };

        if answered.is_empty() {
            warn!(doc_id = %doc.id, "Skipping document: no answered questions on disk or in database");
            continue;
        }

        // Sync DB content to disk so apply_one_document reads consistent content.
        // If the sync fails, report an error instead of silently skipping.
        if need_sync {
            if let Err(e) = fs::write(&abs_path, &doc.content) {
                let msg = format!("Failed to sync DB content to disk: {e}");
                warn!(doc_id = %doc.id, error = %e, "Failed to sync DB content to disk");
                result.total_errors += 1;
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: 0,
                    status: ApplyStatus::Error,
                    error: Some(msg),
                });
                continue;
            }
        }
        work.push((doc, answered, abs_path));
    }

    let total = work.len();
    result.total_work = total;

    // Acquire write guard for non-dry-run (rewrites doc content on disk+DB).
    // Acquired here — after all read-only answer parsing — so dry-run
    // and read-only phases are never blocked by a concurrent write.
    let _write_guard = if config.dry_run || !config.acquire_write_guard {
        None
    } else {
        Some(crate::write_guard::WriteGuard::try_acquire()?)
    };

    for (i, (doc, answered, abs_path)) in work.iter().enumerate() {
        // Check deadline before starting a new document
        if let Some(deadline) = config.deadline {
            if std::time::Instant::now() > deadline {
                break;
            }
        }

        let count = answered.len();
        progress.report(
            i + 1,
            total,
            &format!("Applying {} question(s) to {}", count, doc.title),
        );

        let apply_result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            apply_one_document(abs_path, answered, config.dry_run),
        )
        .await;

        match apply_result {
            Ok(Ok((applied, new_content))) => {
                // Strip reviewed markers that are now redundant due to glossary coverage
                let new_content = if !config.dry_run && !new_content.is_empty() && !glossary_terms.is_empty() {
                    crate::processor::strip_glossary_reviewed_markers(&new_content, &glossary_terms)
                } else {
                    new_content
                };
                // Sync cleaned content to database using the exact content
                // that was written to disk (avoids read-back race with watcher).
                if !config.dry_run && !new_content.is_empty() {
                    let new_hash = content_hash(&new_content);
                    if let Err(e) = db.update_document_content(&doc.id, &new_content, &new_hash) {
                        warn!(doc_id = %doc.id, error = %e, "Failed to sync content to database after apply");
                    }
                    // Defensive: ensure disk matches DB (guards against
                    // concurrent watcher overwriting the file between
                    // apply_one_document's write and this point).
                    if let Err(e) = fs::write(abs_path, &new_content) {
                        warn!(doc_id = %doc.id, error = %e, "Failed to write cleaned content back to disk");
                    }
                }
                result.total_applied += applied;
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: applied,
                    status: if config.dry_run {
                        ApplyStatus::DryRun
                    } else {
                        ApplyStatus::Applied
                    },
                    error: None,
                });
            }
            Ok(Err(e)) => {
                result.total_errors += 1;
                warn!(doc_id = %doc.id, error = %e, "Failed to apply review answers");
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: 0,
                    status: ApplyStatus::Error,
                    error: Some(e.to_string()),
                });
            }
            Err(_) => {
                result.total_errors += 1;
                warn!(doc_id = %doc.id, "Timed out applying review answers (120s)");
                result.documents.push(ApplyDocResult {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    questions_applied: 0,
                    status: ApplyStatus::Error,
                    error: Some("Timed out after 120 seconds".to_string()),
                });
            }
        }
    }

    info!(
        applied = result.total_applied,
        errors = result.total_errors,
        "apply_all_review_answers complete"
    );

    Ok(result)
}

async fn apply_one_document(
    file_path: &Path,
    answered: &[(usize, crate::models::ReviewQuestion)],
    dry_run: bool,
) -> Result<(usize, String), FactbaseError> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| FactbaseError::internal(format!("{}: {}", file_path.display(), e)))?;

    let review_questions: Vec<_> = answered.iter().map(|(_, q)| q.clone()).collect();

    let interpreted: Vec<InterpretedAnswer> = review_questions
        .iter()
        .map(|q| {
            let answer = q.answer.as_deref().unwrap_or("");
            InterpretedAnswer {
                question: q.clone(),
                instruction: interpret_answer(q, answer),
            }
        })
        .collect();

    let has_active_changes = interpreted.iter().any(|ia| {
        !matches!(
            ia.instruction,
            crate::ChangeInstruction::Dismiss | crate::ChangeInstruction::Defer
        )
    });

    if dry_run {
        return Ok((if has_active_changes { review_questions.len() } else { 0 }, String::new()));
    }

    let today = chrono::Local::now().date_naive();
    let mut new_content = content.clone();

    // --- Phase 1: Apply active changes (deterministic + LLM) ---
    if has_active_changes {
        let active: Vec<_> = interpreted
            .iter()
            .filter(|ia| {
                !matches!(
                    ia.instruction,
                    crate::ChangeInstruction::Dismiss | crate::ChangeInstruction::Defer
                )
            })
            .collect();

        let is_deterministic = |ia: &&InterpretedAnswer| {
            matches!(
                ia.instruction,
                crate::ChangeInstruction::AddSource { .. }
                    | crate::ChangeInstruction::Delete { .. }
                    | crate::ChangeInstruction::UpdateTemporal { .. }
                    | crate::ChangeInstruction::AddTemporal { .. }
            )
        };

        // Apply deterministic instructions directly
        let deterministic: Vec<_> = active.iter().copied().filter(is_deterministic).collect();
        if !deterministic.is_empty() {
            let source_pairs: Vec<(&str, &str)> = deterministic
                .iter()
                .filter_map(|ia| match &ia.instruction {
                    crate::ChangeInstruction::AddSource { line_text, source_info } => {
                        Some((line_text.as_str(), source_info.as_str()))
                    }
                    _ => None,
                })
                .collect();
            new_content = apply_source_citations(&new_content, &source_pairs);

            let temporal_updates: Vec<(&str, Option<&str>, &str)> = deterministic
                .iter()
                .filter_map(|ia| match &ia.instruction {
                    crate::ChangeInstruction::UpdateTemporal { line_text, old_tag, new_tag } => {
                        Some((line_text.as_str(), Some(old_tag.as_str()), new_tag.as_str()))
                    }
                    crate::ChangeInstruction::AddTemporal { line_text, tag } => {
                        Some((line_text.as_str(), None, tag.as_str()))
                    }
                    _ => None,
                })
                .collect();
            new_content = apply_confirmations(&new_content, &temporal_updates);

            for ia in &deterministic {
                if let crate::ChangeInstruction::Delete { line_text } = &ia.instruction {
                    if !line_text.is_empty() {
                        new_content = new_content
                            .lines()
                            .filter(|l| !l.contains(line_text.as_str()))
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                }
            }

            let det_line_refs: Vec<usize> =
                deterministic.iter().filter_map(|ia| ia.question.line_ref).collect();
            new_content = stamp_reviewed_lines(&new_content, &det_line_refs, &today);
        }

        // Apply LLM-dependent instructions
        let needs_llm: Vec<_> = active.iter().copied().filter(|ia| !is_deterministic(ia)).collect();
        if !needs_llm.is_empty() {
            let llm_questions: Vec<_> = needs_llm.iter().map(|ia| ia.question.clone()).collect();
            let Some((start, end, section)) =
                identify_affected_section(&new_content, &llm_questions)
            else {
                return Err(FactbaseError::internal("Could not identify affected section"));
            };
            let llm_interpreted: Vec<InterpretedAnswer> =
                needs_llm.into_iter().cloned().collect();
            let new_section = apply_changes_to_section(&section, &llm_interpreted).await?;
            let new_section = stamp_reviewed_markers(&new_section, &today);
            new_content = replace_section(&new_content, start, end, &new_section);
        }
    }

    // --- Phase 2: Stamp dismissed lines (single path for all cases) ---
    let dismissed_line_refs: Vec<usize> = interpreted
        .iter()
        .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
        .flat_map(conflict_line_refs)
        .collect();
    let conflict_dismissed_refs: Vec<usize> = interpreted
        .iter()
        .filter(|ia| {
            matches!(ia.instruction, crate::ChangeInstruction::Dismiss)
                && ia.question.question_type == QuestionType::Conflict
        })
        .flat_map(conflict_line_refs)
        .collect();
    new_content = stamp_sequential_lines(&new_content, &conflict_dismissed_refs);
    let conflict_texts: Vec<String> = interpreted
        .iter()
        .filter(|ia| {
            matches!(ia.instruction, crate::ChangeInstruction::Dismiss)
                && ia.question.question_type == QuestionType::Conflict
        })
        .flat_map(conflict_fact_texts)
        .collect();
    let text_refs: Vec<&str> = conflict_texts.iter().map(|s| s.as_str()).collect();
    new_content = stamp_sequential_by_text(&new_content, &text_refs);
    new_content = stamp_reviewed_lines(&new_content, &dismissed_line_refs, &today);
    let dismissed_texts: Vec<String> = interpreted
        .iter()
        .filter(|ia| matches!(ia.instruction, crate::ChangeInstruction::Dismiss))
        .flat_map(conflict_fact_texts)
        .collect();
    let dismissed_text_refs: Vec<&str> = dismissed_texts.iter().map(|s| s.as_str()).collect();
    new_content = stamp_reviewed_by_text(&new_content, &dismissed_text_refs, &today);

    // --- Phase 3: Remove/uncheck processed questions ---
    let deferred_indices: Vec<usize> = answered
        .iter()
        .zip(interpreted.iter())
        .filter(|(_, ia)| matches!(ia.instruction, crate::ChangeInstruction::Defer))
        .map(|((i, _), _)| *i)
        .collect();
    new_content = uncheck_deferred_questions(&new_content, &deferred_indices);
    // Remove all non-deferred answered questions (dismissed + active applied)
    let remove_indices: Vec<usize> = answered
        .iter()
        .zip(interpreted.iter())
        .filter(|(_, ia)| !matches!(ia.instruction, crate::ChangeInstruction::Defer))
        .map(|((i, _), _)| *i)
        .collect();
    new_content = remove_processed_questions(&new_content, &remove_indices);

    // --- Phase 4: Finalize ---
    new_content = normalize_review_section(&new_content);
    new_content = dedup_titles(&new_content);

    let validation_errors = super::validate::validate_document(&content, &new_content);
    if !validation_errors.is_empty() {
        let details: Vec<String> = validation_errors.iter().map(|e| e.detail.clone()).collect();
        return Err(FactbaseError::internal(format!(
            "Document validation failed (keeping original): {}",
            details.join("; ")
        )));
    }

    write_file(file_path, &new_content)?;
    let applied = if has_active_changes { review_questions.len() } else { 0 };
    Ok((applied, new_content))
}

/// Collect line refs to stamp as reviewed for a dismissed question.
///
/// For conflict questions the description encodes the second fact's line number
/// as a `(line:N)` suffix.  Both facts in the pair need a reviewed marker so
/// the conflict is not regenerated on the next check run.
fn conflict_line_refs(ia: &InterpretedAnswer) -> Vec<usize> {
    let mut refs = Vec::new();
    if let Some(lr) = ia.question.line_ref {
        refs.push(lr);
    }
    if ia.question.question_type == QuestionType::Conflict {
        if let Some(n) = ia
            .question
            .description
            .rsplit("(line:")
            .next()
            .and_then(|s| s.strip_suffix(')'))
            .and_then(|s| s.parse::<usize>().ok())
        {
            refs.push(n);
        }
    }
    refs
}

/// Extract quoted fact texts from a conflict question description.
/// Conflict descriptions look like: `"fact1" @t[...] overlaps with "fact2" @t[...] (line:N)`
fn conflict_fact_texts(ia: &InterpretedAnswer) -> Vec<String> {
    if ia.question.question_type != QuestionType::Conflict {
        return Vec::new();
    }
    crate::patterns::QUOTED_TEXT_REGEX
        .captures_iter(&ia.question.description)
        .map(|c| c[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ReviewQuestion;

    #[test]
    fn test_conflict_line_refs_extracts_both_lines() {
        let ia = InterpretedAnswer {
            question: ReviewQuestion {
                question_type: QuestionType::Conflict,
                line_ref: Some(5),
                description: r#""VP at Acme" @t[2020..2023] overlaps with "CEO at BigCo" @t[2022..2024] - were both true simultaneously? (line:7)"#.to_string(),
                answered: true,
                answer: Some("dismiss".to_string()),
                line_number: 10,
            },
            instruction: crate::ChangeInstruction::Dismiss,
        };
        let refs = conflict_line_refs(&ia);
        assert_eq!(refs, vec![5, 7]);
    }

    #[test]
    fn test_conflict_line_refs_non_conflict_single_line() {
        let ia = InterpretedAnswer {
            question: ReviewQuestion {
                question_type: QuestionType::Temporal,
                line_ref: Some(3),
                description: "some temporal question".to_string(),
                answered: true,
                answer: Some("dismiss".to_string()),
                line_number: 10,
            },
            instruction: crate::ChangeInstruction::Dismiss,
        };
        let refs = conflict_line_refs(&ia);
        assert_eq!(refs, vec![3]);
    }

    #[test]
    fn test_conflict_line_refs_no_line_ref() {
        let ia = InterpretedAnswer {
            question: ReviewQuestion {
                question_type: QuestionType::Conflict,
                line_ref: None,
                description: "conflict without line ref".to_string(),
                answered: true,
                answer: Some("dismiss".to_string()),
                line_number: 10,
            },
            instruction: crate::ChangeInstruction::Dismiss,
        };
        let refs = conflict_line_refs(&ia);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_conflict_fact_texts_extracts_both_quotes() {
        let ia = InterpretedAnswer {
            question: ReviewQuestion {
                question_type: QuestionType::Conflict,
                line_ref: Some(5),
                description: r#""VP at Acme" @t[2020..2023] overlaps with "CEO at BigCo" @t[2022..2024] (line:7)"#.to_string(),
                answered: true,
                answer: Some("dismiss".to_string()),
                line_number: 10,
            },
            instruction: crate::ChangeInstruction::Dismiss,
        };
        let texts = conflict_fact_texts(&ia);
        assert_eq!(texts, vec!["VP at Acme", "CEO at BigCo"]);
    }

    #[test]
    fn test_conflict_fact_texts_non_conflict_returns_empty() {
        let ia = InterpretedAnswer {
            question: ReviewQuestion {
                question_type: QuestionType::Temporal,
                line_ref: Some(3),
                description: r#""Some fact" - when?"#.to_string(),
                answered: true,
                answer: Some("dismiss".to_string()),
                line_number: 10,
            },
            instruction: crate::ChangeInstruction::Dismiss,
        };
        let texts = conflict_fact_texts(&ia);
        assert!(texts.is_empty());
    }

    /// Reproduces the bug where apply_review_answers returns 0 when the disk
    /// file has answered questions but the DB content is stale (unanswered).
    /// The fix: read from disk first (filesystem is source of truth).
    #[tokio::test]
    async fn test_apply_finds_answered_on_disk_when_db_stale() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Disk file has answered questions (the source of truth)
        let disk_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was this true?\n\
> dismiss\n";
        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, disk_content).unwrap();

        // DB content is stale — still has unanswered questions
        let db_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[temporal]` Line 4: When was this true?\n";

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: db_content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        // Should find the answered question from disk even though DB is stale
        assert_eq!(result.documents.len(), 1, "Should process 1 document from disk");
    }

    /// Reproduces the bug where apply_review_answers returns 0 when the disk
    /// file has unanswered questions but the DB content has answered questions.
    /// The fix: fall back to DB content and sync it to disk.
    #[tokio::test]
    async fn test_apply_falls_back_to_db_content_when_disk_diverges() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Disk file has unanswered questions (simulating divergence)
        let disk_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[temporal]` Line 4: When was this true?\n\
  > \n";
        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, disk_content).unwrap();

        // DB content has the same question answered
        let db_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was this true?\n\
  > dismiss\n";

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: db_content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        // Should have processed the document via DB fallback (dismiss returns 0 applied
        // but still processes the document — removes the question from the review queue)
        assert_eq!(result.documents.len(), 1, "Should process 1 document via DB fallback");

        // Disk file should have been modified — the dismissed question should be removed
        let final_disk = std::fs::read_to_string(&doc_file).unwrap();
        assert!(
            !final_disk.contains("- [x] `@q[temporal]`"),
            "Answered question should be removed from disk after apply"
        );
        assert!(
            !final_disk.contains("- [ ] `@q[temporal]`"),
            "Unanswered question should not remain on disk"
        );
    }

    /// When DB has answered questions and disk is in sync, apply should process them.
    #[tokio::test]
    async fn test_apply_finds_answered_from_db_when_disk_in_sync() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Both disk and DB have the answered question
        let content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was this true?\n\
> dismiss\n";
        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        assert_eq!(result.documents.len(), 1, "Should process 1 document");
    }

    /// When both disk and DB have no answered questions, apply should return 0.
    #[tokio::test]
    async fn test_apply_returns_zero_when_no_answered_anywhere() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[temporal]` Line 4: When was this true?\n\
  > \n";
        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        assert_eq!(result.total_applied, 0);
        assert!(result.documents.is_empty());
    }

    /// Reproduces the bug where apply_review_answers returns no-op when the
    /// DB has_review_queue flag is FALSE but the disk file has answered questions.
    /// This happens when the review queue was added/answered externally without
    /// updating the DB flag (e.g., file edited outside factbase tools, or a
    /// race with the file watcher resetting the flag).
    #[tokio::test]
    async fn test_apply_finds_doc_when_has_review_queue_flag_is_false() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Disk file has a review queue with answered questions
        let disk_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was this true?\n\
> dismiss\n";
        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, disk_content).unwrap();

        // DB content has NO review queue (simulating stale has_review_queue=FALSE)
        let db_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n";

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: db_content.to_string(), // No review queue → has_review_queue=FALSE
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        // Before the fix, this returned 0 documents because has_review_queue=FALSE
        // excluded the document from the candidate list entirely.
        assert_eq!(
            result.documents.len(),
            1,
            "Should find and process document even when has_review_queue flag is stale"
        );
    }

    /// Reproduces the bug where answers are written directly into the document
    /// file (not via answer_questions tool) and both DB and disk are in sync,
    /// but apply_review_answers still returns "No answered questions to apply."
    #[tokio::test]
    async fn test_apply_processes_answers_written_directly_to_file() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Content with multiple answered questions (simulating agent editing file directly)
        let content = "\
<!-- factbase:543601 -->\n\
# Technologies\n\
\n\
- Uses Rust @t[=2024-01]\n\
- Uses Python @t[=2023-06]\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When did Rust usage begin?\n\
> dismiss\n\
- [x] `@q[temporal]` Line 5: When did Python usage begin?\n\
> dismiss\n\
- [x] `@q[missing]` Line 4: What is the source for Rust usage?\n\
> dismiss\n";

        let doc_file = repo_dir.join("technologies.md");
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        // DB content matches disk (simulating a scan after file was edited)
        let doc = Document {
            id: "543601".into(),
            repo_id: "r1".into(),
            file_path: "technologies.md".into(),
            file_hash: "hash1".into(),
            title: "Technologies".into(),
            doc_type: Some("technology".into()),
            content: content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("543601"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        assert_eq!(
            result.documents.len(),
            1,
            "Should process document with answers written directly to file"
        );
    }

    /// When DB has answered questions but disk file is missing, the error
    /// should be reported (not silently swallowed).
    #[tokio::test]
    async fn test_apply_reports_error_when_disk_sync_fails() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // DB content has answered questions but NO file on disk
        let db_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was this true?\n\
> dismiss\n";

        // Point repo to a non-existent subdirectory so the write fails
        let bad_repo_dir = dir.path().join("nonexistent").join("deep");

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: bad_repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: "hash1".into(),
            title: "Test Entity".into(),
            doc_type: Some("note".into()),
            content: db_content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
                    doc_id_filter: Some("abc123"),
                    repo_filter: None,
                    dry_run: false,
                    since: None,
                    deadline: None,
                    acquire_write_guard: false,
                };
        let progress = ProgressReporter::Silent;

        let result = apply_all_review_answers(&db, &config, &progress)
            .await
            .unwrap();

        // Should report an error instead of silently returning "no questions"
        assert_eq!(result.total_errors, 1, "Should report disk sync error");
        assert_eq!(result.documents.len(), 1, "Should include error document in results");
        assert!(
            result.documents[0].error.is_some(),
            "Should include error message"
        );
    }

    /// After apply, disk file and DB content must both have no @q tags,
    /// and the DB file_hash must match the disk content hash so a
    /// subsequent scan sees the document as unchanged.
    #[tokio::test]
    async fn test_apply_disk_and_db_in_sync_no_stale_review_queue() {
        use crate::database::Database;

        use crate::models::{Document, Repository};
        use crate::processor::content_hash;
        use crate::progress::ProgressReporter;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let content = "\
<!-- factbase:aaa111 -->\n\
# Test Doc\n\
\n\
- Fact one\n\
- Fact two\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [x] `@q[temporal]` Line 4: When was fact one true?\n\
> dismiss\n\
- [x] `@q[missing]` Line 5: Source for fact two?\n\
> dismiss\n";

        let doc_file = repo_dir.join("test.md");
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let doc = Document {
            id: "aaa111".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            file_hash: content_hash(content),
            title: "Test Doc".into(),
            doc_type: Some("note".into()),
            content: content.to_string(),
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).unwrap();


        let config = ApplyConfig {
            doc_id_filter: Some("aaa111"),
            repo_filter: None,
            dry_run: false,
            since: None,
            deadline: None,
            acquire_write_guard: false,
        };

        let result = apply_all_review_answers(&db, &config, &ProgressReporter::Silent)
            .await
            .unwrap();
        assert_eq!(result.documents.len(), 1);

        // Disk must have no @q tags
        let disk_content = std::fs::read_to_string(&doc_file).unwrap();
        assert!(
            !disk_content.contains("@q["),
            "Disk file must have no @q tags after apply, got:\n{disk_content}"
        );

        // DB must have no @q tags
        let db_doc = db.get_document("aaa111").unwrap().unwrap();
        assert!(
            !db_doc.content.contains("@q["),
            "DB content must have no @q tags after apply"
        );

        // Disk and DB content must match
        assert_eq!(
            disk_content, db_doc.content,
            "Disk and DB content must be identical after apply"
        );

        // DB file_hash must match disk content hash (scan would see UNCHANGED)
        let disk_hash = content_hash(&disk_content);
        assert_eq!(
            db_doc.file_hash, disk_hash,
            "DB file_hash must match disk content hash so scan sees no update"
        );
    }
}
