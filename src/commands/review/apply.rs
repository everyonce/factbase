//! Review answer application logic.

use super::super::{
    parse_since_filter, setup_database, setup_db_and_resolve_repos, setup_review_llm_with_timeout,
};
use super::args::ReviewArgs;
use super::status::file_modified_since;
use chrono::{DateTime, Utc};
use factbase::{
    config::validate_timeout, extract_inbox_blocks, identify_affected_section, interpret_answer,
    parse_review_queue, remove_processed_questions, replace_section, InterpretedAnswer,
    QuestionType, ReviewLlm, ReviewQuestion,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{error, info};

/// Answered question with document context
#[derive(Debug, Clone)]
pub struct AnsweredQuestion {
    pub question: ReviewQuestion,
    pub doc_title: String,
    pub file_path: String,
    pub repo_path: String,
}

#[tracing::instrument(
    name = "cmd_review_apply",
    skip(args),
    fields(repo = ?args.repo, dry_run = args.dry_run, detailed = args.detailed)
)]
pub async fn cmd_review_apply(args: &ReviewArgs) -> anyhow::Result<()> {
    let (db, repos_to_process) = setup_db_and_resolve_repos(args.repo.as_deref())?;

    // Parse --since filter if provided
    let since_filter: Option<DateTime<Utc>> = parse_since_filter(&args.since)?;

    // Collect answered questions grouped by document
    let (answered_by_doc, filtered_count) =
        collect_answered_questions(&db, &repos_to_process, since_filter.as_ref())?;

    if filtered_count > 0 && !args.quiet {
        println!("(Filtered {} document(s) by --since)", filtered_count);
    }

    if answered_by_doc.is_empty() {
        if !args.quiet {
            println!("No answered questions to process.");
        }
        return Ok(());
    }

    let total_questions: usize = answered_by_doc.values().map(|v| v.len()).sum();
    if !args.quiet {
        println!(
            "Found {} answered question(s) in {} document(s).",
            total_questions,
            answered_by_doc.len()
        );
    }

    if args.dry_run && !args.quiet {
        println!("\n--dry-run: showing proposed changes without modifying files\n");
    }

    // Validate timeout if provided
    if let Some(timeout) = args.timeout {
        validate_timeout(timeout)?;
    }

    // Initialize ReviewLlm using shared helper
    let (config, _) = setup_database()?;
    let review_llm = setup_review_llm_with_timeout(&config, args.timeout).await;
    info!("Review LLM initialized with model: {}", review_llm.model());

    // Process each document
    let mut processed_count = 0;
    let mut error_count = 0;

    for (doc_id, questions) in &answered_by_doc {
        let doc_title = &questions[0].doc_title;
        let file_path = &questions[0].file_path;
        let repo_path = &questions[0].repo_path;

        // Construct absolute path from repo path + relative file path
        let abs_path = Path::new(repo_path).join(file_path);
        let abs_path_str = abs_path.to_string_lossy().to_string();

        if !args.quiet {
            println!("\nProcessing {} [{}]...", doc_title, doc_id);
        }

        match process_document(
            &review_llm,
            &abs_path_str,
            questions,
            args.dry_run,
            args.detailed,
        )
        .await
        {
            Ok(changes_made) => {
                if changes_made {
                    processed_count += questions.len();
                    if !args.quiet {
                        println!("  ✓ Applied {} change(s)", questions.len());
                    }
                } else if !args.quiet {
                    println!("  - No changes needed (all dismissed)");
                }
            }
            Err(e) => {
                error_count += 1;
                error!(file = %file_path, error = %e, "Failed to process document");
                if !args.quiet {
                    println!("  ✗ Error: {}", e);
                }
            }
        }
    }

    if !args.quiet {
        println!("\nSummary:");
        println!("  Processed: {} question(s)", processed_count);
        if error_count > 0 {
            println!("  Errors: {}", error_count);
        }
    }

    // --- Inbox block processing ---
    let inbox_docs = collect_inbox_documents(&db, &repos_to_process, since_filter.as_ref())?;

    if !inbox_docs.is_empty() {
        if !args.quiet {
            println!(
                "\nFound {} document(s) with inbox blocks.",
                inbox_docs.len()
            );
        }
        if args.dry_run && !args.quiet {
            println!("--dry-run: showing inbox content without integrating\n");
        }

        let mut inbox_processed = 0;
        let mut inbox_errors = 0;

        for inbox_doc in &inbox_docs {
            if !args.quiet {
                println!(
                    "\nIntegrating inbox: {} [{}]...",
                    inbox_doc.doc_title, inbox_doc.doc_id
                );
            }

            let abs_path = Path::new(&inbox_doc.repo_path).join(&inbox_doc.file_path);
            // Re-read file from disk (may have been updated by review question processing above)
            let content = match fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(e) => {
                    inbox_errors += 1;
                    if !args.quiet {
                        println!("  ✗ Error reading file: {}", e);
                    }
                    continue;
                }
            };

            let blocks = extract_inbox_blocks(&content);
            if blocks.is_empty() {
                continue; // Inbox was already stripped or empty
            }

            if args.dry_run {
                for (i, block) in blocks.iter().enumerate() {
                    println!(
                        "  Inbox block {} (lines {}-{}):",
                        i + 1,
                        block.start_line + 1,
                        block.end_line + 1
                    );
                    for line in block.content.lines().take(5) {
                        println!("    {}", line);
                    }
                    if block.content.lines().count() > 5 {
                        println!("    ...");
                    }
                }
                inbox_processed += 1;
                continue;
            }

            if args.detailed {
                for block in &blocks {
                    println!("  Inbox content:");
                    for line in block.content.lines() {
                        println!("    | {}", line);
                    }
                }
            }

            match factbase::apply_inbox_integration(&review_llm, &content, &blocks).await {
                Ok(new_content) => {
                    if let Err(e) = write_file_safely(&abs_path, &new_content) {
                        inbox_errors += 1;
                        if !args.quiet {
                            println!("  ✗ Error writing file: {}", e);
                        }
                    } else {
                        inbox_processed += 1;
                        if !args.quiet {
                            println!("  ✓ Inbox integrated and stripped");
                        }
                    }
                }
                Err(e) => {
                    inbox_errors += 1;
                    error!(file = %inbox_doc.file_path, error = %e, "Failed to integrate inbox");
                    if !args.quiet {
                        println!("  ✗ Error: {}", e);
                    }
                }
            }
        }

        if !args.quiet {
            println!("\nInbox summary:");
            println!("  Integrated: {} document(s)", inbox_processed);
            if inbox_errors > 0 {
                println!("  Errors: {}", inbox_errors);
            }
        }
    }

    Ok(())
}

/// Process a single document's answered questions
async fn process_document(
    llm: &ReviewLlm,
    file_path: &str,
    questions: &[AnsweredQuestion],
    dry_run: bool,
    verbose: bool,
) -> anyhow::Result<bool> {
    // Read current file content
    let content = fs::read_to_string(file_path)?;

    // Interpret all answers
    let interpreted: Vec<InterpretedAnswer> = questions
        .iter()
        .map(|aq| {
            let answer = aq.question.answer.as_deref().unwrap_or("");
            InterpretedAnswer {
                question: aq.question.clone(),
                instruction: interpret_answer(&aq.question, answer),
            }
        })
        .collect();

    // Verbose: show each question being processed
    if verbose {
        println!("  Questions:");
        for (i, ia) in interpreted.iter().enumerate() {
            let q = &ia.question;
            let type_name = match q.question_type {
                QuestionType::Temporal => "temporal",
                QuestionType::Conflict => "conflict",
                QuestionType::Missing => "missing",
                QuestionType::Ambiguous => "ambiguous",
                QuestionType::Stale => "stale",
                QuestionType::Duplicate => "duplicate",
            };
            let line_info = q
                .line_ref
                .map(|l| format!("Line {}", l))
                .unwrap_or_else(|| "N/A".to_string());
            println!(
                "    {}. @q[{}] {}: {}",
                i + 1,
                type_name,
                line_info,
                q.description
            );
            if let Some(ref answer) = q.answer {
                println!("       Answer: {}", answer);
            }
            println!("       → {:?}", ia.instruction);
        }
    }

    // Check if all are dismissals
    let all_dismissed = interpreted
        .iter()
        .all(|ia| matches!(ia.instruction, factbase::ChangeInstruction::Dismiss));

    if all_dismissed {
        if verbose {
            println!("  All questions dismissed, removing from review queue");
        }
        // Just remove questions from review queue
        if !dry_run {
            let indices: Vec<usize> = (0..questions.len()).collect();
            let new_content = remove_processed_questions(&content, &indices);
            write_file_safely(Path::new(file_path), &new_content)?;
        }
        return Ok(false);
    }

    // Identify affected section
    let review_questions: Vec<ReviewQuestion> =
        questions.iter().map(|aq| aq.question.clone()).collect();

    let Some((start, end, section)) = identify_affected_section(&content, &review_questions) else {
        anyhow::bail!("Could not identify affected section");
    };

    if dry_run {
        println!("  Section (lines {}-{}):", start, end);
        for line in section.lines().take(5) {
            println!("    {}", line);
        }
        if section.lines().count() > 5 {
            println!("    ...");
        }
        println!("  Changes:");
        for ia in &interpreted {
            println!("    - {:?}", ia.instruction);
        }
        return Ok(true);
    }

    // Verbose: show section being processed
    if verbose {
        println!("  Affected section (lines {}-{}):", start, end);
        for line in section.lines().take(10) {
            println!("    | {}", line);
        }
        if section.lines().count() > 10 {
            println!("    | ... ({} more lines)", section.lines().count() - 10);
        }
    }

    // Apply changes using LLM
    if verbose {
        println!("  Calling LLM to apply changes...");
    }
    let new_section = factbase::apply_changes_to_section(llm, &section, &interpreted).await?;

    // Verbose: show before/after diff
    if verbose {
        println!("  Changes applied:");
        let old_lines: Vec<&str> = section.lines().collect();
        let new_lines: Vec<&str> = new_section.lines().collect();

        // Simple diff: show lines that changed
        let max_lines = old_lines.len().max(new_lines.len());
        let mut changes_shown = 0;
        for i in 0..max_lines {
            let old_line = old_lines.get(i).copied().unwrap_or("");
            let new_line = new_lines.get(i).copied().unwrap_or("");
            if old_line != new_line {
                if changes_shown < 10 {
                    if !old_line.is_empty() {
                        println!("    - {}", old_line);
                    }
                    if !new_line.is_empty() {
                        println!("    + {}", new_line);
                    }
                }
                changes_shown += 1;
            }
        }
        if changes_shown > 10 {
            println!("    ... and {} more changes", changes_shown - 10);
        }
        if changes_shown == 0 {
            println!("    (no visible changes)");
        }
    }

    // Replace section in document
    let mut new_content = replace_section(&content, start, end, &new_section);

    // Remove processed questions from review queue
    let indices: Vec<usize> = (0..questions.len()).collect();
    new_content = remove_processed_questions(&new_content, &indices);

    // Write file safely
    write_file_safely(Path::new(file_path), &new_content)?;

    Ok(true)
}

/// Write file atomically using temp file + rename
fn write_file_safely(path: &Path, content: &str) -> anyhow::Result<()> {
    let temp_path = path.with_extension("md.tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Collect all answered questions across documents, grouped by document ID.
/// Returns (questions_by_doc, filtered_count) where filtered_count is the number of docs skipped by --since.
/// Document with inbox blocks
#[derive(Debug, Clone)]
struct InboxDocument {
    doc_id: String,
    doc_title: String,
    file_path: String,
    repo_path: String,
}

/// Collect documents that contain inbox blocks.
fn collect_inbox_documents(
    db: &factbase::Database,
    repos: &[factbase::Repository],
    since_filter: Option<&DateTime<Utc>>,
) -> anyhow::Result<Vec<InboxDocument>> {
    let mut result = Vec::new();

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;

        for doc in docs.values() {
            if doc.is_deleted {
                continue;
            }

            if let Some(since) = since_filter {
                let abs_path = repo.path.join(&doc.file_path);
                if !file_modified_since(&abs_path, since) {
                    continue;
                }
            }

            let blocks = extract_inbox_blocks(&doc.content);
            if !blocks.is_empty() {
                result.push(InboxDocument {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    file_path: doc.file_path.clone(),
                    repo_path: repo.path.to_string_lossy().to_string(),
                });
            }
        }
    }

    Ok(result)
}

pub fn collect_answered_questions(
    db: &factbase::Database,
    repos: &[factbase::Repository],
    since_filter: Option<&DateTime<Utc>>,
) -> anyhow::Result<(HashMap<String, Vec<AnsweredQuestion>>, usize)> {
    let mut result: HashMap<String, Vec<AnsweredQuestion>> = HashMap::new();
    let mut filtered_count = 0usize;

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;

        for doc in docs.values() {
            if doc.is_deleted {
                continue;
            }

            // Filter by modification time if --since is specified
            if let Some(since) = since_filter {
                let abs_path = repo.path.join(&doc.file_path);
                if !file_modified_since(&abs_path, since) {
                    filtered_count += 1;
                    continue;
                }
            }

            if let Some(questions) = parse_review_queue(&doc.content) {
                let answered: Vec<_> = questions
                    .into_iter()
                    .filter(|q| q.answered && q.answer.is_some())
                    .map(|q| AnsweredQuestion {
                        question: q,
                        doc_title: doc.title.clone(),
                        file_path: doc.file_path.clone(),
                        repo_path: String::new(), // Will be set below
                    })
                    .collect();

                if !answered.is_empty() {
                    // Store repo path for constructing absolute file paths
                    let answered_with_repo: Vec<_> = answered
                        .into_iter()
                        .map(|mut q| {
                            q.repo_path = repo.path.to_string_lossy().to_string();
                            q
                        })
                        .collect();
                    result.insert(doc.id.clone(), answered_with_repo);
                }
            }
        }
    }

    Ok((result, filtered_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_file_safely_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        write_file_safely(&file_path, "Hello, world!").unwrap();

        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "Hello, world!");
    }

    #[test]
    fn test_write_file_safely_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        fs::write(&file_path, "Original content").unwrap();
        write_file_safely(&file_path, "New content").unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "New content");
    }

    #[test]
    fn test_write_file_safely_no_temp_file_remains() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let temp_path = temp_dir.path().join("test.md.tmp");

        write_file_safely(&file_path, "Content").unwrap();

        assert!(
            !temp_path.exists(),
            "Temp file should be renamed, not left behind"
        );
    }

    #[test]
    fn test_write_file_safely_invalid_path() {
        let result = write_file_safely(Path::new("/nonexistent/dir/file.md"), "Content");
        assert!(result.is_err());
    }
}
