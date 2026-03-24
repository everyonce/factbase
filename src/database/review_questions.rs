//! Review questions DB table operations.
//!
//! Provides fast indexed access to review questions without parsing markdown files.

use crate::error::FactbaseError;
use crate::models::ReviewQuestion;
use serde_json::Value;
use std::collections::HashSet;

use super::Database;

/// Parameters for querying review questions from the DB.
#[derive(Debug, Default)]
pub struct ReviewQueueDbParams {
    pub repo_id: Option<String>,
    pub doc_id: Option<String>,
    pub question_type: Option<String>,
    /// "unanswered" (default), "deferred", "answered", "all"
    pub status_filter: String,
    pub limit: usize,
    pub offset: usize,
}

impl Database {
    /// Sync review questions for a document from parsed questions.
    ///
    /// Replaces all questions for the document, preserving 'dismissed' status
    /// for questions whose description matches a previously dismissed question.
    pub fn sync_review_questions(
        &self,
        doc_id: &str,
        questions: &[ReviewQuestion],
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let now = chrono::Utc::now().to_rfc3339();

        // Preserve dismissed questions by description (stable across index shifts)
        let dismissed: HashSet<String> = conn
            .prepare(
                "SELECT description FROM review_questions WHERE doc_id = ?1 AND status = 'dismissed'",
            )?
            .query_map([doc_id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();

        // Replace all questions for this doc
        conn.execute("DELETE FROM review_questions WHERE doc_id = ?1", [doc_id])?;

        for (idx, q) in questions.iter().enumerate() {
            let status = if dismissed.contains(&q.description) {
                "dismissed"
            } else if q.answered {
                "verified"
            } else if q.is_deferred() {
                // is_believed() is a subset of is_deferred(); both map to "deferred"
                "deferred"
            } else {
                "open"
            };

            conn.execute(
                "INSERT INTO review_questions \
                 (doc_id, question_index, question_type, description, line_ref, answer, status, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                rusqlite::params![
                    doc_id,
                    idx as i64,
                    q.question_type.as_str(),
                    q.description,
                    q.line_ref.map(|l| l as i64),
                    q.answer,
                    status,
                    now
                ],
            )?;
        }

        Ok(())
    }

    /// Query review questions from DB with filters.
    ///
    /// Returns `(questions_json, total_answered, total_unanswered, total_deferred)`.
    pub fn query_review_questions_db(
        &self,
        params: &ReviewQueueDbParams,
    ) -> Result<(Vec<Value>, u64, u64, u64), FactbaseError> {
        let conn = self.get_conn()?;
        let limit = if params.limit == 0 { 10 } else { params.limit };
        let offset = params.offset;
        let status_filter = if params.status_filter.is_empty() {
            "unanswered"
        } else {
            params.status_filter.as_str()
        };

        // Build dynamic WHERE conditions
        let mut conditions: Vec<String> = vec!["rq.status != 'dismissed'".to_string()];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref repo_id) = params.repo_id {
            args.push(Box::new(repo_id.clone()));
            conditions.push(format!("d.repo_id = ?{}", args.len()));
        }
        if let Some(ref doc_id) = params.doc_id {
            args.push(Box::new(doc_id.clone()));
            conditions.push(format!("rq.doc_id = ?{}", args.len()));
        }
        if let Some(ref qt) = params.question_type {
            args.push(Box::new(qt.clone()));
            conditions.push(format!("rq.question_type = ?{}", args.len()));
        }

        let base_where = conditions.join(" AND ");

        // Count totals (all non-dismissed, matching repo/doc/type filters)
        let count_sql = format!(
            "SELECT \
               COALESCE(SUM(CASE WHEN rq.status = 'verified' THEN 1 ELSE 0 END), 0), \
               COALESCE(SUM(CASE WHEN rq.status = 'open' THEN 1 ELSE 0 END), 0), \
               COALESCE(SUM(CASE WHEN rq.status IN ('deferred', 'believed') THEN 1 ELSE 0 END), 0) \
             FROM review_questions rq \
             JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
             WHERE {base_where}"
        );

        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let (total_answered, total_unanswered, total_deferred): (u64, u64, u64) =
            conn.query_row(&count_sql, refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;

        // Add status filter for the page query
        let mut page_conditions = conditions.clone();
        match status_filter {
            "answered" => {
                page_conditions.push("rq.status = 'verified'".to_string());
            }
            "deferred" => {
                page_conditions.push("rq.status IN ('deferred', 'believed')".to_string());
            }
            "all" => {} // no extra condition
            _ => {
                // "unanswered" = open
                page_conditions.push("rq.status = 'open'".to_string());
            }
        }
        let page_where = page_conditions.join(" AND ");

        args.push(Box::new(limit as i64));
        args.push(Box::new(offset as i64));
        let limit_idx = args.len() - 1;
        let offset_idx = args.len();

        let page_sql = format!(
            "SELECT rq.doc_id, d.title, rq.question_index, rq.question_type, \
                    rq.description, rq.line_ref, rq.answer, rq.status, \
                    rq.confidence, rq.agent_suggestion \
             FROM review_questions rq \
             JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
             WHERE {page_where} \
             ORDER BY rq.doc_id, rq.question_index \
             LIMIT ?{} OFFSET ?{}",
            limit_idx, offset_idx
        );

        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let mut stmt = conn.prepare(&page_sql)?;
        let questions: Vec<Value> = stmt
            .query_map(refs.as_slice(), |row| {
                let doc_id: String = row.get(0)?;
                let doc_title: String = row.get(1)?;
                let question_index: i64 = row.get(2)?;
                let question_type: String = row.get(3)?;
                let description: String = row.get(4)?;
                let line_ref: Option<i64> = row.get(5)?;
                let answer: Option<String> = row.get(6)?;
                let status: String = row.get(7)?;
                let confidence: Option<String> = row.get(8)?;
                let agent_suggestion: Option<String> = row.get(9)?;
                Ok((
                    doc_id,
                    doc_title,
                    question_index,
                    question_type,
                    description,
                    line_ref,
                    answer,
                    status,
                    confidence,
                    agent_suggestion,
                ))
            })?
            .filter_map(Result::ok)
            .map(
                |(
                    doc_id,
                    doc_title,
                    question_index,
                    question_type,
                    description,
                    line_ref,
                    answer,
                    status,
                    confidence,
                    agent_suggestion,
                )| {
                    let is_deferred = status == "deferred" || status == "believed";
                    let answered = status == "verified";
                    // confidence defaults to "deferred" when not set (no attempt made)
                    let confidence_val = confidence.as_deref().unwrap_or("deferred");
                    let mut obj = serde_json::json!({
                        "type": question_type,
                        "description": description,
                        "line_ref": line_ref,
                        "doc_id": doc_id,
                        "doc_title": doc_title,
                        "question_index": question_index,
                        "answered": answered,
                        "answer": answer,
                        "status": status,
                        "confidence": confidence_val,
                        "agent_suggestion": agent_suggestion,
                    });
                    if is_deferred {
                        obj["deferred"] = Value::Bool(true);
                    }
                    obj
                },
            )
            .collect();

        Ok((questions, total_answered, total_unanswered, total_deferred))
    }

    /// Update the status of a single review question in the DB.
    pub fn update_review_question_status(
        &self,
        doc_id: &str,
        question_index: usize,
        status: &str,
        answer: Option<&str>,
    ) -> Result<(), FactbaseError> {
        self.update_review_question_status_with_confidence(
            doc_id,
            question_index,
            status,
            answer,
            None,
            None,
        )
    }

    /// Update the status of a single review question in the DB, including confidence metadata.
    pub fn update_review_question_status_with_confidence(
        &self,
        doc_id: &str,
        question_index: usize,
        status: &str,
        answer: Option<&str>,
        confidence: Option<&str>,
        agent_suggestion: Option<&str>,
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE review_questions SET status = ?1, answer = ?2, updated_at = ?3, \
             confidence = ?4, agent_suggestion = ?5 \
             WHERE doc_id = ?6 AND question_index = ?7",
            rusqlite::params![
                status,
                answer,
                now,
                confidence,
                agent_suggestion,
                doc_id,
                question_index as i64
            ],
        )?;
        Ok(())
    }

    /// Bulk update review question status by type and/or description pattern.
    ///
    /// Returns the number of rows updated.
    pub fn bulk_update_review_question_status(
        &self,
        doc_id_filter: Option<&str>,
        type_filter: Option<&str>,
        desc_filter: Option<&str>,
        status: &str,
    ) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let now = chrono::Utc::now().to_rfc3339();

        let mut conditions = vec!["1=1".to_string()];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(doc_id) = doc_id_filter {
            args.push(Box::new(doc_id.to_string()));
            conditions.push(format!("doc_id = ?{}", args.len()));
        }
        if let Some(qt) = type_filter {
            args.push(Box::new(qt.to_string()));
            conditions.push(format!("question_type = ?{}", args.len()));
        }
        if let Some(desc) = desc_filter {
            args.push(Box::new(desc.to_string()));
            conditions.push(format!("description LIKE ?{}", args.len()));
        }

        args.push(Box::new(status.to_string()));
        let status_idx = args.len();
        args.push(Box::new(now));
        let now_idx = args.len();

        let sql = format!(
            "UPDATE review_questions SET status = ?{status_idx}, updated_at = ?{now_idx} \
             WHERE {}",
            conditions.join(" AND ")
        );

        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let rows = conn.execute(&sql, refs.as_slice())?;
        Ok(rows)
    }

    /// Count review questions by status for a repo (fast, no file parsing).
    pub fn count_review_questions_by_status(
        &self,
        repo_id: Option<&str>,
    ) -> Result<(u64, u64, u64), FactbaseError> {
        let conn = self.get_conn()?;
        let (answered, unanswered, deferred): (u64, u64, u64) = if let Some(rid) = repo_id {
            conn.query_row(
                "SELECT \
                   COALESCE(SUM(CASE WHEN rq.status = 'verified' THEN 1 ELSE 0 END), 0), \
                   COALESCE(SUM(CASE WHEN rq.status = 'open' THEN 1 ELSE 0 END), 0), \
                   COALESCE(SUM(CASE WHEN rq.status IN ('deferred', 'believed') THEN 1 ELSE 0 END), 0) \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE d.repo_id = ?1 AND rq.status != 'dismissed'",
                [rid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?
        } else {
            conn.query_row(
                "SELECT \
                   COALESCE(SUM(CASE WHEN status = 'verified' THEN 1 ELSE 0 END), 0), \
                   COALESCE(SUM(CASE WHEN status = 'open' THEN 1 ELSE 0 END), 0), \
                   COALESCE(SUM(CASE WHEN status IN ('deferred', 'believed') THEN 1 ELSE 0 END), 0) \
                 FROM review_questions \
                 WHERE status != 'dismissed'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?
        };
        Ok((answered, unanswered, deferred))
    }

    /// Reset deferred/believed questions of a given type back to open status.
    ///
    /// Clears both `status` (→ 'open') and `answer` (→ NULL) for all questions
    /// matching `question_type` whose current status is 'deferred' or 'believed'.
    ///
    /// Returns `(count, Vec<(doc_id, file_path)>)` for the affected documents so
    /// callers can also strip the blockquote answers from the markdown files.
    pub fn reset_deferred_questions_by_type(
        &self,
        question_type: &str,
        repo_id: Option<&str>,
    ) -> Result<(usize, Vec<(String, String)>), FactbaseError> {
        let conn = self.get_conn()?;
        let now = chrono::Utc::now().to_rfc3339();

        // Collect affected (doc_id, file_path) before updating
        let affected: Vec<(String, String)> = if let Some(rid) = repo_id {
            conn.prepare(
                "SELECT DISTINCT rq.doc_id, d.file_path \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE rq.question_type = ?1 \
                   AND rq.status IN ('deferred', 'believed') \
                   AND d.repo_id = ?2",
            )?
            .query_map([question_type, rid], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(Result::ok)
            .collect()
        } else {
            conn.prepare(
                "SELECT DISTINCT rq.doc_id, d.file_path \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE rq.question_type = ?1 \
                   AND rq.status IN ('deferred', 'believed')",
            )?
            .query_map([question_type], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(Result::ok)
            .collect()
        };

        if affected.is_empty() {
            return Ok((0, vec![]));
        }

        // Reset status and clear answer
        let count = if let Some(rid) = repo_id {
            conn.execute(
                "UPDATE review_questions SET status = 'open', answer = NULL, updated_at = ?1 \
                 WHERE question_type = ?2 \
                   AND status IN ('deferred', 'believed') \
                   AND doc_id IN (SELECT id FROM documents WHERE repo_id = ?3 AND is_deleted = FALSE)",
                rusqlite::params![now, question_type, rid],
            )?
        } else {
            conn.execute(
                "UPDATE review_questions SET status = 'open', answer = NULL, updated_at = ?1 \
                 WHERE question_type = ?2 AND status IN ('deferred', 'believed')",
                rusqlite::params![now, question_type],
            )?
        };

        Ok((count, affected))
    }

    /// Load review questions for a document from the DB as `ReviewQuestion` structs.
    ///
    /// Returns questions ordered by `question_index`, excluding dismissed ones.
    /// Used as a fallback when inline `@q[]` markers are missing from the document.
    pub fn get_review_questions_for_doc(
        &self,
        doc_id: &str,
    ) -> Result<Vec<ReviewQuestion>, FactbaseError> {
        let conn = self.get_conn()?;
        let questions = conn
            .prepare(
                "SELECT question_type, line_ref, description, answer, status \
                 FROM review_questions \
                 WHERE doc_id = ?1 AND status != 'dismissed' \
                 ORDER BY question_index",
            )?
            .query_map([doc_id], |row| {
                let qt_str: String = row.get(0)?;
                let line_ref: Option<i64> = row.get(1)?;
                let description: String = row.get(2)?;
                let answer: Option<String> = row.get(3)?;
                let status: String = row.get(4)?;
                Ok((qt_str, line_ref, description, answer, status))
            })?
            .filter_map(Result::ok)
            .map(|(qt_str, line_ref, description, answer, status)| {
                let question_type = qt_str
                    .parse::<crate::models::QuestionType>()
                    .unwrap_or(crate::models::QuestionType::Missing);
                let answered = status == "verified";
                let mut q =
                    ReviewQuestion::new(question_type, line_ref.map(|n| n as usize), description);
                q.answered = answered;
                q.answer = answer;
                q
            })
            .collect();
        Ok(questions)
    }

    /// Count open (unanswered) review questions grouped by question_type.
    ///
    /// Returns a map of question_type → count for all non-dismissed, open questions.
    pub fn count_open_questions_by_type(
        &self,
        repo_id: Option<&str>,
    ) -> Result<std::collections::HashMap<String, u64>, FactbaseError> {
        let conn = self.get_conn()?;
        let rows: Vec<(String, u64)> = if let Some(rid) = repo_id {
            conn.prepare(
                "SELECT rq.question_type, COUNT(*) \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE d.repo_id = ?1 AND rq.status = 'open' \
                 GROUP BY rq.question_type",
            )?
            .query_map([rid], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(Result::ok)
            .collect()
        } else {
            conn.prepare(
                "SELECT question_type, COUNT(*) FROM review_questions \
                 WHERE status = 'open' GROUP BY question_type",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(Result::ok)
            .collect()
        };
        Ok(rows.into_iter().collect())
    }

    /// Count documents with has_review_queue = TRUE (fast, no file parsing).
    pub fn count_docs_with_review_queue(
        &self,
        repo_id: Option<&str>,
    ) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count: usize = if let Some(rid) = repo_id {
            conn.query_row(
                "SELECT COUNT(*) FROM documents WHERE repo_id = ?1 AND has_review_queue = TRUE AND is_deleted = FALSE",
                [rid],
                |r| r.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM documents WHERE has_review_queue = TRUE AND is_deleted = FALSE",
                [],
                |r| r.get(0),
            )?
        };
        Ok(count)
    }

    /// Get open high-confidence conflict questions with document titles.
    ///
    /// Returns `(doc_id, doc_title, description)` tuples for open conflict questions
    /// matching the `same_entity_transition` pattern (highest confidence).
    /// Used by the maintain workflow report to surface actionable conflict hints.
    pub fn get_conflict_hints(
        &self,
        repo_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, FactbaseError> {
        let conn = self.get_conn()?;
        let limit = limit.max(1) as i64;
        let rows: Vec<(String, String, String)> = if let Some(rid) = repo_id {
            conn.prepare(
                "SELECT rq.doc_id, d.title, rq.description \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE d.repo_id = ?1 AND rq.question_type = 'conflict' \
                   AND rq.status = 'open' \
                   AND rq.description LIKE '%[pattern:same_entity_transition]%' \
                 ORDER BY rq.created_at DESC \
                 LIMIT ?2",
            )?
            .query_map(rusqlite::params![rid, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .filter_map(Result::ok)
            .collect()
        } else {
            conn.prepare(
                "SELECT rq.doc_id, d.title, rq.description \
                 FROM review_questions rq \
                 JOIN documents d ON rq.doc_id = d.id AND d.is_deleted = FALSE \
                 WHERE rq.question_type = 'conflict' \
                   AND rq.status = 'open' \
                   AND rq.description LIKE '%[pattern:same_entity_transition]%' \
                 ORDER BY rq.created_at DESC \
                 LIMIT ?1",
            )?
            .query_map(rusqlite::params![limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .filter_map(Result::ok)
            .collect()
        };
        Ok(rows)
    }

    /// Check if the review_questions table has any rows (used to detect if populated).
    pub fn has_review_questions_indexed(&self) -> bool {
        let conn = match self.get_conn() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row("SELECT COUNT(*) > 0 FROM review_questions", [], |row| {
            row.get::<_, bool>(0)
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc_with_repo, test_repo_with_id};
    use crate::models::{QuestionType, ReviewQuestion};

    fn make_question(qt: QuestionType, desc: &str) -> ReviewQuestion {
        ReviewQuestion::new(qt, None, desc.to_string())
    }

    fn make_deferred_question(qt: QuestionType, desc: &str) -> ReviewQuestion {
        let mut q = ReviewQuestion::new(qt, None, desc.to_string());
        q.answer = Some("defer: needs research".to_string());
        q
    }

    fn make_answered_question(qt: QuestionType, desc: &str) -> ReviewQuestion {
        let mut q = ReviewQuestion::new(qt, None, desc.to_string());
        q.answered = true;
        q.answer = Some("2024".to_string());
        q
    }

    fn setup(db: &Database) -> String {
        let repo = test_repo_with_id("r1");
        db.upsert_repository(&repo).unwrap();
        let doc = test_doc_with_repo("doc1", "r1", "Test Doc");
        db.upsert_document(&doc).unwrap();
        "doc1".to_string()
    }

    #[test]
    fn test_sync_review_questions_basic() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::Temporal, "When did this happen?"),
            make_question(QuestionType::Missing, "What is the source?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "unanswered".to_string(),
            ..Default::default()
        };
        let (qs, answered, unanswered, deferred) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 2);
        assert_eq!(unanswered, 2);
        assert_eq!(answered, 0);
        assert_eq!(deferred, 0);
    }

    #[test]
    fn test_sync_preserves_dismissed_status() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        // First sync: 2 questions
        let questions = vec![
            make_question(QuestionType::WeakSource, "Phonetool source"),
            make_question(QuestionType::Temporal, "When?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        // Dismiss the weak-source question
        db.update_review_question_status(&doc_id, 0, "dismissed", None)
            .unwrap();

        // Re-sync (simulating a rescan with same questions)
        db.sync_review_questions(&doc_id, &questions).unwrap();

        // Dismissed status should be preserved
        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "all".to_string(),
            ..Default::default()
        };
        let (qs, _, unanswered, _) = db.query_review_questions_db(&params).unwrap();
        // Only 1 question visible (dismissed is excluded from 'all' query)
        assert_eq!(qs.len(), 1);
        assert_eq!(unanswered, 1);
        assert_eq!(qs[0]["type"], "temporal");
    }

    #[test]
    fn test_sync_status_from_question_state() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::Temporal, "Open question"),
            make_deferred_question(QuestionType::Missing, "Deferred question"),
            make_answered_question(QuestionType::Stale, "Answered question"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "all".to_string(),
            ..Default::default()
        };
        let (_, answered, unanswered, deferred) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(unanswered, 1);
        assert_eq!(deferred, 1);
        assert_eq!(answered, 1);
    }

    #[test]
    fn test_query_filter_by_type() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::Temporal, "When?"),
            make_question(QuestionType::WeakSource, "Source?"),
            make_question(QuestionType::Temporal, "When again?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "unanswered".to_string(),
            question_type: Some("temporal".to_string()),
            ..Default::default()
        };
        let (qs, _, _, _) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 2);
        assert!(qs.iter().all(|q| q["type"] == "temporal"));
    }

    #[test]
    fn test_query_filter_by_doc_id() {
        let (db, _tmp) = test_db();
        let repo = test_repo_with_id("r1");
        db.upsert_repository(&repo).unwrap();
        let doc1 = test_doc_with_repo("doc1", "r1", "Doc 1");
        let doc2 = test_doc_with_repo("doc2", "r1", "Doc 2");
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        db.sync_review_questions("doc1", &[make_question(QuestionType::Temporal, "Q1")])
            .unwrap();
        db.sync_review_questions("doc2", &[make_question(QuestionType::Missing, "Q2")])
            .unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "unanswered".to_string(),
            doc_id: Some("doc1".to_string()),
            ..Default::default()
        };
        let (qs, _, _, _) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["doc_id"], "doc1");
    }

    #[test]
    fn test_bulk_update_by_type() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::WeakSource, "Phonetool source A"),
            make_question(QuestionType::WeakSource, "Phonetool source B"),
            make_question(QuestionType::Temporal, "When?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let rows = db
            .bulk_update_review_question_status(None, Some("weak-source"), None, "dismissed")
            .unwrap();
        assert_eq!(rows, 2);

        let (_, unanswered, _) = db.count_review_questions_by_status(None).unwrap();
        assert_eq!(unanswered, 1); // only temporal remains
    }

    #[test]
    fn test_bulk_update_by_desc_pattern() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::WeakSource, "Phonetool profile link"),
            make_question(QuestionType::WeakSource, "LinkedIn profile"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let rows = db
            .bulk_update_review_question_status(None, None, Some("%Phonetool%"), "dismissed")
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_count_by_status() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(QuestionType::Temporal, "Open"),
            make_deferred_question(QuestionType::Missing, "Deferred"),
            make_answered_question(QuestionType::Stale, "Answered"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let (answered, unanswered, deferred) = db.count_review_questions_by_status(None).unwrap();
        assert_eq!(answered, 1);
        assert_eq!(unanswered, 1);
        assert_eq!(deferred, 1);
    }

    #[test]
    fn test_sync_empty_questions_clears_doc() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        db.sync_review_questions(&doc_id, &[make_question(QuestionType::Temporal, "Q1")])
            .unwrap();
        db.sync_review_questions(&doc_id, &[]).unwrap();

        let (_, unanswered, _) = db.count_review_questions_by_status(None).unwrap();
        assert_eq!(unanswered, 0);
    }

    #[test]
    fn test_reset_deferred_questions_by_type_basic() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_deferred_question(QuestionType::WeakSource, "Vague citation A"),
            make_deferred_question(QuestionType::WeakSource, "Vague citation B"),
            make_deferred_question(QuestionType::Temporal, "When?"),
        ];
        // Mark first two as believed
        db.sync_review_questions(&doc_id, &questions).unwrap();
        db.update_review_question_status(&doc_id, 0, "believed", Some("believed: ok"))
            .unwrap();
        db.update_review_question_status(&doc_id, 1, "deferred", Some("defer: no url"))
            .unwrap();

        let (count, affected) = db
            .reset_deferred_questions_by_type("weak-source", None)
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].0, doc_id);

        // Verify DB state
        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "unanswered".to_string(),
            question_type: Some("weak-source".to_string()),
            ..Default::default()
        };
        let (qs, _, unanswered, deferred) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 2);
        assert_eq!(unanswered, 2);
        assert_eq!(deferred, 0);
        // Answers should be cleared
        assert!(qs.iter().all(|q| q["answer"].is_null()));
    }

    #[test]
    fn test_reset_deferred_questions_by_type_only_target_type() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_deferred_question(QuestionType::WeakSource, "Vague citation"),
            make_deferred_question(QuestionType::Temporal, "When?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();
        db.update_review_question_status(&doc_id, 0, "believed", Some("believed: ok"))
            .unwrap();
        db.update_review_question_status(&doc_id, 1, "deferred", Some("defer: no date"))
            .unwrap();

        let (count, _) = db
            .reset_deferred_questions_by_type("weak-source", None)
            .unwrap();
        assert_eq!(count, 1);

        // Temporal deferred should still be deferred
        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "deferred".to_string(),
            ..Default::default()
        };
        let (_, _, _, deferred) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(deferred, 1);
    }

    #[test]
    fn test_reset_deferred_questions_by_type_noop_when_none() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        db.sync_review_questions(
            &doc_id,
            &[make_question(QuestionType::WeakSource, "Open question")],
        )
        .unwrap();

        let (count, affected) = db
            .reset_deferred_questions_by_type("weak-source", None)
            .unwrap();
        assert_eq!(count, 0);
        assert!(affected.is_empty());
    }

    #[test]
    fn test_confidence_and_agent_suggestion_stored_and_returned() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![make_question(QuestionType::Temporal, "When?")];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        // Store confidence + agent_suggestion via the new method
        db.update_review_question_status_with_confidence(
            &doc_id,
            0,
            "verified",
            Some("@t[2024]"),
            Some("high"),
            Some("The date is clearly stated in the document."),
        )
        .unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "answered".to_string(),
            ..Default::default()
        };
        let (qs, _, _, _) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["confidence"], "high");
        assert_eq!(
            qs[0]["agent_suggestion"],
            "The date is clearly stated in the document."
        );
    }

    #[test]
    fn test_confidence_defaults_to_deferred_when_null() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![make_question(QuestionType::Temporal, "When?")];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        // No confidence set — should default to "deferred" in API response
        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "unanswered".to_string(),
            ..Default::default()
        };
        let (qs, _, _, _) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["confidence"], "deferred");
        assert!(qs[0]["agent_suggestion"].is_null());
    }

    #[test]
    fn test_low_confidence_stored_correctly() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![make_question(QuestionType::Missing, "Source?")];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        db.update_review_question_status_with_confidence(
            &doc_id,
            0,
            "verified",
            Some("Wikipedia article"),
            Some("low"),
            Some("Found a possible match but not certain."),
        )
        .unwrap();

        let params = ReviewQueueDbParams {
            limit: 10,
            status_filter: "answered".to_string(),
            ..Default::default()
        };
        let (qs, _, _, _) = db.query_review_questions_db(&params).unwrap();
        assert_eq!(qs[0]["confidence"], "low");
        assert_eq!(
            qs[0]["agent_suggestion"],
            "Found a possible match but not certain."
        );
    }

    #[test]
    fn test_migration_v21_adds_columns_to_existing_db() {
        // Simulate a v20 database and verify migration v21 adds the columns
        let temp = tempfile::TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");

        {
            let conn = rusqlite::Connection::open(&db_path).expect("open connection");
            conn.execute_batch(
                "CREATE TABLE review_questions (
                    id INTEGER PRIMARY KEY,
                    doc_id TEXT NOT NULL,
                    question_index INTEGER NOT NULL,
                    question_type TEXT NOT NULL,
                    description TEXT NOT NULL,
                    line_ref INTEGER,
                    answer TEXT,
                    status TEXT NOT NULL DEFAULT 'open',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                PRAGMA user_version = 20;",
            )
            .expect("create table");
        }

        // Opening the database should apply migration v21
        let db = crate::database::Database::new(&db_path).expect("open database");
        let conn = db.get_conn().expect("get connection");

        let has_confidence: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('review_questions') WHERE name = 'confidence'",
                [],
                |row| row.get(0),
            )
            .expect("query column");
        assert!(
            has_confidence,
            "confidence column should exist after migration v21"
        );

        let has_agent_suggestion: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('review_questions') WHERE name = 'agent_suggestion'",
                [],
                |row| row.get(0),
            )
            .expect("query column");
        assert!(
            has_agent_suggestion,
            "agent_suggestion column should exist after migration v21"
        );
    }

    #[test]
    fn test_migration_v21_idempotent() {
        // Running migration v21 twice should not fail
        let temp = tempfile::TempDir::new().expect("create temp dir");
        let db_path = temp.path().join("test.db");
        let _db = crate::database::Database::new(&db_path).expect("first open");
        // Second open should succeed without error
        let _db2 = crate::database::Database::new(&db_path).expect("second open");
    }

    #[test]
    fn test_has_review_questions_indexed() {
        let (db, _tmp) = test_db();
        assert!(!db.has_review_questions_indexed());

        let doc_id = setup(&db);
        db.sync_review_questions(&doc_id, &[make_question(QuestionType::Temporal, "Q1")])
            .unwrap();
        assert!(db.has_review_questions_indexed());
    }

    #[test]
    fn test_get_conflict_hints_returns_same_entity_transition_only() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions = vec![
            make_question(
                QuestionType::Conflict,
                "\"fact A\" @t[2020..2022] overlaps with \"fact B\" @t[2021..2023] - were both true simultaneously? (line:5) [pattern:same_entity_transition]",
            ),
            make_question(
                QuestionType::Conflict,
                "\"fact C\" @t[2020..2022] overlaps with \"fact D\" @t[2021..2023] - were both true simultaneously? (line:8) [pattern:parallel_overlap]",
            ),
            make_question(QuestionType::Temporal, "When?"),
        ];
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let hints = db.get_conflict_hints(None, 10).unwrap();
        assert_eq!(hints.len(), 1);
        assert!(hints[0].2.contains("[pattern:same_entity_transition]"));
    }

    #[test]
    fn test_get_conflict_hints_excludes_non_open() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let desc = "\"fact A\" @t[2020..2022] overlaps with \"fact B\" @t[2021..2023] - were both true simultaneously? (line:5) [pattern:same_entity_transition]";
        db.sync_review_questions(&doc_id, &[make_question(QuestionType::Conflict, desc)])
            .unwrap();
        db.update_review_question_status(&doc_id, 0, "dismissed", None)
            .unwrap();

        let hints = db.get_conflict_hints(None, 10).unwrap();
        assert!(hints.is_empty());
    }

    #[test]
    fn test_get_conflict_hints_respects_limit() {
        let (db, _tmp) = test_db();
        let doc_id = setup(&db);

        let questions: Vec<ReviewQuestion> = (0..5)
            .map(|i| {
                make_question(
                    QuestionType::Conflict,
                    &format!("\"fact {i}\" @t[2020..2022] overlaps with \"fact x\" @t[2021..2023] - were both true simultaneously? (line:{i}) [pattern:same_entity_transition]"),
                )
            })
            .collect();
        db.sync_review_questions(&doc_id, &questions).unwrap();

        let hints = db.get_conflict_hints(None, 3).unwrap();
        assert_eq!(hints.len(), 3);
    }
}
