use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewPerspective {
    /// Override global stale_days threshold for this repository
    pub stale_days: Option<u32>,
    /// Required fields per document type (e.g., person: [current_role, location])
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    /// Glob patterns for files to skip during review
    pub ignore_patterns: Option<Vec<String>>,
}

/// Repository perspective metadata loaded from `perspective.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Perspective {
    /// Knowledge base type (e.g., "customer-intelligence", "personal")
    #[serde(rename = "type", default)]
    pub type_name: String,
    /// Organization name for context
    pub organization: Option<String>,
    /// Focus area description
    pub focus: Option<String>,
    /// Allowed document types for this repository
    pub allowed_types: Option<Vec<String>>,
    /// Review-specific perspective overrides
    #[serde(default)]
    pub review: Option<ReviewPerspective>,
}

/// A registered knowledge base repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    /// Unique repository identifier
    pub id: String,
    /// Human-readable repository name
    pub name: String,
    /// Filesystem path to the repository root
    pub path: PathBuf,
    /// Optional perspective metadata
    pub perspective: Option<Perspective>,
    /// When this repository was registered
    pub created_at: DateTime<Utc>,
    /// When documents were last indexed
    pub last_indexed_at: Option<DateTime<Utc>>,
    /// When lint was last run
    pub last_lint_at: Option<DateTime<Utc>>,
}

impl Repository {
    /// Returns a JSON summary with `id`, `name`, `path`, `document_count`, `last_indexed_at`.
    pub fn to_summary_json(&self, doc_count: usize) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "path": self.path.to_string_lossy(),
            "document_count": doc_count,
            "last_indexed_at": self.last_indexed_at.map(|t| t.to_rfc3339())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_summary_json() {
        let repo = Repository {
            id: "test".into(),
            name: "Test Repo".into(),
            path: PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        let json = repo.to_summary_json(5);
        assert_eq!(json["id"], "test");
        assert_eq!(json["name"], "Test Repo");
        assert_eq!(json["path"], "/tmp/test");
        assert_eq!(json["document_count"], 5);
        assert!(json["last_indexed_at"].is_null());
        assert!(json.get("perspective").is_none());
    }
}
