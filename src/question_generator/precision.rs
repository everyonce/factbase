//! Precision question generation.
//!
//! Generates `@q[precision]` questions for facts containing vague qualifiers,
//! weasel words, or ambiguous scope terms whose interpretation could change
//! the fact's truth value.

use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{extract_frontmatter_reviewed_date, is_suppressed_for_type, ReviewedType};

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
    (
        "overall",
        "does this mean personally directed, or delegated with oversight?",
    ),
    ("generally", "are there exceptions?"),
    ("largely", "what portion specifically?"),
    ("mostly", "what portion specifically?"),
    ("primarily", "what portion specifically?"),
    ("virtually", "are there exceptions?"),
    ("nearly", "can this be made more precise?"),
    ("almost", "can this be made more precise?"),
    ("widespread", "how widespread — what scope?"),
    // Subjective quality
    ("excellent", "excellent by what standard?"),
    ("great", "great by what measure?"),
    ("good", "good by what measure?"),
    ("poor", "poor by what measure?"),
    ("strong", "strong in what sense?"),
    ("weak", "weak in what sense?"),
    ("best", "best by what criteria?"),
    ("worst", "worst by what criteria?"),
    ("top", "top how — ranked by what?"),
    ("leading", "leading by what measure?"),
    ("premier", "premier by what standard?"),
    // Vague frequency
    ("often", "how often specifically?"),
    ("rarely", "how rarely — what frequency?"),
    ("frequently", "how frequently?"),
    ("occasionally", "how occasionally?"),
    ("sometimes", "how often specifically?"),
    ("always", "are there exceptions?"),
    ("never", "are there documented exceptions?"),
    ("usually", "what portion of the time?"),
    // Vague importance/notability
    ("important", "important to whom and in what way?"),
    ("notable", "notable by what standard?"),
    ("famous", "famous in what context?"),
    ("popular", "popular by what measure?"),
    ("common", "how common — what frequency?"),
    ("rare", "how rare — what frequency?"),
    ("unique", "unique in what respect?"),
    ("special", "special in what way?"),
    ("exceptional", "exceptional by what standard?"),
    // Scope weasels
    ("typically", "are there exceptions?"),
    ("traditionally", "does this still hold?"),
    ("historically", "in what period specifically?"),
    ("effectively", "effective how — by what measure?"),
    ("essentially", "are there meaningful exceptions?"),
    ("arguably", "what is the counterargument?"),
    ("relatively", "relative to what?"),
    ("comparatively", "compared to what?"),
];

/// Regex that strips temporal tags, source refs, and reviewed markers from a
/// fact line so we only match vague words in the actual claim text.
static STRIP_ANNOTATIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[[^\]]*\]|\[\^\d+\]|<!--\s*reviewed:[^>]*-->").expect("strip regex")
});

/// Strips wikilinks: `[[Entity Name]]` and `[[Entity Name|display]]`.
static STRIP_WIKILINKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[[^\]]*\]\]").expect("wikilink regex")
});

/// Strips title-case proper noun phrases: 2+ consecutive words where each
/// word starts with an uppercase letter, with lowercase prepositions/articles
/// (of, the, a, an, in, on, at, for, to, by, with, and, or) allowed between
/// capitalized words.
static STRIP_PROPER_NOUNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[A-Z][A-Za-z'-]*(?:\s+(?:of|the|a|an|in|on|at|for|to|by|with|and|or)\s+[A-Z][A-Za-z'-]*|\s+[A-Z][A-Za-z'-]*)+"
    ).expect("proper noun regex")
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
    let fm_skip = extract_frontmatter_reviewed_date(content)
        .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS);

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with a recent reviewed marker (inline or frontmatter)
        if fm_skip
            || is_suppressed_for_type(line, ReviewedType::Precision, today, REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        // Strip annotations so we only match in the claim text itself
        let clean = STRIP_ANNOTATIONS.replace_all(line, "");
        // Strip wikilinks and title-case proper noun phrases to avoid false positives
        let clean = STRIP_WIKILINKS.replace_all(&clean, "");
        let clean = STRIP_PROPER_NOUNS.replace_all(&clean, "");

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
    fn test_typed_precision_marker_suppresses_precision_only() {
        let today = chrono::Utc::now().date_naive();
        let marker = today - chrono::Duration::days(30);
        // <!-- reviewed:p:DATE --> suppresses precision
        let content = format!(
            "# Entity\n\n- Resulted in heavy losses <!-- reviewed:p:{} -->",
            marker.format("%Y-%m-%d")
        );
        assert!(generate_precision_questions(&content).is_empty());
    }

    #[test]
    fn test_typed_temporal_marker_does_not_suppress_precision() {
        let today = chrono::Utc::now().date_naive();
        let marker = today - chrono::Duration::days(30);
        // <!-- reviewed:t:DATE --> does NOT suppress precision
        let content = format!(
            "# Entity\n\n- Resulted in heavy losses <!-- reviewed:t:{} -->",
            marker.format("%Y-%m-%d")
        );
        assert_eq!(generate_precision_questions(&content).len(), 1);
    }

    #[test]
    fn test_case_insensitive() {
        let content = "# Entity\n\n- HEAVY losses reported";
        assert_eq!(generate_precision_questions(content).len(), 1);
    }

    #[test]
    fn test_subjective_quality_flagged() {
        let content = "# Entity\n\n- Considered an excellent performer";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("excellent"));
    }

    #[test]
    fn test_vague_frequency_flagged() {
        let content = "# Entity\n\n- Often cited in reviews";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("how often"));
    }

    #[test]
    fn test_scope_weasel_flagged() {
        let content = "# Entity\n\n- Arguably the most influential work";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("counterargument"));
    }

    #[test]
    fn test_vague_notability_flagged() {
        let content = "# Entity\n\n- A unique approach to the problem";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("unique"));
    }

    #[test]
    fn test_line_ref_correct() {
        let content = "# Title\n\nParagraph\n\n- Precise fact\n- Resulted in heavy losses";
        let q = generate_precision_questions(content);
        assert_eq!(q[0].line_ref, Some(6));
    }

    #[test]
    fn test_proper_noun_key_not_flagged() {
        // "Key of Khaj-Nisut" is a proper noun — "key" should not fire
        let content = "# Entity\n\n- Key of Khaj-Nisut provides HP scaling";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_proper_noun_light_not_flagged() {
        // "Light of Foliar Incision" is a proper noun — "light" should not fire
        let content = "# Entity\n\n- Light of Foliar Incision scales with EM";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_wikilink_proper_noun_not_flagged() {
        // [[Staff of Homa]] wikilink — "critical" in the wikilink text should not fire,
        // but standalone "critical" outside the wikilink still fires
        let content = "# Entity\n\n- [[Staff of Homa]] is essential for her kit";
        assert!(generate_precision_questions(content).is_empty());
    }

    #[test]
    fn test_standalone_critical_still_flagged() {
        // "critical" used as a standalone vague qualifier should still fire
        let content = "# Entity\n\n- This mechanic is critical for success";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("critical"));
    }

    #[test]
    fn test_heavy_losses_still_flagged() {
        // "heavy" as a standalone vague qualifier should still fire
        let content = "# Entity\n\n- Resulted in heavy losses";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("heavy"));
    }

    #[test]
    fn test_lowercase_key_still_flagged() {
        // "key" in lowercase standalone context (not part of a proper noun) should fire
        let content = "# Entity\n\n- Key gameplay mechanic for progression";
        let q = generate_precision_questions(content);
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("key"));
    }
}
