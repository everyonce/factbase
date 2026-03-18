//! Staleness determination for duplicate entity entries.
//!
//! Given duplicate entries across documents, determines which are current vs stale
//! using temporal tags or file modification dates as a fallback.

use chrono::{NaiveDate, Utc};
use serde::Serialize;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::TemporalTagType;
use crate::organize::types::{DuplicateEntry, EntryLocation};
use crate::processor::parse_temporal_tags;

/// A duplicate entry group with staleness assessed.
#[derive(Debug, Clone, Serialize)]
pub struct StaleDuplicate {
    /// Entity name.
    pub entity_name: String,
    /// The entry considered most current.
    pub current: EntryLocation,
    /// Entries considered stale (older than current).
    pub stale: Vec<EntryLocation>,
}

/// Assess staleness for a list of duplicate entries.
///
/// For each `DuplicateEntry` with 2+ locations, determines which entry is most
/// current and which are stale. Uses temporal tags when available, falling back
/// to `file_modified_at` from the database.
///
/// Returns only groups where at least one entry is determined stale.
pub fn assess_staleness(
    duplicates: &[DuplicateEntry],
    db: &Database,
) -> Result<Vec<StaleDuplicate>, FactbaseError> {
    let today = Utc::now().date_naive();
    let mut results = Vec::new();

    for dup in duplicates {
        if dup.entries.len() < 2 {
            continue;
        }

        // Compute recency date for each entry.
        let mut dated: Vec<(usize, Option<NaiveDate>)> = Vec::new();
        for (i, entry) in dup.entries.iter().enumerate() {
            let temporal_date = latest_temporal_date(&entry.facts, today);
            let date = temporal_date.or_else(|| file_modified_date(db, &entry.doc_id));
            dated.push((i, date));
        }

        // Find the entry with the most recent date.
        let best = dated
            .iter()
            .filter_map(|(i, d)| d.map(|d| (*i, d)))
            .max_by_key(|(_, d)| *d);

        let Some((best_idx, best_date)) = best else {
            continue; // No dates at all — can't determine staleness.
        };

        // Entries with an older date (or no date) are stale.
        let mut stale = Vec::new();
        for &(i, ref date) in &dated {
            if i == best_idx {
                continue;
            }
            match date {
                Some(d) if *d >= best_date => {} // Same or newer — not stale.
                _ => stale.push(dup.entries[i].clone()),
            }
        }

        if !stale.is_empty() {
            results.push(StaleDuplicate {
                entity_name: dup.entity_name.clone(),
                current: dup.entries[best_idx].clone(),
                stale,
            });
        }
    }

    Ok(results)
}

/// Extract the latest temporal date from a set of fact lines.
///
/// Parses temporal tags from all facts and returns the most recent date.
/// `Ongoing` tags are treated as today (still active).
fn latest_temporal_date(facts: &[String], today: NaiveDate) -> Option<NaiveDate> {
    let content = facts.join("\n");
    let tags = parse_temporal_tags(&content);

    tags.iter()
        .filter_map(|tag| match tag.tag_type {
            TemporalTagType::Ongoing => Some(today),
            TemporalTagType::LastSeen | TemporalTagType::PointInTime => {
                tag.start_date.as_deref().and_then(parse_date)
            }
            TemporalTagType::Range | TemporalTagType::Historical => {
                tag.end_date.as_deref().and_then(parse_date)
            }
            TemporalTagType::Unknown => None,
        })
        .max()
}

/// Look up a document's file_modified_at as a NaiveDate.
fn file_modified_date(db: &Database, doc_id: &str) -> Option<NaiveDate> {
    db.get_document(doc_id)
        .ok()
        .flatten()
        .and_then(|d| d.file_modified_at)
        .map(|dt| dt.date_naive())
}

/// Parse a date string (YYYY, YYYY-MM, or YYYY-MM-DD) into a NaiveDate.
fn parse_date(s: &str) -> Option<NaiveDate> {
    match s.len() {
        4 => {
            let y: i32 = s.parse().ok()?;
            NaiveDate::from_ymd_opt(y, 1, 1)
        }
        7 => {
            let y: i32 = s[..4].parse().ok()?;
            let m: u32 = s[5..7].parse().ok()?;
            NaiveDate::from_ymd_opt(y, m, 1)
        }
        10 => {
            let y: i32 = s[..4].parse().ok()?;
            let m: u32 = s[5..7].parse().ok()?;
            let d: u32 = s[8..10].parse().ok()?;
            NaiveDate::from_ymd_opt(y, m, d)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo_in_db};
    use chrono::TimeZone;

    fn loc(doc_id: &str, doc_title: &str, facts: Vec<&str>) -> EntryLocation {
        EntryLocation {
            doc_id: doc_id.to_string(),
            doc_title: doc_title.to_string(),
            section: "Team".to_string(),
            line_start: 5,
            facts: facts.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn test_parse_date_variants() {
        assert_eq!(parse_date("2024"), NaiveDate::from_ymd_opt(2024, 1, 1));
        assert_eq!(parse_date("2024-06"), NaiveDate::from_ymd_opt(2024, 6, 1));
        assert_eq!(
            parse_date("2024-06-15"),
            NaiveDate::from_ymd_opt(2024, 6, 15)
        );
        assert_eq!(parse_date("bad"), None);
    }

    #[test]
    fn test_latest_temporal_date_ongoing_is_today() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        let facts = vec!["- VP Engineering @t[2022..]".to_string()];
        assert_eq!(latest_temporal_date(&facts, today), Some(today));
    }

    #[test]
    fn test_latest_temporal_date_range() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        let facts = vec!["- VP Engineering @t[2020..2023]".to_string()];
        assert_eq!(
            latest_temporal_date(&facts, today),
            NaiveDate::from_ymd_opt(2023, 1, 1)
        );
    }

    #[test]
    fn test_latest_temporal_date_last_seen() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        let facts = vec!["- VP Engineering @t[~2025-06]".to_string()];
        assert_eq!(
            latest_temporal_date(&facts, today),
            NaiveDate::from_ymd_opt(2025, 6, 1)
        );
    }

    #[test]
    fn test_latest_temporal_date_picks_max() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        let facts = vec![
            "- Joined 2018 @t[=2018]".to_string(),
            "- VP Engineering @t[2022..]".to_string(),
        ];
        // Ongoing = today, which is newer than 2018
        assert_eq!(latest_temporal_date(&facts, today), Some(today));
    }

    #[test]
    fn test_latest_temporal_date_no_tags() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        let facts = vec!["- VP Engineering".to_string()];
        assert_eq!(latest_temporal_date(&facts, today), None);
    }

    #[test]
    fn test_assess_staleness_temporal_tags() {
        let (db, _tmp) = test_db();
        let dups = vec![DuplicateEntry {
            entity_name: "jane smith".to_string(),
            entries: vec![
                loc(
                    "aaa111",
                    "Acme Corp",
                    vec!["- VP Engineering @t[2020..2023]"],
                ),
                loc("bbb222", "Globex Inc", vec!["- Director @t[2024..]"]),
            ],
        }];

        let result = assess_staleness(&dups, &db).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].current.doc_id, "bbb222");
        assert_eq!(result[0].stale.len(), 1);
        assert_eq!(result[0].stale[0].doc_id, "aaa111");
    }

    #[test]
    fn test_assess_staleness_file_modified_fallback() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        let mut doc1 = test_doc("aaa111", "Acme Corp");
        doc1.repo_id = "r1".to_string();
        doc1.file_modified_at = Some(Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap());
        db.upsert_document(&doc1).unwrap();

        let mut doc2 = test_doc("bbb222", "Globex Inc");
        doc2.repo_id = "r1".to_string();
        doc2.file_modified_at = Some(Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap());
        db.upsert_document(&doc2).unwrap();

        let dups = vec![DuplicateEntry {
            entity_name: "jane smith".to_string(),
            entries: vec![
                loc("aaa111", "Acme Corp", vec!["- VP Engineering"]),
                loc("bbb222", "Globex Inc", vec!["- Director"]),
            ],
        }];

        let result = assess_staleness(&dups, &db).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].current.doc_id, "bbb222");
        assert_eq!(result[0].stale[0].doc_id, "aaa111");
    }

    #[test]
    fn test_assess_staleness_no_dates_skipped() {
        let (db, _tmp) = test_db();
        let dups = vec![DuplicateEntry {
            entity_name: "jane smith".to_string(),
            entries: vec![
                loc("aaa111", "Acme Corp", vec!["- VP Engineering"]),
                loc("bbb222", "Globex Inc", vec!["- Director"]),
            ],
        }];

        let result = assess_staleness(&dups, &db).unwrap();
        assert!(
            result.is_empty(),
            "No dates means can't determine staleness"
        );
    }

    #[test]
    fn test_assess_staleness_same_date_not_stale() {
        let (db, _tmp) = test_db();
        let dups = vec![DuplicateEntry {
            entity_name: "jane smith".to_string(),
            entries: vec![
                loc("aaa111", "Acme Corp", vec!["- VP @t[=2024]"]),
                loc("bbb222", "Globex Inc", vec!["- Director @t[=2024]"]),
            ],
        }];

        let result = assess_staleness(&dups, &db).unwrap();
        assert!(result.is_empty(), "Same date means neither is stale");
    }

    #[test]
    fn test_assess_staleness_multiple_stale() {
        let (db, _tmp) = test_db();
        let dups = vec![DuplicateEntry {
            entity_name: "jane smith".to_string(),
            entries: vec![
                loc("aaa111", "Doc A", vec!["- Role @t[2018..2020]"]),
                loc("bbb222", "Doc B", vec!["- Role @t[2020..2022]"]),
                loc("ccc333", "Doc C", vec!["- Role @t[2024..]"]),
            ],
        }];

        let result = assess_staleness(&dups, &db).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].current.doc_id, "ccc333");
        assert_eq!(result[0].stale.len(), 2);
    }
}
