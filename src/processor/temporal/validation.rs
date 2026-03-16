//! Temporal tag validation.

use super::date::validate_date;
use super::parser::parse_temporal_tags;

/// Result of validating a temporal tag's date values
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalValidationError {
    /// Line number where the invalid tag was found
    pub line_number: usize,
    /// Raw text of the temporal tag
    pub raw_text: String,
    /// Description of the validation error
    pub message: String,
}

/// Validate all temporal tags in a document.
pub fn validate_temporal_tags(content: &str) -> Vec<TemporalValidationError> {
    let tags = parse_temporal_tags(content);
    let mut errors = Vec::new();

    for mut tag in tags {
        // Collect error messages for this tag
        let mut tag_errors: Vec<String> = Vec::new();

        if let Some(ref start) = tag.start_date {
            if let Some(msg) = validate_date(start) {
                tag_errors.push(format!("start date: {msg}"));
            }
        }

        if let Some(ref end) = tag.end_date {
            if let Some(msg) = validate_date(end) {
                tag_errors.push(format!("end date: {msg}"));
            }
        }

        // Create error structs, moving raw_text for the last one
        let error_count = tag_errors.len();
        for (i, message) in tag_errors.into_iter().enumerate() {
            let raw_text = if i == error_count - 1 {
                std::mem::take(&mut tag.raw_text)
            } else {
                tag.raw_text.clone()
            };
            errors.push(TemporalValidationError {
                line_number: tag.line_number,
                raw_text,
                message,
            });
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_bce_temporal_tags() {
        let content = "- Battle @t[=-0490-03]";
        let errors = validate_temporal_tags(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_unpadded_bce_tags() {
        let content = "- Battle @t[=-330]";
        let errors = validate_temporal_tags(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_bce_notation_validates() {
        let content = "- Battle @t[=331 BCE]";
        let errors = validate_temporal_tags(content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_bce_notation_range_validates() {
        let content = "- Wars @t[490 BCE..479 BCE]";
        let errors = validate_temporal_tags(content);
        assert!(errors.is_empty());
    }
}
