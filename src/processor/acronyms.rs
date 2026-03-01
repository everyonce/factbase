//! Inline acronym expansion deduplication.
//!
//! Detects parenthetical acronym expansions like `DR (Disaster Recovery)` and
//! ensures each acronym is expanded at most once per document. Expansions for
//! terms that exist in a glossary are stripped entirely.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Matches `ACRONYM (Expansion…)` where ACRONYM is 2-10 uppercase letters/digits/&
/// and the parenthetical starts with an uppercase letter (distinguishing expansions
/// from normal parentheticals like "(see above)").
static ACRONYM_EXPANSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([A-Z][A-Z0-9&]{1,9})\s+\(([A-Z][^)]{2,})\)").unwrap()
});

/// Remove duplicate inline acronym expansions from `content`.
///
/// - First expansion of each acronym is kept (unless it appears in `glossary_terms`).
/// - Subsequent expansions of the same acronym are stripped (parenthetical removed).
/// - If the acronym appears in `glossary_terms`, ALL expansions are stripped.
pub fn dedup_acronym_expansions(content: &str, glossary_terms: &HashSet<String>) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = String::with_capacity(content.len());

    for line in content.split('\n') {
        if result.is_empty() {
            // first line — no leading newline
        } else {
            result.push('\n');
        }
        result.push_str(&dedup_line(line, &mut seen, glossary_terms));
    }

    result
}

fn dedup_line(line: &str, seen: &mut HashSet<String>, glossary_terms: &HashSet<String>) -> String {
    let mut out = String::with_capacity(line.len());
    let mut last_end = 0;

    for caps in ACRONYM_EXPANSION_RE.captures_iter(line) {
        let full = caps.get(0).unwrap();
        let acronym = &caps[1];
        let key = acronym.to_uppercase();

        let in_glossary = glossary_terms.iter().any(|t| t.eq_ignore_ascii_case(acronym));
        let is_duplicate = !seen.insert(key.clone());

        if is_duplicate || in_glossary {
            // Keep text before this match, then just the acronym (no parenthetical)
            out.push_str(&line[last_end..full.start()]);
            out.push_str(acronym);
            last_end = full.end();
        }
    }

    out.push_str(&line[last_end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_glossary() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn test_no_expansions_unchanged() {
        let input = "- Uses DR for backup\n- Another line";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), input);
    }

    #[test]
    fn test_single_expansion_kept() {
        let input = "- Uses DR (Disaster Recovery) for backup";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), input);
    }

    #[test]
    fn test_duplicate_expansion_stripped() {
        let input = "- Uses DR (Disaster Recovery) for backup\n- DR (Disaster Recovery) plan active";
        let expected = "- Uses DR (Disaster Recovery) for backup\n- DR plan active";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_multiple_duplicates_stripped() {
        let input = "\
- DR (Disaster Recovery) plan\n\
- DR (Disaster Recovery) site\n\
- DR (Disaster Recovery) testing\n\
- DR (Disaster Recovery) budget";
        let expected = "\
- DR (Disaster Recovery) plan\n\
- DR site\n\
- DR testing\n\
- DR budget";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_different_acronyms_each_kept_once() {
        let input = "\
- DR (Disaster Recovery) plan\n\
- SLA (Service Level Agreement) defined\n\
- DR (Disaster Recovery) site\n\
- SLA (Service Level Agreement) review";
        let expected = "\
- DR (Disaster Recovery) plan\n\
- SLA (Service Level Agreement) defined\n\
- DR site\n\
- SLA review";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_glossary_term_strips_all() {
        let mut glossary = HashSet::new();
        glossary.insert("DR".to_string());
        let input = "- DR (Disaster Recovery) plan\n- DR (Disaster Recovery) site";
        let expected = "- DR plan\n- DR site";
        assert_eq!(dedup_acronym_expansions(input, &glossary), expected);
    }

    #[test]
    fn test_glossary_case_insensitive() {
        let mut glossary = HashSet::new();
        glossary.insert("dr".to_string());
        let input = "- DR (Disaster Recovery) plan";
        let expected = "- DR plan";
        assert_eq!(dedup_acronym_expansions(input, &glossary), expected);
    }

    #[test]
    fn test_normal_parenthetical_not_stripped() {
        // Lowercase start inside parens → not an acronym expansion
        let input = "- DR (see above) plan\n- DR (see above) again";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), input);
    }

    #[test]
    fn test_short_parens_not_matched() {
        // Content inside parens too short (< 3 chars)
        let input = "- DR (OK) plan";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), input);
    }

    #[test]
    fn test_expansion_with_extra_context() {
        let input = "\
- DR (Disaster Recovery. Standard IT term for business continuity) plan\n\
- DR (Disaster Recovery. Standard IT term for business continuity) site";
        let expected = "\
- DR (Disaster Recovery. Standard IT term for business continuity) plan\n\
- DR site";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_preserves_factbase_header() {
        let input = "<!-- factbase:abc123 -->\n# Title\n\n- DR (Disaster Recovery) plan\n- DR (Disaster Recovery) site";
        let expected = "<!-- factbase:abc123 -->\n# Title\n\n- DR (Disaster Recovery) plan\n- DR site";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_two_expansions_same_line() {
        let input = "- DR (Disaster Recovery) and SLA (Service Level Agreement) defined\n- DR (Disaster Recovery) and SLA (Service Level Agreement) review";
        let expected = "- DR (Disaster Recovery) and SLA (Service Level Agreement) defined\n- DR and SLA review";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_empty_content() {
        assert_eq!(dedup_acronym_expansions("", &empty_glossary()), "");
    }

    #[test]
    fn test_ampersand_acronym() {
        let input = "- R&D (Research and Development) budget\n- R&D (Research and Development) team";
        let expected = "- R&D (Research and Development) budget\n- R&D team";
        assert_eq!(dedup_acronym_expansions(input, &empty_glossary()), expected);
    }

    #[test]
    fn test_glossary_plus_duplicate() {
        // SLA in glossary (all stripped), DR not (first kept, rest stripped)
        let mut glossary = HashSet::new();
        glossary.insert("SLA".to_string());
        let input = "\
- SLA (Service Level Agreement) defined\n\
- DR (Disaster Recovery) plan\n\
- SLA (Service Level Agreement) review\n\
- DR (Disaster Recovery) site";
        let expected = "\
- SLA defined\n\
- DR (Disaster Recovery) plan\n\
- SLA review\n\
- DR site";
        assert_eq!(dedup_acronym_expansions(input, &glossary), expected);
    }
}
