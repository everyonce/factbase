use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Type of review question indicating what kind of issue was detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuestionType {
    /// Missing temporal information
    Temporal,
    /// Contradictory facts detected
    Conflict,
    /// Missing data (source, required field)
    Missing,
    /// Unclear or ambiguous meaning
    Ambiguous,
    /// Potentially outdated information
    Stale,
    /// Possible duplicate entity
    Duplicate,
}

impl QuestionType {
    /// Returns the string representation of this question type.
    pub fn as_str(&self) -> &'static str {
        match self {
            QuestionType::Temporal => "temporal",
            QuestionType::Conflict => "conflict",
            QuestionType::Missing => "missing",
            QuestionType::Ambiguous => "ambiguous",
            QuestionType::Stale => "stale",
            QuestionType::Duplicate => "duplicate",
        }
    }
}

impl std::fmt::Display for QuestionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for QuestionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "temporal" => Ok(QuestionType::Temporal),
            "conflict" => Ok(QuestionType::Conflict),
            "missing" => Ok(QuestionType::Missing),
            "ambiguous" => Ok(QuestionType::Ambiguous),
            "stale" => Ok(QuestionType::Stale),
            "duplicate" => Ok(QuestionType::Duplicate),
            _ => Err(format!("Unknown question type: {s}")),
        }
    }
}

/// A review question in the Review Queue section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewQuestion {
    /// Type of question/issue detected
    pub question_type: QuestionType,
    /// Line number referenced in question (1-indexed, if applicable)
    pub line_ref: Option<usize>,
    /// Question description text
    pub description: String,
    /// Whether the checkbox is checked (question answered)
    pub answered: bool,
    /// Answer text from blockquote (if answered)
    pub answer: Option<String>,
    /// Line number where question appears in document (1-indexed)
    pub line_number: usize,
}

impl ReviewQuestion {
    /// Create a new unanswered review question with default fields.
    pub fn new(question_type: QuestionType, line_ref: Option<usize>, description: String) -> Self {
        Self {
            question_type,
            line_ref,
            description,
            answered: false,
            answer: None,
            line_number: 0,
        }
    }

    /// Returns a JSON representation of the base question fields.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "type": self.question_type.as_str(),
            "line_ref": self.line_ref,
            "description": self.description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_type_from_str() {
        assert_eq!(
            "temporal".parse::<QuestionType>().unwrap(),
            QuestionType::Temporal
        );
        assert_eq!(
            "TEMPORAL".parse::<QuestionType>().unwrap(),
            QuestionType::Temporal
        );
        assert_eq!(
            "Temporal".parse::<QuestionType>().unwrap(),
            QuestionType::Temporal
        );
        assert_eq!(
            "conflict".parse::<QuestionType>().unwrap(),
            QuestionType::Conflict
        );
        assert_eq!(
            "missing".parse::<QuestionType>().unwrap(),
            QuestionType::Missing
        );
        assert_eq!(
            "ambiguous".parse::<QuestionType>().unwrap(),
            QuestionType::Ambiguous
        );
        assert_eq!(
            "stale".parse::<QuestionType>().unwrap(),
            QuestionType::Stale
        );
        assert_eq!(
            "duplicate".parse::<QuestionType>().unwrap(),
            QuestionType::Duplicate
        );
    }

    #[test]
    fn test_question_type_from_str_unknown() {
        assert!("unknown".parse::<QuestionType>().is_err());
        assert!("invalid".parse::<QuestionType>().is_err());
        assert!("".parse::<QuestionType>().is_err());
    }

    #[test]
    fn test_question_type_as_str() {
        assert_eq!(QuestionType::Temporal.as_str(), "temporal");
        assert_eq!(QuestionType::Conflict.as_str(), "conflict");
        assert_eq!(QuestionType::Missing.as_str(), "missing");
        assert_eq!(QuestionType::Ambiguous.as_str(), "ambiguous");
        assert_eq!(QuestionType::Stale.as_str(), "stale");
        assert_eq!(QuestionType::Duplicate.as_str(), "duplicate");
    }

    #[test]
    fn test_question_type_display() {
        assert_eq!(QuestionType::Temporal.to_string(), "temporal");
        assert_eq!(QuestionType::Conflict.to_string(), "conflict");
        assert_eq!(QuestionType::Missing.to_string(), "missing");
        assert_eq!(QuestionType::Ambiguous.to_string(), "ambiguous");
        assert_eq!(QuestionType::Stale.to_string(), "stale");
        assert_eq!(QuestionType::Duplicate.to_string(), "duplicate");
    }

    #[test]
    fn test_question_type_roundtrip() {
        for qt in [
            QuestionType::Temporal,
            QuestionType::Conflict,
            QuestionType::Missing,
            QuestionType::Ambiguous,
            QuestionType::Stale,
            QuestionType::Duplicate,
        ] {
            let s = qt.to_string();
            let parsed: QuestionType = s.parse().unwrap();
            assert_eq!(qt, parsed);
        }
    }

    #[test]
    fn test_review_question_new_defaults() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "test".to_string());
        assert_eq!(q.question_type, QuestionType::Temporal);
        assert_eq!(q.line_ref, Some(5));
        assert_eq!(q.description, "test");
        assert!(!q.answered);
        assert!(q.answer.is_none());
        assert_eq!(q.line_number, 0);
    }

    #[test]
    fn test_review_question_to_json() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "When?".to_string());
        let json = q.to_json();
        assert_eq!(json["type"], "temporal");
        assert_eq!(json["line_ref"], 5);
        assert_eq!(json["description"], "When?");
        // Base JSON should not include answered/answer/doc fields
        assert!(json.get("answered").is_none());
        assert!(json.get("answer").is_none());
        assert!(json.get("doc_id").is_none());
    }

    #[test]
    fn test_review_question_to_json_null_line_ref() {
        let q = ReviewQuestion::new(QuestionType::Missing, None, "Source?".to_string());
        let json = q.to_json();
        assert_eq!(json["type"], "missing");
        assert!(json["line_ref"].is_null());
    }
}
