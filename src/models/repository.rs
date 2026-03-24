use super::format::FormatConfig;
use super::question::QuestionType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// A user-defined citation pattern for tier-1 validation (from perspective.yaml).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CitationPattern {
    /// Pattern name (e.g., "catalog_number")
    pub name: String,
    /// Regex pattern string (e.g., "[A-Z]{1,3}[- ]?\\d+")
    pub pattern: String,
    /// Optional human-readable description
    pub description: Option<String>,
}

/// A stale_days value that supports `never`/`infinite`/`null`/`0` as "never stale".
/// `None` means never stale; `Some(n)` means stale after n days.
#[derive(Debug, Clone, PartialEq)]
pub struct StaleDaysEntry(pub Option<u32>);

impl<'de> Deserialize<'de> for StaleDaysEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_stale_days_value(d).map(StaleDaysEntry)
    }
}

impl Serialize for StaleDaysEntry {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            None => s.serialize_none(),
            Some(n) => s.serialize_u32(n),
        }
    }
}

/// Deserialize a stale_days value: number, null, "never", "infinite", or 0 → None (never stale).
fn deserialize_stale_days_value<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<u32>, D::Error> {
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    match &v {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Number(n) => {
            let n = n
                .as_u64()
                .ok_or_else(|| Error::custom("stale_days must be non-negative"))?;
            Ok(if n == 0 { None } else { Some(n as u32) })
        }
        serde_json::Value::String(s) => match s.as_str() {
            "never" | "infinite" => Ok(None),
            other => {
                if let Ok(n) = other.parse::<u32>() {
                    Ok(if n == 0 { None } else { Some(n) })
                } else {
                    Err(Error::custom(format!(
                        "invalid stale_days value: {other:?}; expected a number, null, \"never\", or \"infinite\""
                    )))
                }
            }
        },
        other => Err(Error::custom(format!(
            "invalid stale_days value: {other}; expected a number, null, \"never\", or \"infinite\""
        ))),
    }
}

/// Per-source-type staleness configuration (from `source_types` in perspective.yaml).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceTypeConfig {
    /// Staleness threshold for this source type. `None` means never stale.
    #[serde(
        default,
        deserialize_with = "deserialize_stale_days_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub stale_days: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewPerspective {
    /// Override global stale_days threshold for this repository
    pub stale_days: Option<u32>,
    /// Per-document-type stale_days overrides. `null`, `"never"`, or `"infinite"` means never stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_days_by_type: Option<HashMap<String, StaleDaysEntry>>,
    /// Per-source-type staleness configuration (keyed by {type:x} tag value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_types: Option<HashMap<String, SourceTypeConfig>>,
    /// Required fields per document type (e.g., person: [current_role, location])
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    /// Glob patterns for files to skip during review
    pub ignore_patterns: Option<Vec<String>>,
    /// Document types to treat as glossary/definitions (default: ["definitions"])
    pub glossary_types: Option<Vec<String>>,
    /// Question types to suppress entirely for this repository.
    /// No questions of these types will be generated during scan/check.
    /// Valid values: "temporal", "missing", "ambiguous", "precision",
    ///   "stale", "conflict", "duplicate", "corruption", "weak-source"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppress_question_types: Vec<QuestionType>,
    /// Per-document-type question type suppression.
    /// Keys are document type names (e.g., "character", "historical_event").
    /// Values are lists of question types to suppress for that document type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppress_question_types_by_type: Option<HashMap<String, Vec<QuestionType>>>,
}

impl ReviewPerspective {
    /// Returns true if the given question type is suppressed for this repository
    /// or for the specific document type.
    pub fn is_type_suppressed(&self, qt: QuestionType) -> bool {
        self.suppress_question_types.contains(&qt)
    }

    /// Returns true if the given question type is suppressed, considering both
    /// repo-level suppression and per-document-type suppression.
    pub fn is_suppressed_for_doc(&self, qt: QuestionType, doc_type: Option<&str>) -> bool {
        if self.suppress_question_types.contains(&qt) {
            return true;
        }
        if let (Some(by_type), Some(dt)) = (&self.suppress_question_types_by_type, doc_type) {
            if let Some(types) = by_type.get(dt) {
                return types.contains(&qt);
            }
        }
        false
    }

    /// Resolve the effective stale_days for a given source type and document type.
    ///
    /// Priority (highest first):
    /// 1. `source_types[source_type].stale_days` — per-source-type threshold
    /// 2. `stale_days_by_type[doc_type]` — per-document-type threshold
    /// 3. `stale_days` — repository-level override
    /// 4. `global_default` — hardcoded default (passed by caller)
    ///
    /// Returns `None` if the fact should never be flagged stale.
    pub fn resolve_stale_days(
        &self,
        source_type: Option<&str>,
        doc_type: Option<&str>,
        global_default: i64,
    ) -> Option<i64> {
        // 1. Source-type override
        if let (Some(st_map), Some(st)) = (&self.source_types, source_type) {
            if let Some(cfg) = st_map.get(st) {
                return cfg.stale_days.map(|n| n as i64);
            }
        }
        // 2. Doc-type override
        if let (Some(by_type), Some(dt)) = (&self.stale_days_by_type, doc_type) {
            if let Some(entry) = by_type.get(dt) {
                return entry.0.map(|n| n as i64);
            }
        }
        // 3. Repo-level override or global default
        Some(self.stale_days.map(|n| n as i64).unwrap_or(global_default))
    }
}

/// Repository perspective metadata loaded from `perspective.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Perspective {
    /// Knowledge base type (e.g., "customer-intelligence", "personal")
    #[serde(rename = "type", default)]
    pub type_name: String,
    /// Organization name for context
    pub organization: Option<String>,
    /// Focus area description
    pub focus: Option<String>,
    /// Allowed document types for this repository
    pub allowed_types: Option<Vec<String>>,
    /// Review-specific perspective overrides
    #[serde(default)]
    pub review: Option<ReviewPerspective>,
    /// Output format configuration (link style, frontmatter, etc.)
    #[serde(default)]
    pub format: Option<FormatConfig>,
    /// Link detection mode: "exact" (original) or "fuzzy" (default, enhanced matching)
    #[serde(default)]
    pub link_match_mode: Option<String>,
    /// Domain-specific citation patterns for tier-1 validation.
    #[serde(default)]
    pub citation_patterns: Option<Vec<CitationPattern>>,
    /// Free-text description of internal source policy, injected into tier-2 citation triage.
    #[serde(default)]
    pub internal_sources: Option<String>,
    /// Data sources for private/mixed domains (written by agent during setup source discovery).
    /// Distinct from `internal_sources` (which controls citation validation, not data discovery).
    #[serde(default)]
    pub data_sources: Option<serde_json::Value>,
}

/// Default perspective.yaml template created by init commands.
/// All lines are comments so it parses as empty YAML (no active fields).
pub const PERSPECTIVE_TEMPLATE: &str = "\
# Factbase perspective — tells agents what this knowledge base is about\n\
\n\
# What this knowledge base focuses on\n\
# Examples:\n\
#   focus: Mycology field research and species identification\n\
#   focus: Ancient Mediterranean civilizations and trade routes\n\
#   focus: Customer relationship intelligence for solutions architects\n\
\n\
# Allowed document types (derived from folder names)\n\
# Examples for different domains:\n\
#   allowed_types: [species, habitat, region]           # biology\n\
#   allowed_types: [civilization, event, artifact]       # history\n\
#   allowed_types: [person, company, project]            # business\n\
\n\
# Link detection mode: 'fuzzy' (default) or 'exact'\n\
# Fuzzy adds normalized matching (diacritics, prefix stripping like St./Mt.)\n\
# link_match_mode: fuzzy\n\
\n\
# Output format (optional — omit for factbase defaults)\n\
# format:\n\
#   preset: obsidian   # shorthand for all obsidian-friendly settings\n\
#   link_style: wikilink   # wikilink | markdown | factbase\n\
#   frontmatter: true      # YAML frontmatter with type, tags, dates\n\
#   inline_links: true     # [[Entity Name]] in body text\n\
#   id_placement: frontmatter  # factbase ID in YAML frontmatter\n\
\n\
# Review quality settings\n\
# review:\n\
#   stale_days: 180\n\
#   required_fields:\n\
#     species: [classification, habitat, edibility]\n\
#     civilization: [period, region, key_figures]\n\
#     person: [current_role, location]\n\
#   source_types:\n\
#     web:      { stale_days: 180 }\n\
#     book:     { stale_days: never }\n\
#     rfc:      { stale_days: never }\n\
#     slack:    { stale_days: 90 }\n\
#     email:    { stale_days: 90 }\n\
\n\
# Domain-specific citation patterns (tier-1 validation)\n\
# Citations matching any pattern pass without a weak-source question.\n\
# citation_patterns:\n\
#   - name: catalog_number\n\
#     pattern: '[A-Z]{1,3}[- ]?\\d+'\n\
#     description: Record label catalog numbers (e.g., CL 1355, SD 1361)\n\
#   - name: section_ref\n\
#     pattern: '\\w+ \\d+:\\d+'\n\
#     description: Named section references in Word N:N format (e.g., Code 12:5, Title 42:1983)\n\
\n\
# Suppress specific question types for this repository.\n\
# Use for canonical-source KBs where certain question types are structurally\n\
# inapplicable (e.g., scripture, standards, legal codes, RFCs).\n\
# Valid types: temporal, missing, ambiguous, precision, stale, conflict, duplicate, corruption, weak-source\n\
# review:\n\
#   suppress_question_types: [temporal]   # no @t[] questions for canon-text KBs\n";

/// Load and parse `perspective.yaml` from a repository root directory.
///
/// Returns `Some(Perspective)` if the file exists and contains at least one
/// non-default field. Returns `None` if the file is missing, empty, or
/// all-comments. Warns about `perspective.md` / `perspective.json` files.
pub fn load_perspective_from_file(repo_root: &Path) -> Option<Perspective> {
    // Warn about wrong file extensions
    for wrong in &["perspective.md", "perspective.json"] {
        if repo_root.join(wrong).exists() {
            warn!(
                "Found {} — factbase only reads perspective.yaml. Ignoring.",
                wrong
            );
        }
    }

    let yaml_path = repo_root.join("perspective.yaml");
    let content = match std::fs::read_to_string(&yaml_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // If file is empty or all comments, treat as unconfigured
    let has_data = content.lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    if !has_data {
        return None;
    }

    match serde_yaml_ng::from_str::<Perspective>(&content) {
        Ok(p) => {
            // Only return Some if at least one field is populated
            if p.focus.is_some()
                || p.organization.is_some()
                || p.allowed_types.is_some()
                || !p.type_name.is_empty()
                || p.review.is_some()
                || p.format.is_some()
                || p.link_match_mode.is_some()
                || p.citation_patterns.is_some()
                || p.internal_sources.is_some()
                || p.data_sources.is_some()
            {
                Some(p)
            } else {
                None
            }
        }
        Err(e) => {
            warn!(
                "Failed to parse {}: {}. Fix the YAML syntax and re-scan.",
                yaml_path.display(),
                e
            );
            None
        }
    }
}

/// A registered knowledge base repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    /// Unique repository identifier
    pub id: String,
    /// Human-readable repository name
    pub name: String,
    /// Filesystem path to the repository root
    pub path: PathBuf,
    /// Optional perspective metadata
    pub perspective: Option<Perspective>,
    /// When this repository was registered
    pub created_at: DateTime<Utc>,
    /// When documents were last indexed
    pub last_indexed_at: Option<DateTime<Utc>>,
    /// When lint was last run
    pub last_lint_at: Option<DateTime<Utc>>,
}

impl Repository {
    /// Returns a JSON summary with `id`, `name`, `path`, `document_count`, `last_indexed_at`.
    pub fn to_summary_json(&self, doc_count: usize) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "path": self.path.to_string_lossy(),
            "document_count": doc_count,
            "last_indexed_at": self.last_indexed_at.map(|t| t.to_rfc3339())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_summary_json() {
        let repo = Repository {
            id: "test".into(),
            name: "Test Repo".into(),
            path: PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        let json = repo.to_summary_json(5);
        assert_eq!(json["id"], "test");
        assert_eq!(json["name"], "Test Repo");
        assert_eq!(json["path"], "/tmp/test");
        assert_eq!(json["document_count"], 5);
        assert!(json["last_indexed_at"].is_null());
        assert!(json.get("perspective").is_none());
    }

    #[test]
    fn test_load_perspective_valid_yaml() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Mycology research\nallowed_types:\n  - species\n  - habitat\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.focus.as_deref(), Some("Mycology research"));
        assert_eq!(
            p.allowed_types.as_deref(),
            Some(&["species".to_string(), "habitat".to_string()][..])
        );
    }

    #[test]
    fn test_load_perspective_missing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_empty_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.yaml"), "").unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_all_comments() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.yaml"), PERSPECTIVE_TEMPLATE).unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_malformed_yaml() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: [unclosed bracket\n  bad: {yaml\n",
        )
        .unwrap();
        // Should return None and warn, not panic
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_wrong_types_ignored() {
        let tmp = tempfile::TempDir::new().unwrap();
        // allowed_types should be a list, not a string — serde will fail
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "allowed_types: not-a-list\n",
        )
        .unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_warns_about_md_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.md"), "# Perspective").unwrap();
        std::fs::write(tmp.path().join("perspective.yaml"), "focus: test\n").unwrap();
        // Should still load the yaml, just warn about .md
        let p = load_perspective_from_file(tmp.path());
        assert!(p.is_some());
    }

    #[test]
    fn test_load_perspective_warns_about_json_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("perspective.json"), "{}").unwrap();
        // No yaml file — should return None
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_load_perspective_with_review_section() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: test\nreview:\n  stale_days: 90\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.review.as_ref().unwrap().stale_days, Some(90));
    }

    #[test]
    fn test_load_perspective_no_meaningful_fields() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Valid YAML but no recognized perspective fields
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "unrelated_key: some_value\n",
        )
        .unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_none());
    }

    #[test]
    fn test_perspective_template_is_all_comments() {
        for line in PERSPECTIVE_TEMPLATE.lines() {
            let trimmed = line.trim();
            assert!(
                trimmed.is_empty() || trimmed.starts_with('#'),
                "Non-comment line in PERSPECTIVE_TEMPLATE: {trimmed}"
            );
        }
    }

    #[test]
    fn test_load_perspective_with_format_obsidian() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "format:\n  preset: obsidian\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        let fmt = p.format.unwrap();
        assert_eq!(fmt.preset.as_deref(), Some("obsidian"));
    }

    #[test]
    fn test_load_perspective_with_format_explicit() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "format:\n  link_style: wikilink\n  frontmatter: true\n  id_placement: frontmatter\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        let fmt = p.format.unwrap();
        let r = fmt.resolve();
        assert_eq!(r.link_style, super::super::format::LinkStyle::Wikilink);
        assert!(r.frontmatter);
        assert_eq!(
            r.id_placement,
            super::super::format::IdPlacement::Frontmatter
        );
    }

    #[test]
    fn test_load_perspective_with_link_match_mode() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "link_match_mode: exact\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.link_match_mode.as_deref(), Some("exact"));
    }

    #[test]
    fn test_load_perspective_link_match_mode_fuzzy() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "link_match_mode: fuzzy\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        assert_eq!(p.link_match_mode.as_deref(), Some("fuzzy"));
    }

    #[test]
    fn test_load_perspective_with_citation_patterns() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "citation_patterns:\n  - name: catalog_number\n    pattern: \"[A-Z]{1,3}[- ]?\\\\d+\"\n    description: Record label catalog numbers\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        let patterns = p.citation_patterns.unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].name, "catalog_number");
        assert_eq!(
            patterns[0].description.as_deref(),
            Some("Record label catalog numbers")
        );
    }

    #[test]
    fn test_load_perspective_citation_patterns_is_meaningful_field() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "citation_patterns:\n  - name: test\n    pattern: \"\\\\d+\"\n",
        )
        .unwrap();
        // citation_patterns alone should make the perspective non-None
        assert!(load_perspective_from_file(tmp.path()).is_some());
    }

    #[test]
    fn test_load_perspective_with_internal_sources() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: test\ninternal_sources: |\n  Internal wiki: needs page title + date\n  Chat: needs channel + date\n",
        )
        .unwrap();
        let p = load_perspective_from_file(tmp.path()).unwrap();
        let policy = p.internal_sources.unwrap();
        assert!(policy.contains("Internal wiki"));
        assert!(policy.contains("Chat"));
    }

    #[test]
    fn test_load_perspective_internal_sources_is_meaningful_field() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "internal_sources: \"Internal wiki: needs page title + date\"\n",
        )
        .unwrap();
        assert!(load_perspective_from_file(tmp.path()).is_some());
    }

    // --- source_types deserialization ---

    #[test]
    fn test_source_types_numeric_stale_days() {
        let p: ReviewPerspective = serde_yaml_ng::from_str(
            "stale_days: 180\nsource_types:\n  web:\n    stale_days: 180\n  slack:\n    stale_days: 90\n",
        )
        .unwrap();
        let st = p.source_types.as_ref().unwrap();
        assert_eq!(st["web"].stale_days, Some(180));
        assert_eq!(st["slack"].stale_days, Some(90));
    }

    #[test]
    fn test_source_types_never_sentinel() {
        let p: ReviewPerspective = serde_yaml_ng::from_str(
            "source_types:\n  book:\n    stale_days: never\n  rfc:\n    stale_days: infinite\n  paper:\n    stale_days: null\n  static:\n    stale_days: 0\n",
        )
        .unwrap();
        let st = p.source_types.as_ref().unwrap();
        assert_eq!(st["book"].stale_days, None, "never → None");
        assert_eq!(st["rfc"].stale_days, None, "infinite → None");
        assert_eq!(st["paper"].stale_days, None, "null → None");
        assert_eq!(st["static"].stale_days, None, "0 → None");
    }

    // --- ReviewPerspective::resolve_stale_days ---

    #[test]
    fn test_resolve_stale_days_source_type_wins() {
        let r = ReviewPerspective {
            stale_days: Some(180),
            stale_days_by_type: None,
            source_types: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "web".to_string(),
                    SourceTypeConfig {
                        stale_days: Some(90),
                    },
                );
                m
            }),
            ..Default::default()
        };
        assert_eq!(r.resolve_stale_days(Some("web"), None, 365), Some(90));
    }

    #[test]
    fn test_resolve_stale_days_source_type_never() {
        let r = ReviewPerspective {
            stale_days: Some(180),
            source_types: Some({
                let mut m = std::collections::HashMap::new();
                m.insert("book".to_string(), SourceTypeConfig { stale_days: None });
                m
            }),
            ..Default::default()
        };
        assert_eq!(r.resolve_stale_days(Some("book"), None, 365), None);
    }

    #[test]
    fn test_resolve_stale_days_doc_type_fallback() {
        let r = ReviewPerspective {
            stale_days: Some(180),
            stale_days_by_type: Some({
                let mut m = std::collections::HashMap::new();
                m.insert("encyclopedia".to_string(), StaleDaysEntry(Some(365)));
                m
            }),
            source_types: None,
            ..Default::default()
        };
        // No source type → falls through to doc type
        assert_eq!(
            r.resolve_stale_days(None, Some("encyclopedia"), 180),
            Some(365)
        );
    }

    #[test]
    fn test_resolve_stale_days_global_fallback() {
        let r = ReviewPerspective {
            stale_days: None,
            stale_days_by_type: None,
            source_types: None,
            ..Default::default()
        };
        assert_eq!(r.resolve_stale_days(None, None, 180), Some(180));
    }

    #[test]
    fn test_resolve_stale_days_unknown_source_type_falls_through() {
        let r = ReviewPerspective {
            stale_days: Some(180),
            source_types: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "web".to_string(),
                    SourceTypeConfig {
                        stale_days: Some(90),
                    },
                );
                m
            }),
            ..Default::default()
        };
        // Unknown type → falls through to global
        assert_eq!(
            r.resolve_stale_days(Some("unknown-type"), None, 365),
            Some(180)
        );
    }

    #[test]
    fn test_is_suppressed_for_doc_repo_level() {
        let r = ReviewPerspective {
            suppress_question_types: vec![QuestionType::Temporal],
            ..Default::default()
        };
        assert!(r.is_suppressed_for_doc(QuestionType::Temporal, None));
        assert!(r.is_suppressed_for_doc(QuestionType::Temporal, Some("character")));
        assert!(!r.is_suppressed_for_doc(QuestionType::Missing, None));
    }

    #[test]
    fn test_is_suppressed_for_doc_per_type() {
        let r = ReviewPerspective {
            suppress_question_types_by_type: Some({
                let mut m = std::collections::HashMap::new();
                m.insert("character".to_string(), vec![QuestionType::Temporal]);
                m
            }),
            ..Default::default()
        };
        // Suppressed for "character" doc type
        assert!(r.is_suppressed_for_doc(QuestionType::Temporal, Some("character")));
        // Not suppressed for other doc types
        assert!(!r.is_suppressed_for_doc(QuestionType::Temporal, Some("person")));
        assert!(!r.is_suppressed_for_doc(QuestionType::Temporal, None));
        // Other question types not suppressed for "character"
        assert!(!r.is_suppressed_for_doc(QuestionType::Missing, Some("character")));
    }

    #[test]
    fn test_suppress_question_types_by_type_yaml() {
        let yaml = "suppress_question_types_by_type:\n  character: [temporal]\n  historical_event: [temporal, stale]";
        let p: ReviewPerspective = serde_yaml_ng::from_str(yaml).unwrap();
        let by_type = p.suppress_question_types_by_type.unwrap();
        assert_eq!(by_type["character"], vec![QuestionType::Temporal]);
        assert_eq!(
            by_type["historical_event"],
            vec![QuestionType::Temporal, QuestionType::Stale]
        );
    }
}
