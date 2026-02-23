//! Shared test helpers for organize module tests.

#[cfg(test)]
pub(crate) mod tests {
    use crate::database::Database;
    use crate::models::Document;
    use chrono::Utc;

    /// Create a Document and insert it into the database.
    /// Used by snapshot, merge, split, and links tests.
    pub fn insert_test_doc(
        db: &Database,
        id: &str,
        repo_id: &str,
        title: &str,
        content: &str,
        path: &str,
    ) {
        let doc = Document {
            id: id.to_string(),
            repo_id: repo_id.to_string(),
            file_path: path.to_string(),
            file_hash: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            file_modified_at: Some(Utc::now()),
            ..Document::test_default()
        };
        db.upsert_document(&doc).expect("create doc");
    }

    /// Create a Document without inserting into DB.
    /// Used by move and retype tests.
    pub fn make_test_doc(
        id: &str,
        title: &str,
        file_path: &str,
        doc_type: Option<&str>,
    ) -> Document {
        Document {
            id: id.to_string(),
            repo_id: "test".to_string(),
            file_path: file_path.to_string(),
            title: title.to_string(),
            doc_type: doc_type.map(|s| s.to_string()),
            content: format!("<!-- factbase:{} -->\n# {}\n\nContent here.", id, title),
            file_modified_at: Some(Utc::now()),
            ..Document::test_default()
        }
    }
}
