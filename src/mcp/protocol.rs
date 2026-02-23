use serde_json::Value;

/// Returns the MCP initialize response payload.
///
/// Used by both stdio and HTTP transports to respond to `initialize` requests.
pub fn initialize_result() -> Value {
    serde_json::json!({
        "protocolVersion": "2025-03-26",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "factbase",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_result_structure() {
        let result = initialize_result();
        assert_eq!(result["protocolVersion"], "2025-03-26");
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], "factbase");
        assert!(!result["serverInfo"]["version"].as_str().unwrap().is_empty());
    }
}
