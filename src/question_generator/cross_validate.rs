//! Cross-document fact validation utilities.
//!
//! Provides helper functions for fact-pair identification. LLM-based
//! classification has been removed — the client agent now classifies
//! fact pairs directly via the `get_fact_pairs` MCP tool.

/// Build a deterministic pair ID: lower ID first.
pub fn make_pair_id(fact_a_id: &str, fact_b_id: &str) -> String {
    if fact_a_id <= fact_b_id {
        format!("{fact_a_id}:{fact_b_id}")
    } else {
        format!("{fact_b_id}:{fact_a_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_pair_id_deterministic_ordering() {
        assert_eq!(make_pair_id("aaa_3", "bbb_5"), "aaa_3:bbb_5");
        assert_eq!(make_pair_id("bbb_5", "aaa_3"), "aaa_3:bbb_5");
        assert_eq!(make_pair_id("x_1", "x_1"), "x_1:x_1");
    }
}
