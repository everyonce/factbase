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
    fn test_repo_not_found() {
        let err = repo_not_found("myrepo");
        assert!(matches!(err, FactbaseError::NotFound(_)));
        let msg = err.to_string();
        assert!(msg.contains("myrepo"));
        assert!(msg.contains("factbase repo list"));
    }

    #[test]
    fn test_doc_not_found() {
        let err = doc_not_found("abc123");
        assert!(matches!(err, FactbaseError::NotFound(_)));
        let msg = err.to_string();
        assert!(msg.contains("abc123"));
        assert!(msg.contains("factbase search"));
    }

    #[test]
    fn test_error_constructors_str_and_string() {
        // Each constructor accepts both &str and String; verify variant + message
        let cases: Vec<(FactbaseError, &str)> = vec![
            (FactbaseError::parse("missing field"), "missing field"),
            (FactbaseError::parse(format!("bad value: {}", 42)), "bad value: 42"),
            (FactbaseError::not_found("no such item"), "no such item"),
            (FactbaseError::internal("something broke"), "something broke"),
            (FactbaseError::config("missing value"), "missing value"),
            (FactbaseError::embedding("no response"), "no response"),
        ];
        for (err, expected) in &cases {
            assert!(err.to_string().contains(expected), "Error '{}' should contain '{}'", err, expected);
        }
        // Verify variant matching
        assert!(matches!(FactbaseError::parse("x"), FactbaseError::Parse(_)));
        assert!(matches!(FactbaseError::not_found("x"), FactbaseError::NotFound(_)));
        assert!(matches!(FactbaseError::internal("x"), FactbaseError::Internal(_)));
        assert!(matches!(FactbaseError::config("x"), FactbaseError::Config(_)));
        assert!(matches!(FactbaseError::embedding("x"), FactbaseError::Embedding(_)));
    }

    #[test]
    fn test_format_user_error() {
        let with = format_user_error("Repository not found", Some("Run 'factbase repo list'"));
        assert!(with.contains("error:") && with.contains("Repository not found"));
        assert!(with.contains("hint:") && with.contains("factbase repo list"));

        let without = format_user_error("Something went wrong", None);
        assert!(without.contains("error:") && without.contains("Something went wrong"));
        assert!(!without.contains("hint:"));
    }

    #[test]
    fn test_format_warning() {
        let msg = format_warning("This might cause issues");
        assert!(msg.starts_with("warning:"));
        assert!(msg.contains("This might cause issues"));
    }
}
