//! Content search operations.
//!
//! Full-text search using FTS5 for simple word/phrase patterns,
//! with LIKE fallback for regex patterns.

use super::{append_type_repo_filters, push_type_repo_params};
use crate::database::{decode_content_lossy, Database};
use crate::error::FactbaseError;
use crate::models::{ContentMatch, ContentSearchResult};
use crate::ProgressReporter;
use chrono::{DateTime, Utc};

/// Parameters for content search operations.
pub struct ContentSearchParams<'a> {
    pub pattern: &'a str,
    pub limit: usize,
    pub doc_type: Option<&'a str>,
    pub repo_id: Option<&'a str>,
    pub context_lines: usize,
    pub since: Option<DateTime<Utc>>,
    pub progress: &'a ProgressReporter,
}

/// Returns true if the pattern is a simple word/phrase suitable for FTS5 MATCH.
/// Returns false if it contains regex metacharacters or FTS5 operators.
fn is_fts_compatible(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '\'')
}

/// Escape a pattern for safe use in FTS5 MATCH by quoting it as a phrase.
fn fts5_phrase(pattern: &str) -> String {
    // Quote as a phrase to handle multi-word patterns and prevent FTS5 operator injection
    format!("\"{}\"", pattern.replace('"', "\"\""))
}

impl Database {
    /// Search document content for a pattern (case-insensitive).
    /// Returns matching documents with context around matches.
    /// Uses FTS5 index for simple word/phrase patterns, LIKE fallback for regex.
    pub fn search_content(
        &self,
        params: &ContentSearchParams,
    ) -> Result<Vec<ContentSearchResult>, FactbaseError> {
        if is_fts_compatible(params.pattern) {
            if let Ok(results) = self.search_content_fts5(params) {
                return Ok(results);
            }
            // Fall through to LIKE on FTS5 error (e.g., table missing)
        }
        self.search_content_like(params)
    }

    /// FTS5-based content search for simple word/phrase patterns.
    fn search_content_fts5(
        &self,
        params: &ContentSearchParams,
    ) -> Result<Vec<ContentSearchResult>, FactbaseError> {
        params.progress.log("Searching document content (FTS5)...");
        let conn = self.get_conn()?;

        let fts_query = fts5_phrase(params.pattern);

        let mut sql = String::from(
            "SELECT d.id, d.title, d.doc_type, d.file_path, d.repo_id, d.content
             FROM document_content_fts fts
             INNER JOIN documents d ON d.id = fts.doc_id
             WHERE fts.content MATCH ?1
             AND d.is_deleted = FALSE",
        );

        let param_idx =
            append_type_repo_filters(&mut sql, 2, params.doc_type, params.repo_id, "d.");
        {
            if params.since.is_some() {
                write_str!(sql, " AND d.indexed_at >= ?{}", param_idx);
            }
            write_str!(sql, " ORDER BY d.title LIMIT {}", params.limit);
        }

        let mut stmt = conn.prepare_cached(&sql)?;
        let pattern_lower = params.pattern.to_lowercase();
        let since_str = params.since.map(|dt| dt.to_rfc3339());

        let mut db_params: Vec<&dyn rusqlite::ToSql> = vec![&fts_query];
        push_type_repo_params(&mut db_params, &params.doc_type, &params.repo_id);
        if let Some(ref s) = since_str {
            db_params.push(s);
        }

        let mut results = Vec::with_capacity(params.limit);
        let mut rows = stmt.query(db_params.as_slice())?;
        while let Some(row) = rows.next()? {
            let stored_content: String = row.get(5)?;
            let content = decode_content_lossy(stored_content);
            let matches =
                Self::find_matches_with_context(&content, &pattern_lower, params.context_lines);
            if !matches.is_empty() {
                results.push(ContentSearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    doc_type: row.get(2).ok(),
                    file_path: row.get(3)?,
                    repo_id: row.get(4)?,
                    matches,
                });
            }
        }
        Ok(results)
    }

    /// LIKE-based content search (fallback for regex patterns).
    fn search_content_like(
        &self,
        params: &ContentSearchParams,
    ) -> Result<Vec<ContentSearchResult>, FactbaseError> {
        params.progress.log("Searching document content...");
        let conn = self.get_conn()?;

        let mut sql = String::from(
            "SELECT id, title, doc_type, file_path, repo_id, content
             FROM documents
             WHERE is_deleted = FALSE
             AND content LIKE ?1 COLLATE NOCASE",
        );

        let param_idx = append_type_repo_filters(&mut sql, 2, params.doc_type, params.repo_id, "");

        {
            if params.since.is_some() {
                write_str!(sql, " AND indexed_at >= ?{}", param_idx);
            }
            write_str!(sql, " ORDER BY title LIMIT {}", params.limit);
        }

        let mut stmt = conn.prepare_cached(&sql)?;
        let like_pattern = format!("%{}%", params.pattern);
        let pattern_lower = params.pattern.to_lowercase();
        let since_str = params.since.map(|dt| dt.to_rfc3339());

        let mut results = Vec::with_capacity(params.limit);

        let mut db_params: Vec<&dyn rusqlite::ToSql> = vec![&like_pattern];
        push_type_repo_params(&mut db_params, &params.doc_type, &params.repo_id);
        if let Some(ref s) = since_str {
            db_params.push(s);
        }

        let mut rows = stmt.query(db_params.as_slice())?;
        while let Some(row) = rows.next()? {
            let stored_content: String = row.get(5)?;
            let content = decode_content_lossy(stored_content);

            // Find matches and extract context
            let matches =
                Self::find_matches_with_context(&content, &pattern_lower, params.context_lines);

            if !matches.is_empty() {
                results.push(ContentSearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    doc_type: row.get(2).ok(),
                    file_path: row.get(3)?,
                    repo_id: row.get(4)?,
                    matches,
                });
            }
        }

        Ok(results)
    }

    /// Find all matches of pattern in content with surrounding context lines.
    fn find_matches_with_context(
        content: &str,
        pattern: &str,
        context_lines: usize,
    ) -> Vec<ContentMatch> {
        let lines: Vec<&str> = content.lines().collect();
        let mut matches = Vec::with_capacity(8);
        let mut seen_ranges: Vec<(usize, usize)> = Vec::with_capacity(8);

        for (line_num, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains(pattern) {
                let start = line_num.saturating_sub(context_lines);
                let end = (line_num + context_lines + 1).min(lines.len());

                // Check if this range overlaps with a previous one
                let overlaps = seen_ranges.iter().any(|(s, e)| start < *e && end > *s);
                if overlaps {
                    if let Some(last) = seen_ranges.last_mut() {
                        last.1 = last.1.max(end);
                    }
                    continue;
                }

                seen_ranges.push((start, end));

                let context: String = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(i, l)| format!("{}: {}", start + i + 1, l))
                    .collect::<Vec<_>>()
                    .join("\n");

                matches.push(ContentMatch {
                    line_number: line_num + 1,
                    line: line.to_string(),
                    context,
                });
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo};
    use crate::ProgressReporter;

    fn params<'a>(pattern: &'a str) -> ContentSearchParams<'a> {
        ContentSearchParams {
            pattern,
            limit: 10,
            doc_type: None,
            repo_id: None,
            context_lines: 0,
            since: None,
            progress: &ProgressReporter::Silent,
        }
    }

    #[test]
    fn test_is_fts_compatible() {
        assert!(is_fts_compatible("TODO"));
        assert!(is_fts_compatible("project status"));
        assert!(is_fts_compatible("hello-world"));
        assert!(is_fts_compatible("it's"));
        assert!(!is_fts_compatible(""));
        assert!(!is_fts_compatible("TODO.*fix"));
        assert!(!is_fts_compatible("test[0-9]"));
        assert!(!is_fts_compatible("(group)"));
        assert!(!is_fts_compatible("a|b"));
        assert!(!is_fts_compatible("end$"));
    }

    #[test]
    fn test_fts5_phrase() {
        assert_eq!(fts5_phrase("TODO"), "\"TODO\"");
        assert_eq!(fts5_phrase("project status"), "\"project status\"");
    }

    #[test]
    fn test_search_content_uses_fts5_for_simple_pattern() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        let mut doc = test_doc("abc123", "First Doc");
        doc.content = "Line 1: Hello world\nLine 2: This is a test\nLine 3: TODO fix this bug\nLine 4: More content".to_string();
        db.upsert_document(&doc).expect("upsert");

        // "TODO" is FTS-compatible — should use FTS5 path
        let results = db.search_content(&params("TODO")).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "abc123");
    }

    #[test]
    fn test_search_content_falls_back_to_like_for_regex() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "Error: file not found\nWarning: deprecated".to_string();
        db.upsert_document(&doc).expect("upsert");

        // Pattern with regex chars — should fall back to LIKE
        let results = db.search_content(&params("Error:")).expect("search");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_content_fts5_updated_content() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).expect("upsert repo");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "Original content here".to_string();
        db.upsert_document(&doc).expect("upsert");

        // Update content
        db.update_document_content("abc123", "Updated content with KEYWORD", "newhash")
            .expect("update");

        // FTS5 should find the updated content
        let results = db.search_content(&params("KEYWORD")).expect("search");
        assert_eq!(results.len(), 1);

        // FTS5 should NOT find the old content
        let results = db.search_content(&params("Original")).expect("search");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_content() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert repo should succeed");

        // Create documents with searchable content
        let mut doc1 = test_doc("abc123", "First Doc");
        doc1.content = "Line 1: Hello world\nLine 2: This is a test\nLine 3: TODO fix this bug\nLine 4: More content".to_string();
        db.upsert_document(&doc1).expect("upsert should succeed");

        let mut doc2 = test_doc("def456", "Second Doc");
        doc2.content = "No matches here\nJust regular content".to_string();
        db.upsert_document(&doc2).expect("upsert should succeed");

        // Search for "TODO"
        let results = db
            .search_content(&params("TODO"))
            .expect("search should succeed");

        assert_eq!(results.len(), 1, "should find 1 document");
        assert_eq!(results[0].id, "abc123");
        assert_eq!(results[0].matches.len(), 1, "should have 1 match");
        assert_eq!(results[0].matches[0].line_number, 3);
        assert!(results[0].matches[0].line.contains("TODO"));
    }

    #[test]
    fn test_search_content_with_context() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert repo should succeed");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content =
            "Line 1: Header\nLine 2: Before\nLine 3: MATCH here\nLine 4: After\nLine 5: Footer"
                .to_string();
        db.upsert_document(&doc).expect("upsert should succeed");

        // Search with context=1
        let p = ContentSearchParams {
            context_lines: 1,
            ..params("MATCH")
        };
        let results = db.search_content(&p).expect("search should succeed");

        assert_eq!(results.len(), 1, "should find 1 document");
        assert_eq!(results[0].matches.len(), 1, "should have 1 match");

        let context = &results[0].matches[0].context;
        assert!(
            context.contains("Line 2: Before"),
            "should include line before"
        );
        assert!(context.contains("MATCH"), "should include match line");
        assert!(
            context.contains("Line 4: After"),
            "should include line after"
        );
    }

    #[test]
    fn test_search_content_zero_context() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert repo should succeed");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "Line 1\nMATCH\nLine 3".to_string();
        db.upsert_document(&doc).expect("upsert should succeed");

        let results = db
            .search_content(&params("MATCH"))
            .expect("search should succeed");

        assert_eq!(results.len(), 1, "should find 1 document");
        let context = &results[0].matches[0].context;
        assert!(context.contains("MATCH"), "should include match line");
        assert!(
            !context.contains("Line 1"),
            "should not include line before"
        );
        assert!(!context.contains("Line 3"), "should not include line after");
    }

    #[test]
    fn test_search_content_with_since_filter() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert repo should succeed");

        let mut doc = test_doc("abc123", "Test Doc");
        doc.content = "Line with TODO marker".to_string();
        db.upsert_document(&doc).expect("upsert should succeed");

        // Search without since filter - should find document
        let results = db
            .search_content(&params("TODO"))
            .expect("search should succeed");
        assert_eq!(
            results.len(),
            1,
            "should find document without since filter"
        );

        // Search with since filter in the future - should not find document
        let future = chrono::Utc::now() + chrono::Duration::hours(1);
        let p = ContentSearchParams {
            since: Some(future),
            ..params("TODO")
        };
        let results = db.search_content(&p).expect("search should succeed");
        assert_eq!(
            results.len(),
            0,
            "should not find document with future since filter"
        );

        // Search with since filter in the past - should find document
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let p = ContentSearchParams {
            since: Some(past),
            ..params("TODO")
        };
        let results = db.search_content(&p).expect("search should succeed");
        assert_eq!(
            results.len(),
            1,
            "should find document with past since filter"
        );
    }
}
