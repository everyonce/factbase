//! Temporal tag validation and conflict detection.

use crate::models::{TemporalTag, TemporalTagType};
use crate::patterns::normalize_date_for_comparison;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};

use super::date::validate_date;
use super::parser::parse_temporal_tags;
use super::range::ranges_overlap;

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
                tag_errors.push(format!("start date: {}", msg));
            }
        }

        if let Some(ref end) = tag.end_date {
            if let Some(msg) = validate_date(end) {
                tag_errors.push(format!("end date: {}", msg));
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

/// An illogical sequence error in a temporal tag
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalSequenceError {
    /// Line number where the error was found
    pub line_number: usize,
    /// Raw text of the temporal tag
    pub raw_text: String,
    /// Description of the sequence error
    pub message: String,
}

/// Detect illogical sequences in temporal tags.
pub fn detect_illogical_sequences(content: &str) -> Vec<TemporalSequenceError> {
    let tags = parse_temporal_tags(content);
    let mut errors = Vec::new();
    let now = Utc::now();
    let one_year_from_now = now + Duration::days(365);

    for mut tag in tags {
        // Collect error messages for this tag
        let mut tag_errors: Vec<String> = Vec::new();

        if let (Some(ref start), Some(ref end)) = (&tag.start_date, &tag.end_date) {
            let start_norm = normalize_date_for_comparison(start);
            let end_norm = normalize_date_for_comparison(end);
            if end_norm < start_norm {
                tag_errors.push(format!("end date {} is before start date {}", end, start));
            }
        }

        if let Some(ref start) = tag.start_date {
            if let Some(msg) = check_future_date(start, &one_year_from_now) {
                tag_errors.push(msg);
            }
        }
        if let Some(ref end) = tag.end_date {
            if let Some(msg) = check_future_date(end, &one_year_from_now) {
                tag_errors.push(msg);
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
            errors.push(TemporalSequenceError {
                line_number: tag.line_number,
                raw_text,
                message,
            });
        }
    }

    errors
}

fn check_future_date(date: &str, one_year_from_now: &DateTime<Utc>) -> Option<String> {
    let normalized = normalize_date_for_comparison(date);
    let future_limit = one_year_from_now.format("%Y-%m-%d").to_string();
    if normalized > future_limit {
        Some(format!("date {} is more than 1 year in the future", date))
    } else {
        None
    }
}

/// A conflict between temporal tags on the same line
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalConflict {
    pub line_number: usize,
    pub tag1: String,
    pub tag2: String,
    pub message: String,
}

/// Detect conflicting temporal tags on the same line.
pub fn detect_temporal_conflicts(content: &str) -> Vec<TemporalConflict> {
    let tags = parse_temporal_tags(content);
    let mut conflicts = Vec::new();

    // Group tag indices by line number
    let mut indices_by_line: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, tag) in tags.iter().enumerate() {
        indices_by_line
            .entry(tag.line_number)
            .or_default()
            .push(idx);
    }

    // Collect conflict info (indices and messages) first
    let mut conflict_info: Vec<(usize, usize, usize, String)> = Vec::new(); // (line, idx1, idx2, msg)

    for (line_number, indices) in indices_by_line {
        if indices.len() < 2 {
            continue;
        }

        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let idx1 = indices[i];
                let idx2 = indices[j];
                if let Some(msg) = check_tag_conflict_msg(&tags[idx1], &tags[idx2]) {
                    conflict_info.push((line_number, idx1, idx2, msg));
                }
            }
        }
    }

    // Track which tags have been moved
    let mut moved: HashSet<usize> = HashSet::new();
    let mut tags = tags; // Make mutable for moving

    // Create conflicts, moving raw_text when possible
    for (line_number, idx1, idx2, message) in conflict_info {
        let tag1 = if moved.contains(&idx1) {
            tags[idx1].raw_text.clone()
        } else {
            moved.insert(idx1);
            std::mem::take(&mut tags[idx1].raw_text)
        };
        let tag2 = if moved.contains(&idx2) {
            tags[idx2].raw_text.clone()
        } else {
            moved.insert(idx2);
            std::mem::take(&mut tags[idx2].raw_text)
        };
        conflicts.push(TemporalConflict {
            line_number,
            tag1,
            tag2,
            message,
        });
    }

    conflicts
}

/// Check if two tags conflict and return the conflict message if so.
fn check_tag_conflict_msg(tag1: &TemporalTag, tag2: &TemporalTag) -> Option<String> {
    use TemporalTagType::*;

    match (&tag1.tag_type, &tag2.tag_type) {
        (Ongoing, Range) | (Range, Ongoing) => {
            return Some("conflicting tags: one implies ongoing, other implies ended".to_string());
        }
        (Ongoing, Historical) | (Historical, Ongoing) => {
            return Some("conflicting tags: one implies ongoing, other implies ended".to_string());
        }
        _ => {}
    }

    check_range_overlap_conflict(tag1, tag2)
}

fn check_range_overlap_conflict(tag1: &TemporalTag, tag2: &TemporalTag) -> Option<String> {
    let start1 = tag1.start_date.as_deref();
    let end1 = tag1.end_date.as_deref();
    let start2 = tag2.start_date.as_deref();
    let end2 = tag2.end_date.as_deref();

    if let (Some(s1), Some(s2)) = (start1, start2) {
        if s1 != s2 {
            if let (Some(e1), Some(e2)) = (end1, end2) {
                if e1 != e2 && ranges_overlap(s1, e1, s2, e2) {
                    return Some(format!(
                        "overlapping ranges with different boundaries: {}..{} vs {}..{}",
                        s1, e1, s2, e2
                    ));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_conflicts_no_conflicts() {
        let content = "- Fact @t[2020..2022]\n- Another @t[2023..]";
        let conflicts = detect_temporal_conflicts(content);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflicts_ongoing_vs_range() {
        let content = "- Role @t[2020..] @t[2020..2022]";
        let conflicts = detect_temporal_conflicts(content);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].message.contains("ongoing"));
    }

    #[test]
    fn test_detect_illogical_end_before_start() {
        let content = "- Fact @t[2022..2020]";
        let errors = detect_illogical_sequences(content);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("before start date"));
    }

    #[test]
    fn test_detect_illogical_valid_range() {
        let content = "- Fact @t[2020..2022]";
        let errors = detect_illogical_sequences(content);
        assert!(errors.is_empty());
    }
}
