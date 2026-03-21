use serde::{Deserialize, Serialize};

/// Type of temporal tag indicating how to interpret the date(s)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalTagType {
    /// `@t[=DATE]` - Fact known true at specific point in time
    PointInTime,
    /// `@t[~DATE]` - Last seen/verified date
    LastSeen,
    /// `@t[DATE..DATE]` - Fact true during date range
    Range,
    /// `@t[DATE..]` - Started at date, believed ongoing
    Ongoing,
    /// `@t[..DATE]` - Unknown start, ended at date
    Historical,
    /// `@t[?]` - Temporal context unknown/unverified
    Unknown,
}

/// A parsed temporal tag from document content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalTag {
    /// Type of temporal tag
    pub tag_type: TemporalTagType,
    /// Start date (YYYY, YYYY-QN, YYYY-MM, or YYYY-MM-DD)
    pub start_date: Option<String>,
    /// End date (YYYY, YYYY-QN, YYYY-MM, or YYYY-MM-DD)
    pub end_date: Option<String>,
    /// Line number where tag appears (1-indexed)
    pub line_number: usize,
    /// Raw tag text as it appears in document (e.g., "@t[2020..2022]")
    pub raw_text: String,
}

/// An inline source reference `[^N]` in document content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    /// Footnote number (e.g., 1 for `[^1]`)
    pub number: u32,
    /// Line number where reference appears (1-indexed)
    pub line_number: usize,
}

/// A footnote definition `[^N]: ...` at the end of a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDefinition {
    /// Footnote number (e.g., 1 for `[^1]:`)
    pub number: u32,
    /// Source type (e.g., "LinkedIn profile", "Press release")
    pub source_type: String,
    /// Additional context from the definition
    pub context: String,
    /// Date extracted from definition (e.g., "2024-01-15" from "scraped 2024-01-15")
    pub date: Option<String>,
    /// Line number where definition appears (1-indexed)
    pub line_number: usize,
    /// Explicit source type tag from `{type:x}` at end of definition line.
    /// Matches keys in `perspective.yaml source_types`.
    pub type_tag: Option<String>,
}

/// Statistics about facts and temporal tag coverage in a document
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactStats {
    /// Total number of facts (list items) in the document
    pub total_facts: usize,
    /// Number of facts with at least one temporal tag
    pub facts_with_tags: usize,
    /// Coverage percentage (0.0 to 1.0)
    pub coverage: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_stats_default() {
        let stats = FactStats::default();
        assert_eq!(stats.total_facts, 0);
        assert_eq!(stats.facts_with_tags, 0);
        assert!((stats.coverage - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_temporal_tag_type_variants() {
        // Ensure all 6 variants exist and are distinct
        let variants = [
            TemporalTagType::PointInTime,
            TemporalTagType::LastSeen,
            TemporalTagType::Range,
            TemporalTagType::Ongoing,
            TemporalTagType::Historical,
            TemporalTagType::Unknown,
        ];
        assert_eq!(variants.len(), 6);
        // Verify each variant is distinct
        for (i, v1) in variants.iter().enumerate() {
            for (j, v2) in variants.iter().enumerate() {
                if i != j {
                    assert_ne!(v1, v2);
                }
            }
        }
    }
}
