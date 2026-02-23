//! Error types and formatting for user-facing messages.

use thiserror::Error;

/// Prefix constants for consistent error output
pub mod prefix {
    pub const ERROR: &str = "error:";
    pub const WARNING: &str = "warning:";
    pub const HINT: &str = "hint:";
}

#[derive(Error, Debug)]
pub enum FactbaseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Watcher error: {0}")]
    Watcher(#[from] notify::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Ollama error: {0}")]
    Ollama(String),
}

impl FactbaseError {
    /// Shorthand for `FactbaseError::Parse(msg.into())`
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    /// Shorthand for `FactbaseError::NotFound(msg.into())`
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Shorthand for `FactbaseError::Internal(msg.into())`
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Shorthand for `FactbaseError::Config(msg.into())`
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Shorthand for `FactbaseError::Embedding(msg.into())`
    pub fn embedding(msg: impl Into<String>) -> Self {
        Self::Embedding(msg.into())
    }

    /// Shorthand for `FactbaseError::Llm(msg.into())`
    pub fn llm(msg: impl Into<String>) -> Self {
        Self::Llm(msg.into())
    }

    /// Shorthand for `FactbaseError::Ollama(msg.into())`
    pub fn ollama(msg: impl Into<String>) -> Self {
        Self::Ollama(msg.into())
    }
}

impl From<serde_yaml_ng::Error> for FactbaseError {
    fn from(e: serde_yaml_ng::Error) -> Self {
        FactbaseError::config(e.to_string())
    }
}

impl From<rusqlite::Error> for FactbaseError {
    fn from(e: rusqlite::Error) -> Self {
        let msg = e.to_string();
        if msg.contains("no such column") || msg.contains("no such table") {
            FactbaseError::Database(format!(
                "{msg}\nhint: Database schema is out of date. Update factbase (npm i -g @everyonce/factbase) \
                 or delete the database and re-scan."
            ))
        } else {
            FactbaseError::Database(msg)
        }
    }
}

impl From<serde_json::Error> for FactbaseError {
    fn from(e: serde_json::Error) -> Self {
        FactbaseError::parse(e.to_string())
    }
}

/// Create a "Repository not found" error with helpful suggestion (for MCP tools)
pub fn repo_not_found(repo_id: &str) -> FactbaseError {
    FactbaseError::not_found(format!(
        "Repository not found: {repo_id}\nRun 'factbase repo list' to see available repositories"
    ))
}

/// Create a "Document not found" error with helpful suggestion (for MCP tools)
pub fn doc_not_found(doc_id: &str) -> FactbaseError {
    FactbaseError::not_found(format!(
        "Document not found: {doc_id}\nRun 'factbase search' to find documents"
    ))
}

/// Format a user-friendly error message with optional suggestion.
pub fn format_user_error(msg: &str, suggestion: Option<&str>) -> String {
    match suggestion {
        Some(hint) => format!("{} {}\n{} {}", prefix::ERROR, msg, prefix::HINT, hint),
        None => format!("{} {}", prefix::ERROR, msg),
    }
}

/// Format a warning message.
pub fn format_warning(msg: &str) -> String {
    format!("{} {}", prefix::WARNING, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_not_found_contains_repo_id() {
        let err = repo_not_found("myrepo");
        let msg = err.to_string();
        assert!(msg.contains("myrepo"));
    }

    #[test]
    fn test_repo_not_found_contains_suggestion() {
        let err = repo_not_found("test-repo");
        let msg = err.to_string();
        assert!(msg.contains("factbase repo list"));
    }

    #[test]
    fn test_repo_not_found_is_not_found_variant() {
        let err = repo_not_found("myrepo");
        assert!(matches!(err, FactbaseError::NotFound(_)));
    }

    #[test]
    fn test_doc_not_found_contains_doc_id() {
        let err = doc_not_found("abc123");
        let msg = err.to_string();
        assert!(msg.contains("abc123"));
    }

    #[test]
    fn test_doc_not_found_contains_suggestion() {
        let err = doc_not_found("test-doc");
        let msg = err.to_string();
        assert!(msg.contains("factbase search"));
    }

    #[test]
    fn test_doc_not_found_is_not_found_variant() {
        let err = doc_not_found("abc123");
        assert!(matches!(err, FactbaseError::NotFound(_)));
    }

    #[test]
    fn test_parse_constructor_with_string() {
        let err = FactbaseError::parse(format!("bad value: {}", 42));
        assert!(matches!(err, FactbaseError::Parse(_)));
        assert!(err.to_string().contains("bad value: 42"));
    }

    #[test]
    fn test_parse_constructor_with_str() {
        let err = FactbaseError::parse("missing field");
        assert!(matches!(err, FactbaseError::Parse(_)));
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn test_not_found_constructor_with_string() {
        let err = FactbaseError::not_found(format!("doc {}", "abc"));
        assert!(matches!(err, FactbaseError::NotFound(_)));
        assert!(err.to_string().contains("doc abc"));
    }

    #[test]
    fn test_not_found_constructor_with_str() {
        let err = FactbaseError::not_found("no such item");
        assert!(matches!(err, FactbaseError::NotFound(_)));
        assert!(err.to_string().contains("no such item"));
    }

    #[test]
    fn test_internal_constructor_with_string() {
        let err = FactbaseError::internal(format!("oops: {}", 42));
        assert!(matches!(err, FactbaseError::Internal(_)));
        assert!(err.to_string().contains("oops: 42"));
    }

    #[test]
    fn test_internal_constructor_with_str() {
        let err = FactbaseError::internal("something broke");
        assert!(matches!(err, FactbaseError::Internal(_)));
        assert!(err.to_string().contains("something broke"));
    }

    #[test]
    fn test_config_constructor_with_string() {
        let err = FactbaseError::config(format!("bad field: {}", "x"));
        assert!(matches!(err, FactbaseError::Config(_)));
        assert!(err.to_string().contains("bad field: x"));
    }

    #[test]
    fn test_config_constructor_with_str() {
        let err = FactbaseError::config("missing value");
        assert!(matches!(err, FactbaseError::Config(_)));
        assert!(err.to_string().contains("missing value"));
    }

    #[test]
    fn test_embedding_constructor_with_string() {
        let err = FactbaseError::embedding(format!("dim mismatch: {}", 512));
        assert!(matches!(err, FactbaseError::Embedding(_)));
        assert!(err.to_string().contains("dim mismatch: 512"));
    }

    #[test]
    fn test_embedding_constructor_with_str() {
        let err = FactbaseError::embedding("no response");
        assert!(matches!(err, FactbaseError::Embedding(_)));
        assert!(err.to_string().contains("no response"));
    }

    #[test]
    fn test_llm_constructor_with_string() {
        let err = FactbaseError::llm(format!("timeout: {}s", 30));
        assert!(matches!(err, FactbaseError::Llm(_)));
        assert!(err.to_string().contains("timeout: 30s"));
    }

    #[test]
    fn test_llm_constructor_with_str() {
        let err = FactbaseError::llm("no output");
        assert!(matches!(err, FactbaseError::Llm(_)));
        assert!(err.to_string().contains("no output"));
    }

    #[test]
    fn test_ollama_constructor_with_string() {
        let err = FactbaseError::ollama(format!("connection refused: {}", "localhost"));
        assert!(matches!(err, FactbaseError::Ollama(_)));
        assert!(err.to_string().contains("connection refused: localhost"));
    }

    #[test]
    fn test_ollama_constructor_with_str() {
        let err = FactbaseError::ollama("empty response");
        assert!(matches!(err, FactbaseError::Ollama(_)));
        assert!(err.to_string().contains("empty response"));
    }

    #[test]
    fn test_format_user_error_with_suggestion() {
        let msg = format_user_error("Repository not found", Some("Run 'factbase repo list'"));
        assert!(msg.contains("error:"));
        assert!(msg.contains("Repository not found"));
        assert!(msg.contains("hint:"));
        assert!(msg.contains("factbase repo list"));
    }

    #[test]
    fn test_format_user_error_without_suggestion() {
        let msg = format_user_error("Something went wrong", None);
        assert!(msg.contains("error:"));
        assert!(msg.contains("Something went wrong"));
        assert!(!msg.contains("hint:"));
    }

    #[test]
    fn test_format_warning() {
        let msg = format_warning("This might cause issues");
        assert!(msg.starts_with("warning:"));
        assert!(msg.contains("This might cause issues"));
    }
}
