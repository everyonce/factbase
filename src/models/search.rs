use serde::{Deserialize, Serialize};

/// A semantic search result with relevance scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document ID
    pub id: String,
    /// Document title
    pub title: String,
    /// Document type (e.g., "person", "project")
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    /// Path to the source file
    pub file_path: String,
    /// Cosine similarity score (0.0 to 1.0)
    pub relevance_score: f32,
    /// Text snippet from the matching content
    pub snippet: String,
    /// Snippet with search terms highlighted
    pub highlighted_snippet: Option<String>,
    /// Chunk index if result is from a chunked document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<u32>,
    /// Start byte offset of the chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_start: Option<u32>,
    /// End byte offset of the chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_end: Option<u32>,
}

impl SearchResult {
    /// Convert to JSON value for MCP tool responses.
    pub fn to_json(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// A content/grep search result with line-level matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSearchResult {
    /// Document ID
    pub id: String,
    /// Document title
    pub title: String,
    /// Document type
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    /// Path to the source file
    pub file_path: String,
    /// Repository ID
    pub repo_id: String,
    /// Individual line matches within the document
    pub matches: Vec<ContentMatch>,
}

impl ContentSearchResult {
    /// Convert to JSON value for MCP tool responses.
    pub fn to_json(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// A single line match within a content search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMatch {
    /// 1-based line number of the match
    pub line_number: usize,
    /// The matching line content
    pub line: String,
    /// Surrounding context lines
    pub context: String,
}

/// Paginated wrapper for semantic search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedSearchResult {
    /// The search results for this page
    pub results: Vec<SearchResult>,
    /// Total number of matching results
    pub total_count: usize,
    /// Offset into the full result set
    pub offset: usize,
    /// Maximum results per page
    pub limit: usize,
    /// Number of results removed by deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated_count: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_search_result_to_json() {
        let result = ContentSearchResult {
            id: "abc123".to_string(),
            title: "Test Doc".to_string(),
            doc_type: Some("person".to_string()),
            file_path: "people/test.md".to_string(),
            repo_id: "notes".to_string(),
            matches: vec![ContentMatch {
                line_number: 5,
                line: "hello world".to_string(),
                context: String::new(),
            }],
        };
        let json = result.to_json();
        assert_eq!(json["id"], "abc123");
        assert_eq!(json["title"], "Test Doc");
        assert_eq!(json["type"], "person");
        assert_eq!(json["repo_id"], "notes");
        assert_eq!(json["matches"][0]["line_number"], 5);
    }

    #[test]
    fn test_search_result_to_json() {
        let result = SearchResult {
            id: "abc123".to_string(),
            title: "Test Doc".to_string(),
            doc_type: Some("person".to_string()),
            file_path: "people/test.md".to_string(),
            relevance_score: 0.95,
            snippet: "some text".to_string(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        };
        let json = result.to_json();
        assert_eq!(json["id"], "abc123");
        assert_eq!(json["title"], "Test Doc");
        assert_eq!(json["type"], "person");
        assert_eq!(json["relevance_score"].as_f64().unwrap().round(), 1.0_f64);
    }
}
