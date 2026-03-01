//! Precision question generation.
//!
//! Generates `@q[precision]` questions for facts containing vague qualifiers,
//! weasel words, or ambiguous scope terms whose interpretation could change
//! the fact's truth value.

use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::extract_reviewed_date;

use super::iter_fact_lines;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Vague qualifier patterns — words whose definition is ambiguous enough to
/// change a fact's truthfulness. Domain-agnostic by design.
///
/// Each entry is `(pattern, question_fragment)` where the pattern is matched
/// as a whole word (case-insensitive) and the fragment completes the question.
const VAGUE_QUALIFIERS: &[(&str, &str)] = &[
    // Subjective magnitude
    ("heavy", "what counts as \"heavy\"?"),
    ("light", "what counts as \"light\"?"),
    ("significant", "significant by what measure?"),
    ("crucial", "crucial how — without it, the outcome changes?"),
    ("key", "\"key\" in what sense?"),
    ("major", "major by what standard?"),
    ("minor", "minor by what standard?"),
    ("massive", "what counts as \"massive\"?"),
    ("substantial", "substantial by what measure?"),
    ("considerable", "considerable by what measure?"),
    ("negligible", "negligible by what standard?"),
    ("critical", "critical how — failure without it?"),
    ("vital", "vital in what sense?"),
    ("pivotal", "pivotal how?"),
    ("instrumental", "instrumental how?"),
    // Vague quantities
    ("approximately", "can this be made more precise?"),
    ("roughly", "can this be made more precise?"),
    ("several", "how many specifically?"),
    ("numerous", "how many specifically?"),
    ("many", "how many specifically?"),
    ("few", "how many specifically?"),
    ("some", "how many or which ones specifically?"),
    // Vague time
    ("shortly", "how long specifically?"),
    ("soon", "how long specifically?"),
    ("recently", "when specifically?"),
    ("eventually", "when specifically?"),
    // Vague scope
    ("overall", "does this mean personally directed, or delegated with oversight?"),
    ("generally", "are there exceptions?"),
    ("largely", "what portion specifically?"),
    ("mostly", "what portion specifically?"),
    ("primarily", "what portion specifically?"),
    ("virtually", "are there exceptions?"),
    ("nearly", "can this be made more precise?"),
    ("almost", "can this be made more precise?"),
    ("widespread", "how widespread — what scope?"),
];

/// Regex that strips temporal tags, source refs, and reviewed markers from a
/// fact line so we only match vague words in the actual claim text.
static STRIP_ANNOTATIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[[^\]]*\]|\[\^\d+\]|<!--\s*reviewed:[^>]*-->").expect("strip regex")
});

/// Build a word-boundary regex for a qualifier.
fn qualifier_regex(word: &str) -> Regex {
    Regex::new(&format!(r"(?i)\b{}\b", regex::escape(word))).expect("qualifier regex")
}

/// Generate precision questions for a document.
///
/// Scans fact lines for vague qualifiers and subjective terms whose
/// interpretation could change the fact's truth value.
///
/// Returns a list of `ReviewQuestion` with `question_type = Precision`.
pub fn generate_precision_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let today = Utc::now().date_naive();

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with a recent reviewed marker
        if extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        // Strip annotations so we only match in the claim text itself
        let clean = STRIP_ANNOTATIONS.replace_all(line, "");

        // Find the first vague qualifier match
        for &(word, question_frag) in VAGUE_QUALIFIERS {
            let re = qualifier_regex(word);
            if re.is_match(&clean) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Precision,
                    Some(line_number),
                    format!("\"{fact_text}\" — {question_frag} Can it be made more precise?"),
                ));
                break; // One question per fact line
            }
        }
    }

    questions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_facts_no_questions() {
        assert!(generate_precision_questions("# Title\n\nParagraph text.").is_empty());
    }

    #[test]
    fn test_precise_facts_not_flagged() {
        let content = "# Entity\n\n- 3,057 killed in the battle\n- Founded on 2024-01-15\n- Admiral Nimitz commanded the fleet";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_vague_magnitude_flagged() {
        let content = "# Entity\n\n- Resulted in heavy losses";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].question_type, QuestionType::Precision);
        assert!(q[0].description.contains("heavy"));
    }

    #[test]
    fn test_vague_quantity_flagged() {
        let content = "# Entity\n\n- Approximately 3,000 casualties reported";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("precise"));
    }

    #[test]
    fn test_vague_time_flagged() {
        let content = "# Entity\n\n- Shortly after the event, operations resumed";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("how long"));
    }

    #[test]
    fn test_vague_scope_flagged() {
        let content = "# Entity\n\n- Directed overall strategy for the division";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("overall"));
    }

    #[test]
    fn test_crucial_role_flagged() {
        let content = "# Entity\n\n- Played a crucial role in the outcome";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("crucial"));
    }

    #[test]
    fn test_one_question_per_line() {
        // Line has both "heavy" and "significant" — only one question
        let content = "# Entity\n\n- Heavy and significant losses reported";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_multiple_lines_multiple_questions() {
        let content = "# Entity\n\n- Resulted in heavy losses\n- Approximately 500 involved\n- Precise count: 347 confirmed";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 2); // heavy + approximately, not the precise line
    }

    #[test]
    fn test_word_boundary_matching() {
        // "keyboard" contains "key" but shouldn't match
        let content = "# Entity\n\n- Used a keyboard for input";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_temporal_tag_content_not_matched() {
        // "recently" inside a temporal tag should not trigger
        let content = "# Entity\n\n- Joined the team @t[~2024-01]";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_reviewed_marker_suppresses() {
        let today = chrono::Utc::now().date_naive();
        let marker = today - chrono::Duration::days(30);
        let content = format!(
            "# Entity\n\n- Resulted in heavy losses <!-- reviewed:{} -->",
            marker.format("%Y-%m-%d")
        );
        assert!(generate_precision_questions(&content).is_empty());

        // Old marker does NOT suppress
        let content2 = "# Entity\n\n- Resulted in heavy losses <!-- reviewed:2020-01-01 -->";
        assert_eq!(generate_precision_questions(content2).len(), 1);
    }

    #[test]
    fn test_case_insensitive() {
        let content = "# Entity\n\n- HEAVY losses reported";
        assert_eq!(generate_precision_questions(content).len(), 1);
    }

    #[test]
    fn test_line_ref_correct() {
        let content = "# Title\n\nParagraph\n\n- Precise fact\n- Resulted in heavy losses";
        let q = generate_precision_questions(content);
        assert_eq!(q[0].line_ref, Some(6));
    }
}
