//! Helper functions for MCP tool argument extraction.
//!
//! These functions provide consistent argument parsing across all MCP tools.

use crate::error::FactbaseError;
use serde_json::Value;

// Re-export shared run_blocking for MCP tool modules
pub(crate) use crate::async_helpers::run_blocking;

/// Extract optional string argument from JSON value.
pub(crate) fn get_str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Extract required string argument, returning error if missing.
pub(crate) fn get_str_arg_required(args: &Value, key: &str) -> Result<String, FactbaseError> {
    get_str_arg(args, key)
        .map(String::from)
        .ok_or_else(|| FactbaseError::parse(format!("Missing {} parameter", key)))
}

/// Extract optional u64 argument with default value.
pub(crate) fn get_u64_arg(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Extract required u64 argument, returning error if missing.
pub(crate) fn get_u64_arg_required(args: &Value, key: &str) -> Result<u64, FactbaseError> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| FactbaseError::parse(format!("Missing {} parameter", key)))
}

/// Extract optional `doc_type` and `repo` filter arguments.
///
/// Used by all MCP search tools to consistently extract type/repo filters.
pub(crate) fn extract_type_repo_filters(args: &Value) -> (Option<String>, Option<String>) {
    (
        get_str_arg(args, "doc_type").map(String::from),
        get_str_arg(args, "repo").map(String::from),
    )
}

/// Extract optional bool argument with default value.
pub(crate) fn get_bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_str_arg_present() {
        let args = serde_json::json!({"name": "test"});
        assert_eq!(get_str_arg(&args, "name"), Some("test"));
    }

    #[test]
    fn test_get_str_arg_missing() {
        let args = serde_json::json!({});
        assert_eq!(get_str_arg(&args, "name"), None);
    }

    #[test]
    fn test_get_str_arg_wrong_type() {
        let args = serde_json::json!({"name": 123});
        assert_eq!(get_str_arg(&args, "name"), None);
    }

    #[test]
    fn test_get_str_arg_required_present() {
        let args = serde_json::json!({"id": "abc123"});
        let result = get_str_arg_required(&args, "id");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "abc123");
    }

    #[test]
    fn test_get_str_arg_required_missing() {
        let args = serde_json::json!({});
        let result = get_str_arg_required(&args, "id");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing id"));
    }

    #[test]
    fn test_get_u64_arg_present() {
        let args = serde_json::json!({"limit": 20});
        assert_eq!(get_u64_arg(&args, "limit", 10), 20);
    }

    #[test]
    fn test_get_u64_arg_missing_uses_default() {
        let args = serde_json::json!({});
        assert_eq!(get_u64_arg(&args, "limit", 10), 10);
    }

    #[test]
    fn test_get_u64_arg_wrong_type_uses_default() {
        let args = serde_json::json!({"limit": "twenty"});
        assert_eq!(get_u64_arg(&args, "limit", 10), 10);
    }

    #[test]
    fn test_get_u64_arg_required_present() {
        let args = serde_json::json!({"count": 5});
        let result = get_u64_arg_required(&args, "count");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[test]
    fn test_get_u64_arg_required_missing() {
        let args = serde_json::json!({});
        let result = get_u64_arg_required(&args, "count");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing count"));
    }

    #[test]
    fn test_extract_type_repo_filters_both() {
        let args = serde_json::json!({"doc_type": "person", "repo": "notes"});
        let (doc_type, repo) = extract_type_repo_filters(&args);
        assert_eq!(doc_type.as_deref(), Some("person"));
        assert_eq!(repo.as_deref(), Some("notes"));
    }

    #[test]
    fn test_extract_type_repo_filters_none() {
        let args = serde_json::json!({});
        let (doc_type, repo) = extract_type_repo_filters(&args);
        assert!(doc_type.is_none());
        assert!(repo.is_none());
    }

    #[test]
    fn test_get_bool_arg_present_true() {
        let args = serde_json::json!({"flag": true});
        assert!(get_bool_arg(&args, "flag", false));
    }

    #[test]
    fn test_get_bool_arg_present_false() {
        let args = serde_json::json!({"flag": false});
        assert!(!get_bool_arg(&args, "flag", true));
    }

    #[test]
    fn test_get_bool_arg_missing_uses_default() {
        let args = serde_json::json!({});
        assert!(get_bool_arg(&args, "flag", true));
        assert!(!get_bool_arg(&args, "flag", false));
    }

    #[test]
    fn test_get_bool_arg_wrong_type_uses_default() {
        let args = serde_json::json!({"flag": "yes"});
        assert!(get_bool_arg(&args, "flag", true));
    }
}
