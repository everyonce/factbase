//! Review status display logic.

use super::super::{parse_since_filter, print_output, OutputFormat};
use crate::commands::setup::Setup;
use super::args::ReviewArgs;
use chrono::{DateTime, Utc};
use factbase::models::QuestionType;
use factbase::processor::parse_review_queue;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// JSON output structure for review status
#[derive(Serialize)]
pub struct ReviewStatusJson {
    pub total: usize,
    pub answered: usize,
    pub unanswered: usize,
    pub deferred: usize,
    pub by_type: HashMap<String, TypeStats>,
    pub documents: Vec<DocStats>,
}

#[derive(Serialize)]
pub struct TypeStats {
    pub total: usize,
    pub answered: usize,
}

#[derive(Serialize)]
pub struct DocStats {
    pub id: String,
    pub title: String,
    pub total: usize,
    pub answered: usize,
}

/// Check if a file was modified since the given datetime
pub fn file_modified_since(file_path: &Path, since: &DateTime<Utc>) -> bool {
    match fs::metadata(file_path) {
        Ok(metadata) => match metadata.modified() {
            Ok(modified) => {
                let modified_time = DateTime::<Utc>::from(modified);
                modified_time >= *since
            }
            Err(_) => true, // Include if we can't read modification time
        },
        Err(_) => true, // Include if we can't read metadata
    }
}

pub fn cmd_review_status(args: &ReviewArgs) -> anyhow::Result<()> {
    let ctx = Setup::new().resolve_repos(args.repo.as_deref()).build()?;
    let (db, repos_to_process) = (&ctx.db, ctx.repos());

    // Parse --since filter if provided
    let since_filter: Option<DateTime<Utc>> = parse_since_filter(&args.since)?;

    // Collect all questions grouped by document
    let mut total = 0usize;
    let mut answered = 0usize;
    let mut deferred = 0usize;
    let mut by_type: HashMap<QuestionType, (usize, usize)> = HashMap::new(); // (total, answered)
    let mut docs_with_questions: Vec<(String, String, usize, usize)> = Vec::with_capacity(32); // (id, title, total, answered)
    let mut filtered_count = 0usize;

    for repo in repos_to_process {
        let docs = db.get_documents_with_review_queue(Some(&repo.id))?;

        for doc in &docs {
            // Filter by modification time if --since is specified
            if let Some(ref since) = since_filter {
                let abs_path = repo.path.join(&doc.file_path);
                if !file_modified_since(&abs_path, since) {
                    filtered_count += 1;
                    continue;
                }
            }

            if let Some(questions) = parse_review_queue(&doc.content) {
                if questions.is_empty() {
                    continue;
                }

                let doc_total = questions.len();
                let doc_answered = questions.iter().filter(|q| q.answered).count();

                total += doc_total;
                answered += doc_answered;

                for q in &questions {
                    if q.is_deferred() {
                        deferred += 1;
                    }
                    let entry = by_type.entry(q.question_type).or_insert((0, 0));
                    entry.0 += 1;
                    if q.answered {
                        entry.1 += 1;
                    }
                }

                docs_with_questions.push((
                    doc.id.clone(),
                    doc.title.clone(),
                    doc_total,
                    doc_answered,
                ));
            }
        }
    }

    // Determine output format
    let format = OutputFormat::resolve(args.json, args.format);

    // Build output data
    let output = ReviewStatusJson {
        total,
        answered,
        unanswered: total - answered - deferred,
        deferred,
        by_type: by_type
            .iter()
            .map(|(k, (t, a))| {
                (
                    format!("{k:?}").to_lowercase(),
                    TypeStats {
                        total: *t,
                        answered: *a,
                    },
                )
            })
            .collect(),
        documents: docs_with_questions
            .iter()
            .map(|(id, title, t, a)| DocStats {
                id: id.clone(),
                title: title.clone(),
                total: *t,
                answered: *a,
            })
            .collect(),
    };

    let quiet = args.quiet;
    print_output(format, &output, || {
        if filtered_count > 0 && !quiet {
            println!("(Filtered {filtered_count} document(s) by --since)");
        }

        if total > 0 {
            println!("Review Status");
            println!("=============");
            println!("Total questions: {total}");
            println!("  Answered:   {answered} (ready to apply)");
            println!("  Deferred:   {deferred}");
            println!("  Unanswered: {}", total - answered - deferred);
            println!();
            println!("By type:");
            for (qtype, (t, a)) in &by_type {
                println!("  {qtype:?}: {t} ({a} answered)");
            }
            println!();
            println!("Documents with questions:");
            for (id, title, t, a) in &docs_with_questions {
                println!("  {title} [{id}]: {t} ({a} answered)");
            }
        } else {
            println!("No review questions found.");
            println!("Run `factbase check` to generate review questions.");
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_modified_since_recent_file() {
        // Create a temp file (will have current timestamp)
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        // Check against a time in the past - file should be "modified since"
        let past = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(file_modified_since(path, &past));
    }

    #[test]
    fn test_file_modified_since_future_time() {
        // Create a temp file
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        // Check against a time in the future - file should NOT be "modified since"
        let future = DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(!file_modified_since(path, &future));
    }

    #[test]
    fn test_file_modified_since_nonexistent_file() {
        // Non-existent file should return true (include if we can't read metadata)
        let path = Path::new("/nonexistent/path/to/file.md");
        let since = Utc::now();

        assert!(file_modified_since(path, &since));
    }

    #[test]
    fn test_type_stats_serialization() {
        let stats = TypeStats {
            total: 10,
            answered: 3,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"answered\":3"));
    }

    #[test]
    fn test_doc_stats_serialization() {
        let stats = DocStats {
            id: "abc123".to_string(),
            title: "Test Document".to_string(),
            total: 5,
            answered: 2,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"id\":\"abc123\""));
        assert!(json.contains("\"title\":\"Test Document\""));
        assert!(json.contains("\"total\":5"));
        assert!(json.contains("\"answered\":2"));
    }

    #[test]
    fn test_review_status_json_serialization() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "temporal".to_string(),
            TypeStats {
                total: 3,
                answered: 1,
            },
        );

        let status = ReviewStatusJson {
            total: 5,
            answered: 2,
            unanswered: 2,
            deferred: 1,
            by_type,
            documents: vec![DocStats {
                id: "doc1".to_string(),
                title: "First Doc".to_string(),
                total: 5,
                answered: 2,
            }],
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"total\":5"));
        assert!(json.contains("\"answered\":2"));
        assert!(json.contains("\"unanswered\":2"));
        assert!(json.contains("\"deferred\":1"));
        assert!(json.contains("\"temporal\""));
        assert!(json.contains("\"documents\""));
    }

    #[test]
    fn test_review_status_json_empty_collections() {
        let status = ReviewStatusJson {
            total: 0,
            answered: 0,
            unanswered: 0,
            deferred: 0,
            by_type: HashMap::new(),
            documents: Vec::new(),
        };

        let json = serde_json::to_string(&status).unwrap();
        // Empty collections should still serialize
        assert!(json.contains("\"by_type\":{}"));
        assert!(json.contains("\"documents\":[]"));
    }
}
