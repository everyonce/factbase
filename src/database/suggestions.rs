use chrono::Utc;

use crate::error::FactbaseError;
use crate::Database;

/// A pending organization suggestion stored in the database.
#[derive(Debug, Clone, PartialEq)]
pub struct OrganizationSuggestion {
    pub id: i64,
    pub doc_id: String,
    pub suggestion_type: String,
    pub suggested_value: String,
    pub source: String,
    pub reason: Option<String>,
    pub created_at: String,
}

impl Database {
    /// Insert a new organization suggestion.
    pub fn insert_suggestion(
        &self,
        doc_id: &str,
        suggestion_type: &str,
        suggested_value: &str,
        source: &str,
    ) -> Result<i64, FactbaseError> {
        self.insert_suggestion_with_reason(doc_id, suggestion_type, suggested_value, source, None)
    }

    /// Insert a new organization suggestion with an optional reason.
    pub fn insert_suggestion_with_reason(
        &self,
        doc_id: &str,
        suggestion_type: &str,
        suggested_value: &str,
        source: &str,
        reason: Option<&str>,
    ) -> Result<i64, FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO organization_suggestions (doc_id, suggestion_type, suggested_value, source, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![doc_id, suggestion_type, suggested_value, source, reason, Utc::now().to_rfc3339()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// List all pending suggestions, optionally filtered by repo.
    pub fn list_suggestions(
        &self,
        repo_id: Option<&str>,
    ) -> Result<Vec<OrganizationSuggestion>, FactbaseError> {
        let conn = self.get_conn()?;
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(rid) = repo_id {
                (
                "SELECT s.id, s.doc_id, s.suggestion_type, s.suggested_value, s.source, s.reason, s.created_at
                 FROM organization_suggestions s
                 JOIN documents d ON d.id = s.doc_id
                 WHERE d.repo_id = ?1
                 ORDER BY s.created_at".to_string(),
                vec![Box::new(rid.to_string())],
            )
            } else {
                (
                    "SELECT id, doc_id, suggestion_type, suggested_value, source, reason, created_at
                 FROM organization_suggestions
                 ORDER BY created_at"
                        .to_string(),
                    vec![],
                )
            };
        let mut stmt = conn.prepare_cached(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut rows = stmt.query(params_ref.as_slice())?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            results.push(OrganizationSuggestion {
                id: row.get(0)?,
                doc_id: row.get(1)?,
                suggestion_type: row.get(2)?,
                suggested_value: row.get(3)?,
                source: row.get(4)?,
                reason: row.get(5)?,
                created_at: row.get(6)?,
            });
        }
        Ok(results)
    }

    /// Delete a suggestion by ID.
    pub fn delete_suggestion(&self, id: i64) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute("DELETE FROM organization_suggestions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Delete all suggestions for a document.
    pub fn delete_suggestions_for_doc(&self, doc_id: &str) -> Result<usize, FactbaseError> {
        let conn = self.get_conn()?;
        let count = conn.execute(
            "DELETE FROM organization_suggestions WHERE doc_id = ?1",
            [doc_id],
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo, test_repo_with_id};

    #[test]
    fn test_insert_and_list_suggestions() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("d1", "Test Doc");
        doc.repo_id = repo.id.clone();
        db.upsert_document(&doc).unwrap();

        let id = db
            .insert_suggestion("d1", "move", "new/path/", "update")
            .unwrap();
        assert!(id > 0);

        let suggestions = db.list_suggestions(None).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].doc_id, "d1");
        assert_eq!(suggestions[0].suggestion_type, "move");
        assert_eq!(suggestions[0].suggested_value, "new/path/");
        assert_eq!(suggestions[0].source, "update");
    }

    #[test]
    fn test_list_suggestions_by_repo() {
        let (db, _tmp) = test_db();
        let repo1 = test_repo_with_id("r1");
        let repo2 = test_repo_with_id("r2");
        db.upsert_repository(&repo1).unwrap();
        db.upsert_repository(&repo2).unwrap();

        let mut doc1 = test_doc("d1", "Doc 1");
        doc1.repo_id = "r1".to_string();
        let mut doc2 = test_doc("d2", "Doc 2");
        doc2.repo_id = "r2".to_string();
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        db.insert_suggestion("d1", "move", "a/", "update").unwrap();
        db.insert_suggestion("d2", "rename", "new.md", "update")
            .unwrap();

        let r1_suggestions = db.list_suggestions(Some("r1")).unwrap();
        assert_eq!(r1_suggestions.len(), 1);
        assert_eq!(r1_suggestions[0].doc_id, "d1");

        let all = db.list_suggestions(None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_delete_suggestion() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("d1", "Test");
        doc.repo_id = repo.id.clone();
        db.upsert_document(&doc).unwrap();

        let id = db.insert_suggestion("d1", "move", "x/", "update").unwrap();
        db.delete_suggestion(id).unwrap();

        let suggestions = db.list_suggestions(None).unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_delete_suggestions_for_doc() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("d1", "Test");
        doc.repo_id = repo.id.clone();
        db.upsert_document(&doc).unwrap();

        db.insert_suggestion("d1", "move", "a/", "update").unwrap();
        db.insert_suggestion("d1", "rename", "b.md", "update")
            .unwrap();

        let count = db.delete_suggestions_for_doc("d1").unwrap();
        assert_eq!(count, 2);
        assert!(db.list_suggestions(None).unwrap().is_empty());
    }

    #[test]
    fn test_insert_suggestion_with_reason() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("d1", "Test Doc");
        doc.repo_id = repo.id.clone();
        db.upsert_document(&doc).unwrap();

        db.insert_suggestion_with_reason(
            "d1",
            "move",
            "new/path/",
            "update",
            Some("12 of 14 links point to new-testament documents"),
        )
        .unwrap();

        let suggestions = db.list_suggestions(None).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(
            suggestions[0].reason.as_deref(),
            Some("12 of 14 links point to new-testament documents")
        );
    }

    #[test]
    fn test_insert_suggestion_reason_defaults_to_none() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("d1", "Test Doc");
        doc.repo_id = repo.id.clone();
        db.upsert_document(&doc).unwrap();

        db.insert_suggestion("d1", "move", "new/path/", "update")
            .unwrap();

        let suggestions = db.list_suggestions(None).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].reason.is_none());
    }
}
