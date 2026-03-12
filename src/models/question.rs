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
    /// Document corruption detected (garbage footnotes, corrupted titles, etc.)
    Corruption,
    /// Imprecise language that could change truth value (weasel words, vague qualifiers)
    Precision,
    /// Citation too vague to independently verify
    WeakSource,
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
            QuestionType::Corruption => "corruption",
            QuestionType::Precision => "precision",
            QuestionType::WeakSource => "weak-source",
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
            "corruption" => Ok(QuestionType::Corruption),
            "precision" => Ok(QuestionType::Precision),
            "weak-source" => Ok(QuestionType::WeakSource),
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
    /// Confidence level for this question (e.g., "high", "low").
    /// When None, treated as normal confidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// Reason for the confidence level (e.g., "fact in definition document").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_reason: Option<String>,
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
            confidence: None,
            confidence_reason: None,
        }
    }

    /// Create a new review question with confidence metadata.
    pub fn with_confidence(mut self, confidence: &str, reason: &str) -> Self {
        self.confidence = Some(confidence.to_string());
        self.confidence_reason = Some(reason.to_string());
        self
    }

    /// Returns true if this question was deferred (unchecked but has an answer/note).
    pub fn is_deferred(&self) -> bool {
        !self.answered && self.answer.is_some()
    }

    /// Returns true if this question has a believed answer (confidence=believed).
    /// Believed answers are stored as deferred with a "believed: " prefix.
    pub fn is_believed(&self) -> bool {
        self.is_deferred()
            && self
                .answer
                .as_deref()
                .is_some_and(|a| a.starts_with("believed: "))
    }

    /// Returns a JSON representation of the base question fields.
    pub fn to_json(&self) -> Value {
        let mut json = serde_json::json!({
            "type": self.question_type.as_str(),
            "line_ref": self.line_ref,
            "description": self.description,
        });
        if let Some(ref c) = self.confidence {
            json["confidence"] = Value::String(c.clone());
        }
        if let Some(ref r) = self.confidence_reason {
            json["confidence_reason"] = Value::String(r.clone());
        }
        json
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
        assert_eq!(
            "corruption".parse::<QuestionType>().unwrap(),
            QuestionType::Corruption
        );
        assert_eq!(
            "precision".parse::<QuestionType>().unwrap(),
            QuestionType::Precision
        );
        assert_eq!(
            "weak-source".parse::<QuestionType>().unwrap(),
            QuestionType::WeakSource
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
        assert_eq!(QuestionType::Corruption.as_str(), "corruption");
        assert_eq!(QuestionType::Precision.as_str(), "precision");
        assert_eq!(QuestionType::WeakSource.as_str(), "weak-source");
    }

    #[test]
    fn test_question_type_display() {
        assert_eq!(QuestionType::Temporal.to_string(), "temporal");
        assert_eq!(QuestionType::Conflict.to_string(), "conflict");
        assert_eq!(QuestionType::Missing.to_string(), "missing");
        assert_eq!(QuestionType::Ambiguous.to_string(), "ambiguous");
        assert_eq!(QuestionType::Stale.to_string(), "stale");
        assert_eq!(QuestionType::Duplicate.to_string(), "duplicate");
        assert_eq!(QuestionType::Corruption.to_string(), "corruption");
        assert_eq!(QuestionType::Precision.to_string(), "precision");
        assert_eq!(QuestionType::WeakSource.to_string(), "weak-source");
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
            QuestionType::Corruption,
            QuestionType::Precision,
            QuestionType::WeakSource,
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

    #[test]
    fn test_is_deferred_unchecked_with_answer() {
        let mut q = ReviewQuestion::new(QuestionType::Temporal, None, "When?".to_string());
        q.answer = Some("defer".to_string());
        assert!(q.is_deferred());
    }

    #[test]
    fn test_is_deferred_answered() {
        let mut q = ReviewQuestion::new(QuestionType::Temporal, None, "When?".to_string());
        q.answered = true;
        q.answer = Some("2024".to_string());
        assert!(!q.is_deferred());
    }

    #[test]
    fn test_is_deferred_unanswered_no_answer() {
        let q = ReviewQuestion::new(QuestionType::Temporal, None, "When?".to_string());
        assert!(!q.is_deferred());
    }

    #[test]
    fn test_is_believed_with_believed_prefix() {
        let mut q = ReviewQuestion::new(QuestionType::Stale, None, "Stale fact".to_string());
        q.answer = Some("believed: Still accurate per Wikipedia".to_string());
        assert!(q.is_believed());
        assert!(q.is_deferred()); // believed is a subset of deferred
    }

    #[test]
    fn test_is_believed_false_for_regular_defer() {
        let mut q = ReviewQuestion::new(QuestionType::Stale, None, "Stale fact".to_string());
        q.answer = Some("defer: could not find source".to_string());
        assert!(!q.is_believed());
        assert!(q.is_deferred());
    }

    #[test]
    fn test_is_believed_false_for_answered() {
        let mut q = ReviewQuestion::new(QuestionType::Stale, None, "Stale fact".to_string());
        q.answered = true;
        q.answer = Some("believed: answer".to_string());
        assert!(!q.is_believed()); // answered overrides believed
    }

    #[test]
    fn test_new_has_no_confidence() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "test".to_string());
        assert!(q.confidence.is_none());
        assert!(q.confidence_reason.is_none());
    }

    #[test]
    fn test_with_confidence_sets_fields() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "test".to_string())
            .with_confidence("low", "fact in glossary");
        assert_eq!(q.confidence.as_deref(), Some("low"));
        assert_eq!(q.confidence_reason.as_deref(), Some("fact in glossary"));
    }

    #[test]
    fn test_to_json_includes_confidence_when_set() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "When?".to_string())
            .with_confidence("low", "sourced from docs");
        let json = q.to_json();
        assert_eq!(json["confidence"], "low");
        assert_eq!(json["confidence_reason"], "sourced from docs");
    }

    #[test]
    fn test_to_json_omits_confidence_when_none() {
        let q = ReviewQuestion::new(QuestionType::Temporal, Some(5), "When?".to_string());
        let json = q.to_json();
        assert!(json.get("confidence").is_none());
        assert!(json.get("confidence_reason").is_none());
    }
}
