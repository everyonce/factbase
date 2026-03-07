use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewPerspective {
    /// Override global stale_days threshold for this repository
    pub stale_days: Option<u32>,
    /// Required fields per document type (e.g., person: [current_role, location])
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    /// Glob patterns for files to skip during review
    pub ignore_patterns: Option<Vec<String>>,
    /// Document types to treat as glossary/definitions (default: ["definitions"])
    pub glossary_types: Option<Vec<String>>,
}

/// Repository perspective metadata loaded from `perspective.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

/// Default perspective.yaml template created by init commands.
/// All lines are comments so it parses as empty YAML (no active fields).
pub const PERSPECTIVE_TEMPLATE: &str = "\
# Factbase perspective — tells agents what this knowledge base is about\n\
\n\
# What this knowledge base focuses on\n\
# Examples:\n\
#   focus: Mycology field research and species identification\n\
#   focus: Ancient Mediterranean civilizations and trade routes\n\
#   focus: Customer relationship intelligence for solutions architects\n\
\n\
# Allowed document types (derived from folder names)\n\
# Examples for different domains:\n\
#   allowed_types: [species, habitat, region]           # biology\n\
#   allowed_types: [civilization, event, artifact]       # history\n\
#   allowed_types: [person, company, project]            # business\n\
\n\
# Review quality settings\n\
# review:\n\
#   stale_days: 180\n\
#   required_fields:\n\
#     species: [classification, habitat, edibility]\n\
#     civilization: [period, region, key_figures]\n\
#     person: [current_role, location]\n";

/// Load and parse `perspective.yaml` from a repository root directory.
///
/// Returns `Some(Perspective)` if the file exists and contains at least one
/// non-default field. Returns `None` if the file is missing, empty, or
/// all-comments. Warns about `perspective.md` / `perspective.json` files.
pub fn load_perspective_from_file(repo_root: &Path) -> Option<Perspective> {
    // Warn about wrong file extensions
    for wrong in &["perspective.md", "perspective.json"] {
        if repo_root.join(wrong).exists() {
            warn!(
                "Found {} — factbase only reads perspective.yaml. Ignoring.",
                wrong
            );
        }
    }

    let yaml_path = repo_root.join("perspective.yaml");
    let content = match std::fs::read_to_string(&yaml_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // If file is empty or all comments, treat as unconfigured
    let has_data = content
        .lines()
        .any(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        });
    if !has_data {
        return None;
    }

    match serde_yaml_ng::from_str::<Perspective>(&content) {
        Ok(p) => {
            // Only return Some if at least one field is populated
            if p.focus.is_some()
                || p.organization.is_some()
                || p.allowed_types.is_some()
                || !p.type_name.is_empty()
                || p.review.is_some()
            {
                Some(p)
            } else {
                None
            }
        }
        Err(e) => {
            warn!(
                "Failed to parse {}: {}. Fix the YAML syntax and re-scan.",
                yaml_path.display(),
                e
            );
            None
        }
    }
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
    pub last_check_at: Option<DateTime<Utc>>,
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
            last_check_at: None,
        };
        let json = repo.to_summary_json(5);
        assert_eq!(json["id"], "test");
        assert_eq!(json["name"], "Test Repo");
        assert_eq!(json["path"], "/tmp/test");
        assert_eq!(json["document_count"], 5);
        assert!(json["last_indexed_at"].is_null());
        assert!(json.get("perspective").is_none());
    }

    #[test]
    fn test_load_perspective_valid_yaml() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Mycology research\nallowed_types:\n  - species\n  - habitat\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.focus.as_deref(), Some("Mycology research"));
        assert_eq!(p.allowed_types.as_deref(), Some(&["species".to_string(), "habitat".to_string()][..]));
    }

    #[test]
    fn test_load_perspective_missing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_empty_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.yaml"), "").unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_all_comments() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.yaml"), PERSPECTIVE_TEMPLATE).unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_malformed_yaml() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: [unclosed bracket\n  bad: {yaml\n",
        )
        .unwrap();
        // Should return None and warn, not panic
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_wrong_types_ignored() {
        let tmp = tempfile::TempDir::new().unwrap();
        // allowed_types should be a list, not a string — serde will fail
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "allowed_types: not-a-list\n",
        )
        .unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_warns_about_md_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.md"), "# Perspective").unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: test\n",
        )
        .unwrap();
        // Should still load the yaml, just warn about .md
        let p = load_perspective_from_file(tmp.path());
        assert!(p.is_some());
    }

    #[test]
    fn test_load_perspective_warns_about_json_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.json"), "{}").unwrap();
        // No yaml file — should return None
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_with_review_section() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: test\nreview:\n  stale_days: 90\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.review.as_ref().unwrap().stale_days, Some(90));
    }

    #[test]
    fn test_load_perspective_no_meaningful_fields() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Valid YAML but no recognized perspective fields
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "unrelated_key: some_value\n",
        )
        .unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_perspective_template_is_all_comments() {
        for line in PERSPECTIVE_TEMPLATE.lines() {
            let trimmed = line.trim();
            assert!(
                trimmed.is_empty() || trimmed.starts_with('#'),
                "Non-comment line in PERSPECTIVE_TEMPLATE: {trimmed}"
            );
        }
    }
}
