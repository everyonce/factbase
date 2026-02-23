//! Shared error types for web API endpoints.

use axum::{http::StatusCode, Json};
use serde::Serialize;

/// Error response structure.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub code: String,
}

impl ApiError {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(msg, "NOT_FOUND")
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(msg, "BAD_REQUEST")
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(msg, "INTERNAL_ERROR")
    }
}

/// Convert FactbaseError to API response.
pub fn handle_error(e: crate::error::FactbaseError) -> (StatusCode, Json<ApiError>) {
    use crate::error::FactbaseError;
    match e {
        FactbaseError::NotFound(msg) => (StatusCode::NOT_FOUND, Json(ApiError::not_found(msg))),
        FactbaseError::Parse(msg) => (StatusCode::BAD_REQUEST, Json(ApiError::bad_request(msg))),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FactbaseError;

    #[test]
    fn test_api_error_not_found() {
        let err = ApiError::not_found("Document not found");
        assert_eq!(err.code, "NOT_FOUND");
        assert_eq!(err.error, "Document not found");
    }

    #[test]
    fn test_api_error_bad_request() {
        let err = ApiError::bad_request("Invalid input");
        assert_eq!(err.code, "BAD_REQUEST");
        assert_eq!(err.error, "Invalid input");
    }

    #[test]
    fn test_api_error_internal() {
        let err = ApiError::internal("Something went wrong");
        assert_eq!(err.code, "INTERNAL_ERROR");
        assert_eq!(err.error, "Something went wrong");
    }

    #[test]
    fn test_api_error_serialize() {
        let err = ApiError::not_found("Not found");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"error\":\"Not found\""));
        assert!(json.contains("\"code\":\"NOT_FOUND\""));
    }

    #[test]
    fn test_handle_error_not_found() {
        let err = FactbaseError::not_found("doc123");
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json.code, "NOT_FOUND");
        assert!(json.error.contains("doc123"));
    }

    #[test]
    fn test_handle_error_parse() {
        let err = FactbaseError::parse("invalid format");
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json.code, "BAD_REQUEST");
        assert!(json.error.contains("invalid format"));
    }

    #[test]
    fn test_handle_error_internal() {
        let err = FactbaseError::internal("unexpected error");
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json.code, "INTERNAL_ERROR");
    }

    #[test]
    fn test_handle_error_config() {
        let err = FactbaseError::config("invalid config");
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json.code, "INTERNAL_ERROR");
    }

    #[test]
    fn test_handle_error_embedding() {
        let err = FactbaseError::embedding("embedding failed");
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json.code, "INTERNAL_ERROR");
    }

    #[test]
    fn test_handle_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = FactbaseError::Io(io_err);
        let (status, json) = handle_error(err);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json.code, "INTERNAL_ERROR");
    }
}
