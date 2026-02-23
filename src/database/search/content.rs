//! Content search operations.
//!
//! Full-text search using SQL LIKE patterns with context extraction.

use super::{append_type_repo_filters, push_type_repo_params};
use crate::database::{decode_content_lossy, Database};
use crate::error::FactbaseError;
use crate::models::{ContentMatch, ContentSearchResult};
use chrono::{DateTime, Utc};

impl Database {
    /// Search document content for a pattern (case-insensitive).
    /// Returns matching documents with context around matches.
    pub fn search_content(
        &self,
        pattern: &str,
        limit: usize,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
        context_lines: usize,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<ContentSearchResult>, FactbaseError> {
        let conn = self.get_conn()?;

        let mut sql = String::from(
            "SELECT id, title, doc_type, file_path, repo_id, content
             FROM documents
             WHERE is_deleted = FALSE
             AND content LIKE ?1 COLLATE NOCASE",
        );

        let param_idx = append_type_repo_filters(&mut sql, 2, doc_type, repo_id, "");

        if since.is_some() {
            use std::fmt::Write;
            write!(sql, " AND indexed_at >= ?{}", param_idx).expect("write to String");
        }

        sql.push_str(&format!(" ORDER BY title LIMIT {}", limit));

        let mut stmt = conn.prepare_cached(&sql)?;
        let like_pattern = format!("%{}%", pattern);
        let pattern_lower = pattern.to_lowercase();
        let since_str = since.map(|dt| dt.to_rfc3339());

        let mut results = Vec::with_capacity(limit);

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&like_pattern];
        push_type_repo_params(&mut params, &doc_type, &repo_id);
        if let Some(ref s) = since_str {
            params.push(s);
        }

        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let stored_content: String = row.get(5)?;
            let content = decode_content_lossy(stored_content);

            // Find matches and extract context
            let matches = Self::find_matches_with_context(&content, &pattern_lower, context_lines);

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
    use crate::database::tests::{test_db, test_doc, test_repo};

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
            .search_content("TODO", 10, None, None, 0, None)
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
        let results = db
            .search_content("MATCH", 10, None, None, 1, None)
            .expect("search should succeed");

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
            .search_content("MATCH", 10, None, None, 0, None)
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
            .search_content("TODO", 10, None, None, 0, None)
            .expect("search should succeed");
        assert_eq!(
            results.len(),
            1,
            "should find document without since filter"
        );

        // Search with since filter in the future - should not find document
        let future = chrono::Utc::now() + chrono::Duration::hours(1);
        let results = db
            .search_content("TODO", 10, None, None, 0, Some(future))
            .expect("search should succeed");
        assert_eq!(
            results.len(),
            0,
            "should not find document with future since filter"
        );

        // Search with since filter in the past - should find document
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let results = db
            .search_content("TODO", 10, None, None, 0, Some(past))
            .expect("search should succeed");
        assert_eq!(
            results.len(),
            1,
            "should find document with past since filter"
        );
    }
}
