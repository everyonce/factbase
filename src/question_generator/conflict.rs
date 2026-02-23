//! Conflict question generation.
//!
//! Generates `@q[conflict]` questions for overlapping date ranges
//! or contradictory facts.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{
    extract_reviewed_date, normalize_date_for_comparison, FACT_LINE_REGEX, MANUAL_LINK_REGEX,
};
use crate::processor::parse_temporal_tags;

use super::extract_fact_text;

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

/// Generate conflict questions for a document.
///
/// Detects:
/// 1. Overlapping date ranges for facts that appear mutually exclusive (e.g., two jobs)
/// 2. Same attribute with different values on different lines
///
/// Returns a list of `ReviewQuestion` with `question_type = Conflict`.
pub fn generate_conflict_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();

    // Collect facts with temporal ranges
    let facts = collect_facts_with_ranges(content);

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
fn collect_facts_with_ranges(content: &str) -> Vec<FactWithRange> {
    let mut facts = Vec::new();
    let tags = parse_temporal_tags(content);
    let mut current_section: Option<String> = None;
    let today = chrono::Local::now().date_naive();
    const REVIEWED_SKIP_DAYS: i64 = 180;

    // Stop before the review queue section
    let end = content
        .find(crate::patterns::REVIEW_QUEUE_MARKER)
        .unwrap_or(content.len());

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

        // Skip facts with a recent reviewed marker
        if extract_reviewed_date(line)
            .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
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

/// Check if two facts have a conflict (overlapping ranges for similar facts).
fn check_fact_conflict(fact1: &FactWithRange, fact2: &FactWithRange) -> Option<ReviewQuestion> {
    // Both facts need temporal info to detect overlap
    let start1 = fact1.start_date.as_deref()?;
    let start2 = fact2.start_date.as_deref()?;

    // Skip cross-section comparisons unless both are career-type sections
    if fact1.section != fact2.section {
        let career_keywords = ["career", "history", "roles", "experience", "employment"];
        let both_career = [&fact1.section, &fact2.section].iter().all(|s| {
            s.as_ref().is_some_and(|h| {
                let lower = h.to_lowercase();
                career_keywords.iter().any(|k| lower.contains(k))
            })
        });
        if !both_career {
            return None;
        }
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

    // Check if facts look like they could be mutually exclusive
    // (e.g., both are job titles, both are locations, etc.)
    if !facts_may_conflict(&fact1.text, &fact2.text) {
        return None;
    }

    // Suppress conflicts for same-company roles: duplicate entries across sections
    // or promotion sequences with boundary-month overlaps
    if is_same_company_role(&fact1.text, &fact2.text, start1, end1, start2, end2) {
        return None;
    }

    // Suppress boundary-month overlaps in sequential career roles (any company).
    // LinkedIn always reports the transition month in both roles, e.g.
    // Role A ends 2023-09, Role B starts 2023-09.  This is not a real conflict.
    if is_boundary_month_sequential(start1, end1, start2, end2) {
        return None;
    }

    // Generate conflict question
    let description = format!(
        "\"{}\" @t[{}..{}] overlaps with \"{}\" @t[{}..{}] - were both true simultaneously?",
        fact1.text,
        start1,
        if fact1.is_ongoing { "" } else { end1 },
        fact2.text,
        start2,
        if fact2.is_ongoing { "" } else { end2 }
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
    s1 <= e2 && s2 <= e1
}

/// Heuristic to check if two facts might be mutually exclusive.
/// Returns true if both facts appear to be the same type of thing
/// (e.g., both are job roles, both are locations).
fn facts_may_conflict(text1: &str, text2: &str) -> bool {
    // Roster lines with cross-references are distinct entries, not conflicts
    if MANUAL_LINK_REGEX.is_match(text1) || MANUAL_LINK_REGEX.is_match(text2) {
        return false;
    }

    // If both facts mention different proper names, they describe
    // different people/entities and aren't mutually exclusive
    if contains_different_proper_names(text1, text2) {
        return false;
    }

    let t1 = text1.to_lowercase();
    let t2 = text2.to_lowercase();

    // Job role indicators
    let job_keywords = [
        " at ",
        "ceo",
        "cto",
        "cfo",
        "coo",
        "vp ",
        "director",
        "manager",
        "engineer",
        "developer",
        "analyst",
        "consultant",
        "founder",
        "president",
        "head of",
    ];

    let is_job1 = job_keywords.iter().any(|k| t1.contains(k));
    let is_job2 = job_keywords.iter().any(|k| t2.contains(k));

    // Both are jobs - potential conflict
    if is_job1 && is_job2 {
        return true;
    }

    // Location indicators
    let location_keywords = [
        "lives in",
        "based in",
        "located in",
        "moved to",
        "relocated",
    ];

    let is_location1 = location_keywords.iter().any(|k| t1.contains(k));
    let is_location2 = location_keywords.iter().any(|k| t2.contains(k));

    // Both are locations - potential conflict
    if is_location1 && is_location2 {
        return true;
    }

    false
}

/// Extract company name from a role fact like "CTO at Acme Corp" → "acme corp".
/// Returns None if no " at " pattern found.
fn extract_company(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at_pos = lower.find(" at ")?;
    let company = lower[at_pos + 4..].trim();
    // Strip trailing temporal/source markers
    let company = company
        .split(|c: char| c == '@' || c == '[')
        .next()
        .unwrap_or(company)
        .trim();
    if company.is_empty() {
        None
    } else {
        Some(company.to_string())
    }
}

/// Check if two facts describe roles at the same company.
/// Suppresses conflicts for:
/// 1. Duplicate entries (same role + same company)
/// 2. Promotion sequences (different role, same company, sequential dates)
fn is_same_company_role(
    text1: &str,
    text2: &str,
    start1: &str,
    end1: &str,
    start2: &str,
    end2: &str,
) -> bool {
    let Some(company1) = extract_company(text1) else {
        return false;
    };
    let Some(company2) = extract_company(text2) else {
        return false;
    };
    if company1 != company2 {
        return false;
    }
    // Same company — check if duplicate entry (same text) or promotion (sequential dates)
    let t1 = text1.to_lowercase();
    let t2 = text2.to_lowercase();
    if t1 == t2 {
        return true; // exact duplicate
    }
    // Promotion: one role ends when the other starts (boundary overlap within 1 month)
    let e1_norm = normalize_date_for_comparison(end1);
    let s2_norm = normalize_date_for_comparison(start2);
    let e2_norm = normalize_date_for_comparison(end2);
    let s1_norm = normalize_date_for_comparison(start1);
    // Check if end of one is near start of the other
    dates_within_one_month(&e1_norm, &s2_norm) || dates_within_one_month(&e2_norm, &s1_norm)
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
        let parts: Vec<&str> = d.split('-').collect();
        if parts.len() >= 2 {
            Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
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
/// overlap (end of one equals start of the other).  LinkedIn always reports the
/// transition month in both roles, so `@t[..2023-09]` + `@t[2023-09..]` is not
/// a real conflict — it's a normal career transition.
fn is_boundary_month_sequential(start1: &str, end1: &str, start2: &str, end2: &str) -> bool {
    let e1 = normalize_date_for_comparison(end1);
    let s2 = normalize_date_for_comparison(start2);
    let e2 = normalize_date_for_comparison(end2);
    let s1 = normalize_date_for_comparison(start1);
    // Sequential: end of one matches start of the other (exactly, not "within 1 month")
    // Use strict less-than to exclude point-in-time facts where start == end == boundary.
    (e1 == s2 && s1 < s2) || (e2 == s1 && s2 < s1)
}

/// Detect duplicate role entries within a document.
/// Returns pairs of (line1, line2, fact_text) for facts that appear to be
/// the same role at the same company.
pub fn find_duplicate_roles(content: &str) -> Vec<(usize, usize, String)> {
    let facts = collect_facts_with_ranges(content);
    let mut duplicates = Vec::new();
    for i in 0..facts.len() {
        for j in (i + 1)..facts.len() {
            let Some(c1) = extract_company(&facts[i].text) else {
                continue;
            };
            let Some(c2) = extract_company(&facts[j].text) else {
                continue;
            };
            if c1 == c2 && facts[i].text.to_lowercase() == facts[j].text.to_lowercase() {
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

/// Generate duplicate questions for role entries that appear multiple times
/// within the same document.
pub fn generate_duplicate_role_questions(content: &str) -> Vec<ReviewQuestion> {
    find_duplicate_roles(content)
        .into_iter()
        .map(|(line1, line2, text)| {
            ReviewQuestion::new(
                QuestionType::Duplicate,
                Some(line2),
                format!(
                    "Duplicate role entry: \"{}\" appears on lines {} and {} - remove the duplicate",
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
/// Filters out job title words to avoid matching "Senior Director" as a name.
fn extract_proper_names(text: &str) -> Vec<String> {
    let title_words: &[&str] = &[
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

        if is_capitalized && !title_words.contains(&clean) {
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
    fn test_generate_conflict_questions_overlapping_jobs() {
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Conflict);
        assert!(questions[0].description.contains("overlaps"));
        assert!(questions[0].description.contains("simultaneously"));
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
    fn test_generate_conflict_questions_different_types() {
        // Job and location shouldn't conflict
        let content =
            "# Person\n\n- CTO at Acme @t[2020..2023]\n- Lives in San Francisco @t[2020..2023]";
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
    fn test_generate_conflict_questions_line_ref() {
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at Acme @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1);
        // Line ref should point to the first fact in the conflict
        assert_eq!(questions[0].line_ref, Some(3));
    }

    #[test]
    fn test_reviewed_facts_skip_conflict_detection() {
        // Both facts have recent reviewed markers — should generate no conflicts
        let content = "# Person\n\n- CTO at Acme @t[2020..2023] <!-- reviewed:2026-01-15 -->\n- CEO at Acme @t[2022..2024] <!-- reviewed:2026-01-15 -->";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 0);
    }

    #[test]
    fn test_facts_may_conflict_jobs() {
        assert!(facts_may_conflict("CTO at Acme Corp", "CEO at BigCo"));
        assert!(facts_may_conflict("Software Engineer", "Senior Developer"));
        assert!(facts_may_conflict(
            "VP of Engineering",
            "Director of Product"
        ));
    }

    #[test]
    fn test_facts_may_conflict_locations() {
        assert!(facts_may_conflict("Lives in NYC", "Based in SF"));
        assert!(facts_may_conflict("Located in London", "Moved to Paris"));
    }

    #[test]
    fn test_facts_may_conflict_different_types() {
        assert!(!facts_may_conflict("CTO at Acme", "Lives in NYC"));
        assert!(!facts_may_conflict(
            "MBA from Stanford",
            "Engineer at Google"
        ));
    }

    #[test]
    fn test_ranges_overlap_basic() {
        assert!(ranges_overlap("2020", "2023", "2022", "2024"));
        assert!(!ranges_overlap("2018", "2020", "2022", "2024"));
    }

    #[test]
    fn test_ranges_overlap_adjacent() {
        // Adjacent ranges don't overlap
        assert!(!ranges_overlap("2020", "2021", "2022", "2023"));
    }

    #[test]
    fn test_ranges_overlap_contained() {
        // One range fully contained in another
        assert!(ranges_overlap("2020", "2025", "2022", "2023"));
    }

    #[test]
    fn test_facts_may_conflict_roster_with_links_no_conflict() {
        // Two roster entries with [[links]] should NOT conflict
        assert!(!facts_may_conflict(
            "[[abc123]] Jason King - Senior Director",
            "[[def456]] Anurag Voleti - VP Data Science"
        ));
    }

    #[test]
    fn test_facts_may_conflict_jobs_without_links() {
        // Two job facts without [[links]] should still conflict
        assert!(facts_may_conflict(
            "Senior Director at Acme",
            "VP Data Science at BigCo"
        ));
    }

    #[test]
    fn test_facts_may_conflict_one_link_one_plain() {
        // One fact with [[link]] and one without should NOT conflict
        assert!(!facts_may_conflict(
            "[[abc123]] Jason King - Senior Director",
            "VP Data Science at BigCo"
        ));
    }

    #[test]
    fn test_facts_may_conflict_different_proper_names() {
        // Different people mentioned — not a conflict
        assert!(!facts_may_conflict(
            "Collaborates with Aaron Stranahan, VP Infrastructure",
            "Coordinates with Wes Thompson, Manager IT Procurement"
        ));
    }

    #[test]
    fn test_facts_may_conflict_same_proper_name() {
        // Same person mentioned — could still conflict
        assert!(facts_may_conflict(
            "Aaron Stranahan, VP Infrastructure",
            "Aaron Stranahan, Director Engineering"
        ));
    }

    #[test]
    fn test_facts_may_conflict_no_proper_names() {
        // No proper names, just job keywords — still conflicts
        assert!(facts_may_conflict(
            "Senior Director at Acme",
            "VP Data Science at BigCo"
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

    // --- is_same_company_role tests ---

    #[test]
    fn test_extract_company() {
        assert_eq!(extract_company("CTO at Acme Corp"), Some("acme corp".into()));
        assert_eq!(extract_company("Engineer at Google"), Some("google".into()));
        assert_eq!(extract_company("Lives in NYC"), None);
    }

    #[test]
    fn test_same_company_duplicate_suppresses_conflict() {
        let content = "# Person\n\n## Career History\n- CTO at Acme @t[2020..2023]\n\n## Current Role\n- CTO at Acme @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty(), "Duplicate role entries should not generate conflict questions");
    }

    #[test]
    fn test_same_company_promotion_suppresses_conflict() {
        let content = "# Person\n\n- Engineer at Acme @t[2018..2020]\n- CTO at Acme @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty(), "Promotion at same company should not generate conflict");
    }

    #[test]
    fn test_different_company_still_conflicts() {
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1, "Different companies with overlap should still conflict");
    }

    #[test]
    fn test_same_company_non_sequential_still_conflicts() {
        // Same company but roles overlap by more than a month boundary
        let content = "# Person\n\n- Engineer at Acme @t[2018..2022]\n- CTO at Acme @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1, "Same company with significant overlap should still conflict");
    }

    // --- duplicate role detection tests ---

    #[test]
    fn test_generate_duplicate_role_questions() {
        let content = "# Person\n\n## Career\n- CTO at Acme @t[2020..2023]\n\n## Summary\n- CTO at Acme @t[2020..2023]";
        let questions = generate_duplicate_role_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Duplicate);
        assert!(questions[0].description.contains("Duplicate role entry"));
    }

    #[test]
    fn test_no_duplicate_role_for_different_roles() {
        let content = "# Person\n\n- Engineer at Acme @t[2018..2020]\n- CTO at Acme @t[2020..2023]";
        let questions = generate_duplicate_role_questions(content);
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
    fn test_boundary_month_different_companies_suppressed() {
        // LinkedIn pattern: Role A ends 2023-09, Role B starts 2023-09 at different company
        let content = "# Person\n\n- Engineer at Acme @t[2020..2023-09]\n- CTO at BigCo @t[2023-09..2024]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty(), "Boundary-month career transition should not generate conflict");
    }

    #[test]
    fn test_boundary_month_year_granularity_suppressed() {
        // Same pattern at year granularity
        let content = "# Person\n\n- Engineer at Acme @t[2018..2020]\n- CTO at BigCo @t[2020..2023]";
        let questions = generate_conflict_questions(content);
        assert!(questions.is_empty(), "Boundary-year career transition should not generate conflict");
    }

    #[test]
    fn test_real_overlap_different_companies_still_conflicts() {
        // Genuine multi-month overlap should still flag
        let content = "# Person\n\n- CTO at Acme @t[2020..2023]\n- CEO at BigCo @t[2022..2024]";
        let questions = generate_conflict_questions(content);
        assert_eq!(questions.len(), 1, "Real overlap should still generate conflict");
    }

    #[test]
    fn test_boundary_month_sequential_helper() {
        assert!(is_boundary_month_sequential("2020", "2023-09", "2023-09", "2024"));
        assert!(is_boundary_month_sequential("2023-09", "2024", "2020", "2023-09"));
        // Point-in-time (start == end) should NOT be treated as sequential
        assert!(!is_boundary_month_sequential("2023", "2023", "2023", "2023"));
        // Real overlap should not match
        assert!(!is_boundary_month_sequential("2020", "2023", "2022", "2024"));
    }
}
