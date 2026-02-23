use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub repo_id: String,
    pub file_path: String,
    pub file_hash: String,
    pub title: String,
    pub doc_type: Option<String>,
    pub content: String,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub indexed_at: DateTime<Utc>,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub perspective: Option<Perspective>,
    pub created_at: DateTime<Utc>,
    pub last_indexed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub links_detected: usize,
}

impl std::fmt::Display for ScanResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} added, {} updated, {} deleted, {} unchanged",
            self.added, self.updated, self.deleted, self.unchanged
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perspective {
    #[serde(rename = "type")]
    pub type_name: String,
    pub organization: Option<String>,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RepoStats {
    pub total: usize,
    pub active: usize,
    pub deleted: usize,
    pub by_type: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub doc_type: Option<String>,
    pub file_path: String,
    pub relevance_score: f32,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub source_id: String,
    pub target_id: String,
    pub context: Option<String>,
    pub created_at: DateTime<Utc>,
}
