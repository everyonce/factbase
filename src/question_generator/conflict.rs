//! Conflict question generation.
//!
//! Generates `@q[conflict]` questions for overlapping date ranges
//! within the same document section.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{
    extract_reviewed_date, normalize_date_for_comparison, date_cmp, FACT_LINE_REGEX, MANUAL_LINK_REGEX,
};
use crate::processor::parse_temporal_tags;

use super::extract_fact_text;

/// Recognized conflict pattern for agent guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPattern {
    /// Two overlapping facts about different entities that may legitimately coexist.
    ParallelOverlap,
    /// Two overlapping facts about the same entity where one likely supersedes the other.
    SameEntityTransition,
    /// Overlap is small relative to the spans — likely date-source imprecision.
    DateImprecision,
    /// No recognized pattern — needs manual investigation.
    Unknown,
}

impl ConflictPattern {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::ParallelOverlap => "parallel_overlap",
            Self::SameEntityTransition => "same_entity_transition",
            Self::DateImprecision => "date_imprecision",
            Self::Unknown => "unknown",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::ParallelOverlap => "Overlapping facts about different entities that may legitimately coexist. If both are valid parallel facts, answer: 'Not a conflict: parallel overlap' and mark both facts with <!-- reviewed:YYYY-MM-DD -->.",
            Self::SameEntityTransition => "Overlapping facts about the same entity where one likely supersedes the other. If sequential progression, answer: 'Not a conflict: transition — [earlier fact] ended when [later fact] began' and adjust the end date of the earlier entry.",
            Self::DateImprecision => "Overlap is small relative to the date ranges — likely imprecision from the data source. If the facts are clearly sequential, answer: 'Not a conflict: date imprecision — adjust [fact] end date to [date]'.",
            Self::Unknown => "Investigate which fact is current.",
        }
    }
}

/// Classify the likely conflict pattern between two overlapping facts.
pub fn classify_conflict_pattern(
    text1: &str,
    text2: &str,
    start1: &str,
    end1: &str,
    start2: &str,
    end2: &str,
) -> ConflictPattern {
    let names1 = extract_proper_names(text1);
    let names2 = extract_proper_names(text2);
    let shared_entity = names1.iter().any(|n| names2.contains(n))
        || has_shared_significant_word(text1, text2);

    // Same entity with overlap → transition (one supersedes the other)
    if shared_entity {
        return ConflictPattern::SameEntityTransition;
    }

    // Different entities overlapping → parallel overlap
    // (the `facts_may_conflict` check already excluded facts with
    //  completely different proper names, so reaching here means
    //  the names are ambiguous or absent — but different-entity
    //  overlaps with significant duration are typically parallel)
    let s1 = normalize_date_for_comparison(start1);
    let e1 = normalize_date_for_comparison(end1);
    let s2 = normalize_date_for_comparison(start2);
    let e2 = normalize_date_for_comparison(end2);

    // Compute overlap size in months
    let overlap_start = if date_cmp(&s1, &s2) == std::cmp::Ordering::Greater { &s1 } else { &s2 };
    let overlap_end = if date_cmp(&e1, &e2) == std::cmp::Ordering::Less { &e1 } else { &e2 };
    let overlap_months = months_between(overlap_start, overlap_end);
    let span1 = months_between(&s1, &e1).max(1);
    let span2 = months_between(&s2, &e2).max(1);
    let min_span = span1.min(span2);

    // Small overlap relative to spans → date imprecision
    if overlap_months <= 6 && min_span > 12 {
        return ConflictPattern::DateImprecision;
    }

    // Significant overlap at different entities → parallel overlap
    if overlap_months > 6 {
        return ConflictPattern::ParallelOverlap;
    }

    ConflictPattern::Unknown
}

/// Approximate month count between two normalized date strings.
fn months_between(a: &str, b: &str) -> i32 {
    let parse = |d: &str| -> (i32, i32) {
        let (neg, rest) = if d.starts_with('-') {
            (true, &d[1..])
        } else {
            (false, d)
        };
        let parts: Vec<&str> = rest.split('-').collect();
        let y: i32 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
        let m: i32 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(1);
        (if neg { -y } else { y }, m)
    };
    let (y1, m1) = parse(a);
    let (y2, m2) = parse(b);
    (y2 * 12 + m2) - (y1 * 12 + m1)
}

/// A fact with its temporal range for conflict detection.
#[derive(Debug)]
struct FactWithRange {
    line_number: usize,
    text: String,
    section: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    is_ongoing: bool,
}

/// Filter out conflict questions whose referenced line participates in a
/// boundary-month sequential pattern.  This catches conflicts from
/// ANY generator (rule-based or LLM cross-validation).
pub fn filter_sequential_conflicts(
    content: &str,
    questions: &mut Vec<ReviewQuestion>,
) {
    if questions.is_empty() {
        return;
    }
    let facts = collect_facts_with_ranges(content, false);
    if facts.len() < 2 {
        return;
    }
    // Build set of line numbers that participate in sequential patterns
    let mut sequential_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for i in 0..facts.len() {
        for j in (i + 1)..facts.len() {
            let (f1, f2) = (&facts[i], &facts[j]);
            let (Some(s1), Some(s2)) = (f1.start_date.as_deref(), f2.start_date.as_deref())
            else {
                continue;
            };
            let e1 = if f1.is_ongoing {
                "9999-12-31"
            } else {
                f1.end_date.as_deref().unwrap_or(s1)
            };
            let e2 = if f2.is_ongoing {
                "9999-12-31"
            } else {
                f2.end_date.as_deref().unwrap_or(s2)
            };
            if is_boundary_month_sequential(s1, e1, s2, e2)
                || is_shared_entity_sequential(&f1.text, &f2.text, s1, s2)
            {
                sequential_lines.insert(f1.line_number);
                sequential_lines.insert(f2.line_number);
            }
        }
    }
    if sequential_lines.is_empty() {
        return;
    }
    questions.retain(|q| {
        !(q.question_type == QuestionType::Conflict
            && q.line_ref.is_some_and(|ln| sequential_lines.contains(&ln)))
    });
}

/// Generate conflict questions for a document.
///
/// Detects overlapping date ranges for facts within the same section.
/// Facts with different proper names or manual links are excluded.
/// Sequential entries with boundary-month overlaps are suppressed.
///
/// Returns a list of `ReviewQuestion` with `question_type = Conflict`.
pub fn generate_conflict_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();

    // Collect facts with temporal ranges
    let facts = collect_facts_with_ranges(content, true);

    // Check for overlapping ranges between facts
    for i in 0..facts.len() {
        for j in (i + 1)..facts.len() {
            if let Some(question) = check_fact_conflict(&facts[i], &facts[j]) {
                questions.push(question);
            }
        }
    }

    questions
}

/// Collect all facts (list items) with their temporal ranges.
///
/// When `skip_reviewed` is true, facts with any `<!-- reviewed:... -->` marker
/// are permanently excluded — conflict overlaps are structural (the dates don't
/// change), so once reviewed the suppression never expires.
/// When false, all facts are included (used for boundary-month filtering — need
/// the complete picture to detect sequential pairs).
fn collect_facts_with_ranges(content: &str, skip_reviewed: bool) -> Vec<FactWithRange> {
    let mut facts = Vec::new();
    let tags = parse_temporal_tags(content);
    let mut current_section: Option<String> = None;

    // Stop before the review queue section
    let end = crate::patterns::body_end_offset(content);

    for (line_idx, line) in content[..end].lines().enumerate() {
        let line_number = line_idx + 1;

        // Track section headings
        if line.starts_with("## ") {
            current_section = Some(line.trim_start_matches('#').trim().to_string());
            continue;
        }

        // Only process fact lines (list items)
        if !FACT_LINE_REGEX.is_match(line) {
            continue;
        }

        // Skip facts with a reviewed marker — conflict overlaps are structural
        // (the dates don't change), so once reviewed the suppression is permanent.
        if skip_reviewed && extract_reviewed_date(line).is_some() {
            continue;
        }

        // Skip facts annotated as sequential (permanent suppression)
        if line.contains("<!-- sequential") {
            continue;
        }

        // Find temporal tags on this line
        let line_tags: Vec<_> = tags
            .iter()
            .filter(|t| t.line_number == line_number)
            .collect();

        // Extract the best range from tags
        let (start_date, end_date, is_ongoing) = if line_tags.is_empty() {
            (None, None, false)
        } else {
            // Prefer Range/Ongoing tags, fall back to PointInTime/LastSeen as single-point ranges
            let tag = line_tags
                .iter()
                .find(|t| {
                    matches!(
                        t.tag_type,
                        crate::models::TemporalTagType::Range
                            | crate::models::TemporalTagType::Ongoing
                    )
                })
                .or_else(|| {
                    line_tags.iter().find(|t| {
                        matches!(
                            t.tag_type,
                            crate::models::TemporalTagType::PointInTime
                                | crate::models::TemporalTagType::LastSeen
                        )
                    })
                });

            let Some(tag) = tag else { continue };

            let is_ongoing = matches!(tag.tag_type, crate::models::TemporalTagType::Ongoing);

            match tag.tag_type {
                crate::models::TemporalTagType::PointInTime
                | crate::models::TemporalTagType::LastSeen => {
                    // Treat as a single-point range: start == end
                    let date = tag.start_date.clone();
                    (date.clone(), date, false)
                }
                _ => (tag.start_date.clone(), tag.end_date.clone(), is_ongoing),
            }
        };

        facts.push(FactWithRange {
            line_number,
            text: extract_fact_text(line),
            section: current_section.clone(),
            start_date,
            end_date,
            is_ongoing,
        });
    }

    facts
}

/// Check if two facts have a conflict (overlapping ranges within the same section).
fn check_fact_conflict(fact1: &FactWithRange, fact2: &FactWithRange) -> Option<ReviewQuestion> {
    // Both facts need temporal info to detect overlap
    let start1 = fact1.start_date.as_deref()?;
    let start2 = fact2.start_date.as_deref()?;

    // Only compare facts within the same section
    if fact1.section != fact2.section {
        return None;
    }

    // Determine end dates (ongoing = far future for comparison)
    let end1 = if fact1.is_ongoing {
        "9999-12-31"
    } else {
        fact1.end_date.as_deref().unwrap_or(start1)
    };
    let end2 = if fact2.is_ongoing {
        "9999-12-31"
    } else {
        fact2.end_date.as_deref().unwrap_or(start2)
    };

    // Check if ranges overlap
    if !ranges_overlap(start1, end1, start2, end2) {
        return None;
    }

    // Check general exclusions (different entities, manual links)
    if !facts_may_conflict(&fact1.text, &fact2.text) {
        return None;
    }

    // Suppress boundary-month overlaps in sequential entries.
    // Data sources often report the transition date in both entries, e.g.
    // entry A ends 2023-09, entry B starts 2023-09.  This is not a real conflict.
    if is_boundary_month_sequential(start1, end1, start2, end2) {
        return None;
    }

    // Suppress overlaps for sequential entries at the same entity.
    // When two facts share a common proper name and one clearly starts before
    // the other, the overlap is due to date granularity in transitions.
    // E.g. "Phase 1 at Acme @t[2015..2019]" and "Phase 2 at Acme @t[2018..2023]"
    // — the 2018-2019 overlap is imprecision, not a real conflict.
    if is_shared_entity_sequential(&fact1.text, &fact2.text, start1, start2) {
        return None;
    }

    // Classify the conflict pattern and include hint in description
    let pattern = classify_conflict_pattern(
        &fact1.text, &fact2.text, start1, end1, start2, end2,
    );

    // Generate conflict question with pattern hint
    let description = format!(
        "\"{}\" @t[{}..{}] overlaps with \"{}\" @t[{}..{}] - were both true simultaneously? (line:{}) [pattern:{}]",
        fact1.text,
        start1,
        if fact1.is_ongoing { "" } else { end1 },
        fact2.text,
        start2,
        if fact2.is_ongoing { "" } else { end2 },
        fact2.line_number,
        pattern.tag(),
    );

    Some(ReviewQuestion::new(
        QuestionType::Conflict,
        Some(fact1.line_number),
        description,
    ))
}

/// Check if two date ranges overlap.
fn ranges_overlap(start1: &str, end1: &str, start2: &str, end2: &str) -> bool {
    let s1 = normalize_date_for_comparison(start1);
    let e1 = normalize_date_for_comparison(end1);
    let s2 = normalize_date_for_comparison(start2);
    let e2 = normalize_date_for_comparison(end2);

    // Ranges overlap if: start1 <= end2 AND start2 <= end1
    date_cmp(&s1, &e2) != std::cmp::Ordering::Greater && date_cmp(&s2, &e1) != std::cmp::Ordering::Greater
}

/// Check if two facts could potentially conflict.
///
/// Returns false for facts that are clearly about different entities
/// (different proper names) or are cross-reference roster entries.
fn facts_may_conflict(text1: &str, text2: &str) -> bool {
    // Roster lines with cross-references are distinct entries, not conflicts
    if MANUAL_LINK_REGEX.is_match(text1) || MANUAL_LINK_REGEX.is_match(text2) {
        return false;
    }

    // If both facts mention different proper names, they describe
    // different entities and aren't mutually exclusive
    if contains_different_proper_names(text1, text2) {
        return false;
    }

    // Key-value facts with different keys describe independent attributes
    // (e.g. "Email: x" vs "Phone: y") and cannot conflict with each other.
    if have_different_attribute_keys(text1, text2) {
        return false;
    }

    true
}

/// Extract the attribute key from a `Key: Value` or `**Key**: Value` fact.
/// Returns `None` if the fact doesn't follow a key-value pattern.
fn extract_attribute_key(text: &str) -> Option<&str> {
    // Find the first colon that's followed by a space (to avoid matching URLs like http:)
    let colon_pos = text.find(": ")?;
    let key = &text[..colon_pos];
    // Strip bold markers
    let key = key.trim_start_matches("**").trim_end_matches("**");
    let key = key.trim();
    // Must be a short label (≤ 4 words), not a sentence fragment
    if key.is_empty() || key.split_whitespace().count() > 4 {
        return None;
    }
    Some(key)
}

/// Returns true if both facts have key-value structure with different keys.
fn have_different_attribute_keys(text1: &str, text2: &str) -> bool {
    match (extract_attribute_key(text1), extract_attribute_key(text2)) {
        (Some(k1), Some(k2)) => !k1.eq_ignore_ascii_case(k2),
        _ => false,
    }
}

/// Extract the year prefix from a date string ("2018" → "2018", "-0490" → "-0490").
fn extract_year(date: &str) -> Option<&str> {
    let rest = date.strip_prefix('-').unwrap_or(date);
    if rest.len() >= 4 && rest[..4].chars().all(|c| c.is_ascii_digit()) {
        let end = if date.starts_with('-') { 5 } else { 4 };
        Some(&date[..end])
    } else {
        None
    }
}

/// Check if two date strings share the same calendar year.
fn dates_same_year(date_a: &str, date_b: &str) -> bool {
    matches!((extract_year(date_a), extract_year(date_b)), (Some(a), Some(b)) if a == b)
}

/// Check if two normalized date strings are within one month of each other.
fn dates_within_one_month(date_a: &str, date_b: &str) -> bool {
    // Normalized dates are like "2020-01-01". Compare as strings — if they match
    // or differ by at most one month, return true.
    if date_a == date_b {
        return true;
    }
    // Parse year-month from the normalized dates
    let parse = |d: &str| -> Option<(i32, i32)> {
        let (neg, rest) = if d.starts_with('-') {
            (true, &d[1..])
        } else {
            (false, d)
        };
        let parts: Vec<&str> = rest.split('-').collect();
        if parts.len() >= 2 {
            let year: i32 = parts[0].parse().ok()?;
            let month: i32 = parts[1].parse().ok()?;
            Some((if neg { -year } else { year }, month))
        } else {
            None
        }
    };
    let Some((y1, m1)) = parse(date_a) else {
        return false;
    };
    let Some((y2, m2)) = parse(date_b) else {
        return false;
    };
    let months1 = y1 * 12 + m1;
    let months2 = y2 * 12 + m2;
    (months1 - months2).abs() <= 1
}

/// Returns true when two date ranges are sequential with at most a boundary-month
/// overlap (end of one equals start of the other).  Data sources often report the
/// transition date in both entries, so `@t[..2023-09]` + `@t[2023-09..]` is not
/// a real conflict — it's a normal sequential transition.
fn is_boundary_month_sequential(start1: &str, end1: &str, start2: &str, end2: &str) -> bool {
    let e1 = normalize_date_for_comparison(end1);
    let s2 = normalize_date_for_comparison(start2);
    let e2 = normalize_date_for_comparison(end2);
    let s1 = normalize_date_for_comparison(start1);
    // Sequential: end of one is within one month of start of the other.
    // Data sources often report the transition date in both entries, or off by one month
    // due to date granularity. Use strict less-than to exclude point-in-time facts.
    let month_seq = (dates_within_one_month(&e1, &s2) && date_cmp(&s1, &s2) == std::cmp::Ordering::Less)
        || (dates_within_one_month(&e2, &s1) && date_cmp(&s2, &s1) == std::cmp::Ordering::Less);
    if month_seq {
        return true;
    }
    // Boundary-year overlap: the end year of one entry matches the start year of
    // the next.  Data sources commonly have month-level imprecision within the
    // transition year (e.g. entry A ends 2018-06, entry B starts 2018-01).  When
    // the entries are clearly sequential (one starts years before the other) and
    // the only overlap is within a single calendar year, suppress the conflict.
    (dates_same_year(end1, start2) && date_cmp(&s1, &s2) == std::cmp::Ordering::Less) || (dates_same_year(end2, start1) && date_cmp(&s2, &s1) == std::cmp::Ordering::Less)
}

/// Returns true when two facts share a common entity name and one clearly
/// starts before the other, indicating a sequential transition (e.g., role
/// change, phase progression) rather than a genuine conflict.
///
/// Detects shared entities via two methods:
/// 1. Multi-word proper names (e.g., "Tivity Health", "Acme Corp")
/// 2. Shared significant words — catches single-word names, camelCase (e.g.,
///    "axialHealthcare"), and other patterns that proper-name extraction misses.
fn is_shared_entity_sequential(text1: &str, text2: &str, start1: &str, start2: &str) -> bool {
    let names1 = extract_proper_names(text1);
    let names2 = extract_proper_names(text2);
    let shared = names1.iter().any(|n| names2.contains(n))
        || has_shared_significant_word(text1, text2);
    if !shared {
        return false;
    }
    // One must clearly start before the other (not simultaneous)
    let s1 = normalize_date_for_comparison(start1);
    let s2 = normalize_date_for_comparison(start2);
    date_cmp(&s1, &s2) != std::cmp::Ordering::Equal
}

/// Check if two fact texts share a significant word (likely an entity/company name).
///
/// A word is "significant" if it is ≥4 characters, not a common English word
/// that frequently appears capitalized, and not temporal/footnote markup.
/// This catches single-word entity names ("Google"), camelCase names
/// ("axialHealthcare"), and other patterns that `extract_proper_names`
/// (which requires 2+ consecutive capitalized words) would miss.
fn has_shared_significant_word(text1: &str, text2: &str) -> bool {
    let words1 = extract_significant_words(text1);
    let words2 = extract_significant_words(text2);
    words1.iter().any(|w| words2.contains(w))
}

/// Extract significant words from a fact line for entity matching.
///
/// Only extracts words that look like entity names: must contain at least one
/// letter, have at least one uppercase letter (proper noun or camelCase), be
/// ≥4 characters, and not be a common English word that appears capitalized.
/// Words are lowercased for case-insensitive comparison.
fn extract_significant_words(text: &str) -> Vec<String> {
    static STOP_WORDS: &[&str] = &[
        // Common English words that frequently appear capitalized in context
        // (titles, roles, descriptors). Excluded to avoid false entity matches.
        "Senior", "Junior", "Lead", "Chief", "Head", "Director", "Manager",
        "Engineer", "Developer", "Analyst", "Consultant", "Founder", "President",
        "Officer", "Architect", "Designer", "Specialist", "Coordinator",
        "Administrator", "Associate", "Principal", "Staff", "Distinguished",
        "Infrastructure", "Engineering", "Operations", "Product", "Data",
        "Science", "Technology", "Marketing", "Sales", "Finance", "Procurement",
        "Security", "Platform", "Software", "Hardware", "Vice", "Executive",
        "Member", "Board", "Advisor", "Advisory",
        // Common fact/action words that may appear capitalized
        "Entry", "Phase", "Role", "Position", "Based", "Lives", "Located",
        "Moved", "Joined", "Left", "Promoted", "Hired", "Resigned", "Retired",
        "Collaborates", "Coordinates", "Reports", "Works", "Manages",
        // Common organizational suffixes (would match across different entities)
        "Corp", "Inc.", "Group", "Company", "International", "Global",
        "National", "Services", "Solutions", "Systems", "Consulting",
    ];

    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            w.len() >= 4
                && w.chars().any(|c| c.is_uppercase())
                && w.chars().any(|c| c.is_alphabetic())
                && !STOP_WORDS.contains(w)
        })
        .map(|w| w.to_lowercase())
        .collect()
}

/// Detect duplicate fact entries within a document.
/// Returns pairs of (line1, line2, fact_text) for facts with identical text.
pub fn find_duplicate_entries(content: &str) -> Vec<(usize, usize, String)> {
    let facts = collect_facts_with_ranges(content, true);
    let mut duplicates = Vec::new();
    for i in 0..facts.len() {
        for j in (i + 1)..facts.len() {
            if facts[i].text.to_lowercase() == facts[j].text.to_lowercase() {
                duplicates.push((
                    facts[i].line_number,
                    facts[j].line_number,
                    facts[i].text.clone(),
                ));
            }
        }
    }
    duplicates
}

/// Generate duplicate questions for fact entries that appear multiple times
/// within the same document.
pub fn generate_duplicate_entry_questions(content: &str) -> Vec<ReviewQuestion> {
    find_duplicate_entries(content)
        .into_iter()
        .map(|(line1, line2, text)| {
            ReviewQuestion::new(
                QuestionType::Duplicate,
                Some(line2),
                format!(
                    "Duplicate entry: \"{}\" appears on lines {} and {} - remove the duplicate",
                    text, line1, line2
                ),
            )
        })
        .collect()
}

/// Check if two texts mention different proper names (2+ consecutive capitalized words).
/// If both contain proper names and they differ, the facts are about different entities.
fn contains_different_proper_names(text1: &str, text2: &str) -> bool {
    let names1 = extract_proper_names(text1);
    let names2 = extract_proper_names(text2);
    if names1.is_empty() || names2.is_empty() {
        return false;
    }
    // If no names overlap, they're about different people
    !names1.iter().any(|n| names2.contains(n))
}

/// Extract proper names (2+ consecutive capitalized words) from text.
/// Filters out common capitalized English words (titles, roles, descriptors)
/// to avoid matching phrases like "Senior Director" as entity names.
fn extract_proper_names(text: &str) -> Vec<String> {
    let common_words: &[&str] = &[
        "Senior",
        "Junior",
        "Lead",
        "Chief",
        "Head",
        "Director",
        "Manager",
        "Engineer",
        "Developer",
        "Analyst",
        "Consultant",
        "Founder",
        "President",
        "Officer",
        "Architect",
        "Designer",
        "Specialist",
        "Coordinator",
        "Administrator",
        "Associate",
        "Principal",
        "Staff",
        "Distinguished",
        "Infrastructure",
        "Engineering",
        "Operations",
        "Product",
        "Data",
        "Science",
        "Technology",
        "Marketing",
        "Sales",
        "Finance",
        "Procurement",
        "Security",
        "Platform",
        "Software",
        "Hardware",
        "Collaborates",
        "Coordinates",
        "Reports",
        "Works",
        "Manages",
    ];

    let mut names = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for word in text.split_whitespace() {
        let clean = word.trim_end_matches(|c: char| !c.is_alphanumeric());
        let first_char = clean.chars().next().unwrap_or('a');
        let is_capitalized = first_char.is_uppercase()
            && clean.len() > 1
            && !clean
                .chars()
                .all(|c| c.is_uppercase() || !c.is_alphabetic());

        if is_capitalized && !common_words.contains(&clean) {
            current.push(clean);
        } else {
            if current.len() >= 2 {
                names.push(current.join(" "));
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        names.push(current.join(" "));
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_conflict_questions_no_overlap() {
        let content = "# Person\n\n- CTO at Acme @t[2018..2020]\n- VP at BigCo @t[2021..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_conflict_questions_overlapping() {
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Conflict);
        assert!(questions[0].description.contains("overlaps"));
        assert!(questions[0].description.contains("simultaneously"));
        assert_eq!(questions[0].line_ref, Some(3));
        assert!(questions[0].description.contains("(line:4)"));
        assert!(questions[0].description.contains("[pattern:"));
    }

    #[test]
    fn test_generate_conflict_questions_ongoing_overlap() {
        let content = "# Person\n\n- Engineer at Acme @t[2020..]\n- Developer at BigCo @t[2022..]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Conflict);
    }

    #[test]
    fn test_generate_conflict_questions_no_temporal_tags() {
        let content = "# Person\n\n- CTO at Acme\n- VP at BigCo";
        let questions = generate_conflict_questions(content);
        // No temporal tags = no overlap detection possible
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_conflict_questions_different_sections() {
        // Facts in different sections should not conflict
        let content =
            "# Entity\n\n## Section A\n- Fact one @t[2020..2023]\n\n## Section B\n- Fact two @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_conflict_questions_overlapping_locations() {
        let content =
            "# Person\n\n- Lives in NYC @t[2020..2023]\n- Based in San Francisco @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Conflict);
    }

    #[test]
    fn test_reviewed_markers_suppress_conflicts() {
        // Both reviewed
        let c1 = "# Person\n\n- CTO at Acme @t[2020..2023] <!-- reviewed:2026-01-15 -->\n- CEO at Acme @t[2022..2024] <!-- reviewed:2026-01-15 -->";
        assert!(generate_conflict_questions(c1).is_empty());
        // With explanation text
        let c2 = "# Person\n\n## Career History\n\n\
            - CTO at TechCo @t[2020..] <!-- reviewed:2026-02-21 Not a conflict: concurrent advisory role -->\n\
            - Board Member at StartupX @t[2021..] <!-- reviewed:2026-02-21 Not a conflict: board role alongside primary employment -->";
        assert!(generate_conflict_questions(c2).is_empty());
        // With separate comment
        let c3 = "# Person\n\n## Career History\n\n\
            - CTO at TechCo @t[2020..] <!-- reviewed:2026-02-21 --> <!-- advisory role -->\n\
            - Advisor at OtherCo @t[2021..] <!-- reviewed:2026-02-21 --> <!-- advisory role -->";
        assert!(generate_conflict_questions(c3).is_empty());
        // Old reviewed marker still suppresses
        let c4 = "# Person\n\n- CTO at Acme @t[2020..2023] <!-- reviewed:2020-01-01 -->\n- CEO at BigCo @t[2022..2024] <!-- reviewed:2020-01-01 -->";
        assert!(generate_conflict_questions(c4).is_empty());
        // One reviewed, one not — still suppresses
        let c5 = "# Person\n\n- CTO at Acme @t[2020..2023] <!-- reviewed:2020-01-01 -->\n- CEO at BigCo @t[2022..2024]";
        assert!(generate_conflict_questions(c5).is_empty());
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap("2020", "2023", "2022", "2024"));
        assert!(!ranges_overlap("2018", "2020", "2022", "2024"));
        assert!(!ranges_overlap("2020", "2021", "2022", "2023")); // adjacent
        assert!(ranges_overlap("2020", "2025", "2022", "2023")); // contained
    }

    #[test]
    fn test_facts_may_conflict() {
        // Plain facts may conflict
        assert!(facts_may_conflict("CTO at Acme Corp", "CEO at BigCo"));
        assert!(facts_may_conflict("Lives in NYC", "Based in SF"));
        // Same proper name — may conflict
        assert!(facts_may_conflict("Aaron Stranahan, VP Infrastructure", "Aaron Stranahan, Director Engineering"));
        // Two roster entries with [[links]] should NOT conflict
        assert!(!facts_may_conflict("[[abc123]] Jason King - Senior Director", "[[def456]] Anurag Voleti - VP Data Science"));
        // One with [[link]], one without — NOT conflict
        assert!(!facts_may_conflict("[[abc123]] Jason King - Senior Director", "VP Data Science at BigCo"));
        // Different proper names — not a conflict
        assert!(!facts_may_conflict(
            "Collaborates with Aaron Stranahan, VP Infrastructure",
            "Coordinates with Wes Thompson, Manager IT Procurement"
        ));
    }

    #[test]
    fn test_extract_proper_names() {
        assert_eq!(
            extract_proper_names("Collaborates with Aaron Stranahan, VP Infrastructure"),
            vec!["Aaron Stranahan"]
        );
        assert_eq!(
            extract_proper_names("Reports to Wes Thompson"),
            vec!["Wes Thompson"]
        );
        // ALL-CAPS like VP, CEO should not count
        assert!(extract_proper_names("VP of Engineering").is_empty());
    }

    #[test]
    fn test_cross_section_no_conflict() {
        let content = "# Person\n\n## Key Relationships\n- Collaborates with director at Acme @t[2022..]\n\n## Current Responsibilities\n- Manager of engineering team @t[2022..]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_same_section_still_conflicts() {
        let content = "# Person\n\n## Career History\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_point_in_time_vs_range_conflict() {
        let content = "# Person\n\n- CTO at Acme @t[=2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("overlaps"));
    }

    #[test]
    fn test_two_point_in_time_same_date_conflict() {
        let content = "# Person\n\n- CTO at Acme @t[=2023]\n- CEO at BigCo @t[=2023]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_two_point_in_time_different_dates_no_conflict() {
        let content = "# Person\n\n- CTO at Acme @t[=2020]\n- CEO at BigCo @t[=2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_last_seen_vs_range_conflict() {
        let content = "# Person\n\n- CTO at Acme @t[~2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_point_in_time_outside_range_no_conflict() {
        let content = "# Person\n\n- CTO at Acme @t[=2018]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty());
    }

    // --- duplicate entry detection tests ---

    #[test]
    fn test_cross_section_duplicate_no_conflict() {
        // Same fact in different sections — no conflict (different sections)
        let content = "# Entity\n\n## Section A\n- CTO at Acme @t[2020..2023]\n\n## Section B\n- CTO at Acme @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty(), "Duplicate entries in different sections should not generate conflict questions");
    }

    #[test]
    fn test_overlapping_entries_still_conflicts() {
        let content = "# Entity\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        assert_eq!(generate_conflict_questions(content).len(), 1);
        // Significant overlap
        let content2 = "# Entity\n\n- Entry A @t[2018..2022]\n- Entry B @t[2020..2023]";
        assert_eq!(generate_conflict_questions(content2).len(), 1);
        // Multi-year overlap at different entities
        let content3 = "# Entity\n\n\
            - Entry A @t[2018-01..2023-06]\n\
            - Entry B @t[2021-03..2024-12]";
        assert_eq!(generate_conflict_questions(content3).len(), 1);
    }

    // --- duplicate entry question tests ---

    #[test]
    fn test_generate_duplicate_entry_questions() {
        let content = "# Entity\n\n## Section A\n- CTO at Acme @t[2020..2023]\n\n## Section B\n- CTO at Acme @t[2020..2023]";
        let questions = generate_duplicate_entry_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Duplicate);
        assert!(questions[0].description.contains("Duplicate entry"));
    }

    #[test]
    fn test_no_duplicate_for_different_text() {
        let content = "# Entity\n\n- Entry A @t[2018..2020]\n- Entry B @t[2020..2023]";
        let questions = generate_duplicate_entry_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_dates_within_one_month() {
        assert!(dates_within_one_month("2020-01-01", "2020-01-01"));
        assert!(dates_within_one_month("2020-01-01", "2020-02-01"));
        assert!(!dates_within_one_month("2020-01-01", "2020-03-01"));
    }

    // --- boundary-month sequential suppression tests ---

    #[test]
    fn test_boundary_month_sequential_suppressed() {
        // All boundary-month/year patterns should suppress conflicts
        let cases = [
            // Exact boundary: A ends 2023-09, B starts 2023-09
            "# Entity\n\n- Entry A @t[2020..2023-09]\n- Entry B @t[2023-09..2024]",
            // Year granularity boundary
            "# Entity\n\n- Entry A @t[2018..2020]\n- Entry B @t[2020..2023]",
            // Off-by-one month
            "# Entity\n\n- Entry A @t[2020-01..2022-03]\n- Entry B @t[2022-02..2024-06]",
            // Multiple sequential entries
            "# Entity\n\n## History\n\
                - Phase 1 @t[2015-06..2018-10]\n\
                - Phase 2 @t[2018-09..2020-04]\n\
                - Phase 3 @t[2020-03..2022-07]\n\
                - Phase 4 @t[2022-06..2024-02]\n\
                - Phase 5 @t[2024-01..]",
            // Exact match promotion (same end/start month)
            "# Person\n\n## Career History\n\
                - Enterprise AM at Company @t[2020-06..2023-04]\n\
                - Principal AM at Company @t[2023-04..2025-06]",
            // Boundary-year with month precision
            "# Entity\n\n\
                - Entry A @t[2014-03..2018-06]\n\
                - Entry B @t[2018-01..2023-09]",
            // Three sequential with boundary-year overlaps
            "# Entity\n\n## History\n\
                - Phase 1 @t[2003-06..2005-09]\n\
                - Phase 2 @t[2005-01..2012-03]\n\
                - Phase 3 @t[2012-06..2018-11]",
            // With accumulated footnotes
            "# John Boyd\n\n## Career History\n\
                - Manager at Acme Corp @t[2021-02..2023-02] [^9] [^10] [^11]\n\
                - Director at Acme Corp @t[2023-02..2026-01] [^12] [^13] [^14]",
        ];
        for (i, content) in cases.iter().enumerate() {
            let questions = generate_conflict_questions(content);
            assert!(questions.is_empty(), "Case {} should not generate conflict, got: {:?}", i, questions.iter().map(|q| &q.description).collect::<Vec<_>>());
        }
    }

    #[test]
    fn test_is_boundary_month_sequential() {
        // Exact boundary
        assert!(is_boundary_month_sequential("2020", "2023-09", "2023-09", "2024"));
        assert!(is_boundary_month_sequential("2023-09", "2024", "2020", "2023-09"));
        // Off-by-one month
        assert!(is_boundary_month_sequential("2020-01", "2022-03", "2022-02", "2024"));
        assert!(is_boundary_month_sequential("2022-02", "2024", "2020-01", "2022-03"));
        // Boundary-year with month precision
        assert!(is_boundary_month_sequential("2014-03", "2018-06", "2018-01", "2023-09"));
        assert!(is_boundary_month_sequential("2005-03", "2012-09", "2012-01", "2018-06"));
        // Point-in-time should NOT be sequential
        assert!(!is_boundary_month_sequential("2023", "2023", "2023", "2023"));
        // Real overlap should not match
        assert!(!is_boundary_month_sequential("2020", "2023", "2022", "2024"));
        assert!(!is_boundary_month_sequential("2018-01", "2023-06", "2021-03", "2024-12"));
    }

    #[test]
    fn test_filter_sequential_conflicts() {
        // Removes boundary-month conflicts
        let content = "# Entity\n\n\
            - Entry A @t[2011-03..2016-11]\n\
            - Entry B @t[2016-11..2020-11]\n\
            - Entry C @t[2020-11..2024-01]";
        let mut questions = vec![
            ReviewQuestion::new(QuestionType::Conflict, Some(3), "Cross-check: Entry A — overlapping entry".to_string()),
            ReviewQuestion::new(QuestionType::Conflict, Some(4), "Cross-check: Entry B — overlapping entry".to_string()),
            ReviewQuestion::new(QuestionType::Stale, Some(3), "Stale fact".to_string()),
        ];
        filter_sequential_conflicts(content, &mut questions);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Stale);

        // Keeps real overlap conflicts
        let content2 = "# Entity\n\n\
            - Entry A @t[2020..2023]\n\
            - Entry B @t[2022..2024]";
        let mut questions2 = vec![ReviewQuestion::new(QuestionType::Conflict, Some(3), "Cross-check: overlapping entries".to_string())];
        filter_sequential_conflicts(content2, &mut questions2);
        assert_eq!(questions2.len(), 1);

        // Works with reviewed facts
        let today = chrono::Local::now().format("%Y-%m-%d");
        let content3 = format!(
            "# Person\n\n\
            - Sr. Manager at Acme @t[2018-06..2020-03] <!-- reviewed:{today} -->\n\
            - Director at Acme @t[2020-03..2022-11] <!-- reviewed:{today} -->\n\
            - VP at BigCo @t[2022-11..]"
        );
        let mut questions3 = vec![
            ReviewQuestion::new(QuestionType::Conflict, Some(3), "Cross-check: Sr. Manager overlaps Director".to_string()),
            ReviewQuestion::new(QuestionType::Conflict, Some(4), "Cross-check: Director overlaps VP".to_string()),
        ];
        filter_sequential_conflicts(&content3, &mut questions3);
        assert!(questions3.is_empty());

        // Catches shared-entity patterns
        let content4 = "# Person\n\n## Career History\n\
            - Manager at Tivity Health @t[2015..2019]\n\
            - Director at Tivity Health @t[2018..2023]";
        let mut questions4 = vec![ReviewQuestion::new(QuestionType::Conflict, Some(4), "Cross-check: overlapping entries".to_string())];
        filter_sequential_conflicts(content4, &mut questions4);
        assert!(questions4.is_empty());

        // Catches camelCase entity patterns
        let content5 = "# Person\n\n## Career History\n\
            - VP at axialHealthcare @t[2016-01..2018-11]\n\
            - SVP at axialHealthcare @t[2018-11..2022-03]";
        let mut questions5 = vec![ReviewQuestion::new(QuestionType::Conflict, Some(4), "Cross-check: overlapping entries".to_string())];
        filter_sequential_conflicts(content5, &mut questions5);
        assert!(questions5.is_empty());
    }

    #[test]
    fn test_conflict_description_includes_second_line() {
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("(line:4)"));
    }

    #[test]
    fn test_sequential_marker_suppresses_conflict() {
        // Plain sequential marker
        let c1 = "# Person\n\n## Career History\n\
            - CTO at Acme @t[2020..2023] <!-- sequential -->\n\
            - CEO at Acme @t[2022..2024] <!-- sequential -->";
        assert!(generate_conflict_questions(c1).is_empty());
        // With explanation
        let c2 = "# Person\n\n## Career History\n\
            - Manager at Acme @t[2020..2023] <!-- sequential: promotion -->\n\
            - Director at Acme @t[2022..2024] <!-- sequential: promotion -->";
        assert!(generate_conflict_questions(c2).is_empty());
    }

    // --- shared-entity sequential (promotion pattern) tests ---

    #[test]
    fn test_shared_entity_sequential_suppresses_conflict() {
        // Same company, different start dates = promotion pattern
        for content in [
            "# Person\n\n## Career History\n\
                - Manager at Tivity Health @t[2015..2019]\n\
                - Director at Tivity Health @t[2018..2023]",
            // Multi-year overlap at same entity
            "# Person\n\n## Career History\n\
                - VP at Acme Corp @t[2015..2020]\n\
                - SVP at Acme Corp @t[2018..2023]",
            // camelCase company name
            "# Person\n\n## Career History\n\
                - VP at axialHealthcare @t[2016-01..2018-11]\n\
                - SVP at axialHealthcare @t[2018-11..2022-03]",
            // camelCase multi-year overlap
            "# Person\n\n## Career History\n\
                - VP at axialHealthcare @t[2015..2019]\n\
                - SVP at axialHealthcare @t[2018..2022]",
            // Single-word company
            "# Person\n\n## Career History\n\
                - Engineer at Google @t[2018..2021]\n\
                - Manager at Google @t[2021..2024]",
        ] {
            let questions = generate_conflict_questions(content);
            assert!(questions.is_empty(), "Should not conflict for: {}", content);
        }
    }

    #[test]
    fn test_different_entities_still_conflicts() {
        // Different entities with overlap should still conflict
        for content in [
            "# Person\n\n## Career History\n\
                - VP at Acme @t[2020..2023]\n\
                - CTO at Globex @t[2022..2024]",
            "# Person\n\n## Career History\n\
                - Engineer at Google @t[2020..2023]\n\
                - Manager at Amazon @t[2022..2024]",
        ] {
            assert_eq!(generate_conflict_questions(content).len(), 1, "Should conflict for: {}", content);
        }
    }

    #[test]
    fn test_is_shared_entity_sequential() {
        // Multi-word proper name
        assert!(is_shared_entity_sequential("Manager at Tivity Health", "Director at Tivity Health", "2015", "2018"));
        // camelCase company name
        assert!(is_shared_entity_sequential("VP at axialHealthcare", "SVP at axialHealthcare", "2016", "2018"));
        // Single-word company
        assert!(is_shared_entity_sequential("Engineer at Google", "Manager at Google", "2018", "2021"));
        // Different entities
        assert!(!is_shared_entity_sequential("VP at Acme Corp", "Director at Beta Inc", "2020", "2022"));
        assert!(!is_shared_entity_sequential("VP at Acme", "CTO at Globex", "2020", "2022"));
        // Same start date = not sequential
        assert!(!is_shared_entity_sequential("VP at Acme Corp", "Director at Acme Corp", "2020", "2020"));
        // No proper names
        assert!(!is_shared_entity_sequential("Entry A", "Entry B", "2020", "2022"));
    }

    // --- ConflictPattern classification tests ---

    #[test]
    fn test_classify_parallel_overlap_different_entities() {
        // Two facts at different entities with multi-year overlap → parallel_overlap
        let p = classify_conflict_pattern(
            "CTO at Acme", "Board Member at StartupX",
            "2018", "2023", "2020", "9999-12-31",
        );
        assert_eq!(p, ConflictPattern::ParallelOverlap);
    }

    #[test]
    fn test_classify_same_entity_transition() {
        // Two facts at the same entity → same_entity_transition
        let p = classify_conflict_pattern(
            "VP Engineering at Acme", "CTO at Acme",
            "2018", "2023", "2022", "9999-12-31",
        );
        assert_eq!(p, ConflictPattern::SameEntityTransition);
    }

    #[test]
    fn test_classify_date_imprecision_small_overlap() {
        // Small overlap (3 months) relative to multi-year spans → date_imprecision
        let p = classify_conflict_pattern(
            "Lives in NYC", "Based in San Francisco",
            "2018", "2022-03", "2022-01", "2025",
        );
        assert_eq!(p, ConflictPattern::DateImprecision);
    }

    #[test]
    fn test_classify_unknown_short_spans() {
        // Short spans with moderate overlap, no entity match → unknown
        let p = classify_conflict_pattern(
            "Consulting gig", "Freelance project",
            "2023-01", "2023-08", "2023-04", "2023-10",
        );
        assert_eq!(p, ConflictPattern::Unknown);
    }

    // --- attribute key extraction tests ---

    #[test]
    fn test_extract_attribute_key() {
        // Standard key-value
        assert_eq!(extract_attribute_key("Email: foo@bar.com"), Some("Email"));
        assert_eq!(extract_attribute_key("Phone: 555-1234"), Some("Phone"));
        // Bold key
        assert_eq!(extract_attribute_key("**Email**: foo@bar.com"), Some("Email"));
        assert_eq!(extract_attribute_key("**Hire Date**: 2020-01-15"), Some("Hire Date"));
        // Multi-word key (up to 4 words)
        assert_eq!(extract_attribute_key("Spore Print Color: white"), Some("Spore Print Color"));
        // Too many words → not a key
        assert_eq!(extract_attribute_key("This is a long sentence fragment: value"), None);
        // No colon → not key-value
        assert_eq!(extract_attribute_key("CTO at Acme Corp"), None);
        // URL-like colon (no space after) → not key-value
        assert_eq!(extract_attribute_key("Website http://example.com"), None);
        // Empty key
        assert_eq!(extract_attribute_key(": value"), None);
    }

    #[test]
    fn test_have_different_attribute_keys() {
        // Different keys → true
        assert!(have_different_attribute_keys("Email: foo@bar.com", "Phone: 555-1234"));
        assert!(have_different_attribute_keys("**Email**: foo", "**Slack**: @foo"));
        assert!(have_different_attribute_keys("Habitat: forest", "Classification: Agaricales"));
        // Same key → false
        assert!(!have_different_attribute_keys("Role: CTO", "Role: CEO"));
        assert!(!have_different_attribute_keys("**Role**: CTO", "Role: CEO"));
        // Case-insensitive
        assert!(!have_different_attribute_keys("email: foo", "Email: bar"));
        // One or both not key-value → false (can't determine)
        assert!(!have_different_attribute_keys("CTO at Acme", "Email: foo"));
        assert!(!have_different_attribute_keys("CTO at Acme", "CEO at BigCo"));
    }

    #[test]
    fn test_attribute_key_suppresses_conflict() {
        // Different key-value attributes in same section should not conflict
        let content = "# Entity\n\n\
            - **Email**: foo@bar.com @t[2020..]\n\
            - **Slack**: @foo @t[2020..]";
        assert!(generate_conflict_questions(content).is_empty());

        // Mixed bold/plain keys
        let content2 = "# Entity\n\n\
            - Hire Date: 2020-01-15 @t[=2020-01]\n\
            - Manager: Jane Smith @t[2020..]\n\
            - Department: Engineering @t[2020..]";
        assert!(generate_conflict_questions(content2).is_empty());

        // Non-people domain: botany
        let content3 = "# Species\n\n\
            - Habitat: temperate forests @t[2020..]\n\
            - Classification: Agaricales @t[2020..]\n\
            - Spore Color: white @t[~2023]";
        assert!(generate_conflict_questions(content3).is_empty());
    }

    #[test]
    fn test_same_attribute_key_still_conflicts() {
        // Same key with overlapping dates → real conflict
        // Use lowercase key to avoid shared-entity-sequential suppression
        let content = "# Entity\n\n\
            - role: alpha @t[2020..2023]\n\
            - role: beta @t[2022..2024]";
        assert_eq!(generate_conflict_questions(content).len(), 1);
    }

    #[test]
    fn test_non_kv_facts_still_conflict() {
        // Facts without key-value structure should still be checked
        let content = "# Entity\n\n\
            - CTO at Acme @t[2020..2023]\n\
            - CEO at BigCo @t[2022..2024]";
        assert_eq!(generate_conflict_questions(content).len(), 1);
    }

    #[test]
    fn test_months_between_basic() {
        assert_eq!(months_between("2020-01-01", "2020-06-01"), 5);
        assert_eq!(months_between("2020-01-01", "2021-01-01"), 12);
        assert_eq!(months_between("2020-06-01", "2020-01-01"), -5);
    }

    #[test]
    fn test_conflict_pattern_tags() {
        assert_eq!(ConflictPattern::ParallelOverlap.tag(), "parallel_overlap");
        assert_eq!(ConflictPattern::SameEntityTransition.tag(), "same_entity_transition");
        assert_eq!(ConflictPattern::DateImprecision.tag(), "date_imprecision");
        assert_eq!(ConflictPattern::Unknown.tag(), "unknown");
        for p in [ConflictPattern::ParallelOverlap, ConflictPattern::SameEntityTransition, ConflictPattern::DateImprecision, ConflictPattern::Unknown] {
            assert!(!p.hint().is_empty());
        }
    }

}
