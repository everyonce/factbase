//! Shared test helpers for search command tests.

#[cfg(test)]
pub(crate) mod tests {
    use factbase::SearchResult;

    pub fn make_result(id: &str, title: &str, doc_type: Option<&str>, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            title: title.to_string(),
            doc_type: doc_type.map(String::from),
            file_path: format!("{id}.md"),
            relevance_score: score,
            snippet: String::new(),
            highlighted_snippet: None,
            chunk_index: None,
            chunk_start: None,
            chunk_end: None,
        }
    }
}
