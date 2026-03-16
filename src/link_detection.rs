//! Link detection service for entity mentions.
//!
//! Detects links via regex (`[[id]]`) and fuzzy string matching (full title,
//! unique words, abbreviations, normalized variants). No LLM required.
//!
//! Supports two modes:
//! - `Exact`: Full title + unique words + abbreviations (original behavior)
//! - `Fuzzy` (default): Adds normalized matching (diacritics, prefix stripping),
//!   minimum entity name length filter (3 chars), and prefix-stripped candidates.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::patterns::{MANUAL_LINK_REGEX, WIKILINK_REGEX};

/// Link matching mode for entity detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkMatchMode {
    /// Original behavior: full title + unique words + abbreviations
    Exact,
    /// Enhanced: adds normalized matching, prefix stripping, min-length filter
    #[default]
    Fuzzy,
}

/// A detected link between documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedLink {
    /// ID of the target document
    pub target_id: String,
    /// Title of the target document
    pub target_title: String,
    /// The text that triggered the link detection
    pub mention_text: String,
    /// Surrounding context where the mention was found
    pub context: String,
}

// =============================================================================
// Fuzzy string pre-filter for link detection
// =============================================================================

/// Common words (4+ chars) excluded from single-word matching.
const STOP_WORDS: &[&str] = &[
    "about", "after", "also", "back", "been", "being", "came", "come", "could", "does", "down",
    "each", "even", "every", "find", "first", "from", "gave", "goes", "going", "good", "great",
    "have", "here", "high", "into", "just", "keep", "know", "last", "left", "like", "line", "long",
    "look", "made", "make", "many", "more", "most", "much", "must", "name", "next", "note", "only",
    "open", "other", "over", "part", "said", "same", "show", "side", "some", "such", "take",
    "tell", "than", "that", "them", "then", "they", "this", "time", "took", "turn", "upon", "very",
    "want", "well", "went", "were", "what", "when", "will", "with", "word", "work", "year", "your",
    "about", "above", "after", "again", "along", "being", "below", "between", "both", "could",
    "doing", "during", "every", "found", "given", "going", "group", "having", "house", "large",
    "later", "never", "often", "order", "other", "place", "point", "right", "shall", "should",
    "since", "small", "state", "still", "their", "there", "these", "thing", "think", "those",
    "three", "through", "under", "until", "using", "where", "which", "while", "world", "would",
];

/// A match candidate mapping a pattern string to an entity.
struct MatchCandidate {
    entity_id: String,
    entity_title: String,
    regex: Regex,
}

/// Common title prefixes stripped in fuzzy mode to generate additional candidates.
const TITLE_PREFIXES: &[&str] = &["St.", "Mt.", "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Rev."];

/// Minimum entity title length (in chars) for fuzzy matching.
/// Titles shorter than this are only matched via full-title exact regex.
const MIN_FUZZY_TITLE_LEN: usize = 3;

/// Strip diacritics from a string by decomposing to NFD and dropping combining marks.
/// Falls back to simple ASCII mapping for common characters.
fn strip_diacritics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => out.push('A'),
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
            'È' | 'É' | 'Ê' | 'Ë' => out.push('E'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'Ì' | 'Í' | 'Î' | 'Ï' => out.push('I'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => out.push('O'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'Ù' | 'Ú' | 'Û' | 'Ü' => out.push('U'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'Ñ' => out.push('N'),
            'ñ' => out.push('n'),
            'Ç' => out.push('C'),
            'ç' => out.push('c'),
            'Ý' => out.push('Y'),
            'ý' | 'ÿ' => out.push('y'),
            'Ð' => out.push('D'),
            'ð' => out.push('d'),
            'Ø' => out.push('O'),
            'ø' => out.push('o'),
            'Æ' => {
                out.push('A');
                out.push('E');
            }
            'æ' => {
                out.push('a');
                out.push('e');
            }
            'ß' => {
                out.push('s');
                out.push('s');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Strip a known title prefix (e.g., "St. Paul" → "Paul").
/// Returns `Some(remainder)` if a prefix was stripped and remainder is 3+ chars.
fn strip_title_prefix(title: &str) -> Option<String> {
    for prefix in TITLE_PREFIXES {
        if let Some(rest) = title.strip_prefix(prefix) {
            let trimmed = rest.trim();
            if trimmed.len() >= MIN_FUZZY_TITLE_LEN {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Build match candidates from known entities for fuzzy pre-filtering.
///
/// For each entity, generates:
/// 1. Full title match (case-insensitive, word boundary)
/// 2. Unique words (4+ chars, not stop words, not shared across entities)
/// 3. Abbreviation from first letters of each word (3+ chars)
///
/// In `Fuzzy` mode, additionally:
/// 4. Normalized title (diacritics stripped) if different from original
/// 5. Prefix-stripped title (e.g., "St. Paul" → "Paul")
/// 6. Entities with titles < 3 chars are skipped for word/abbreviation candidates
fn build_match_candidates(
    known_entities: &[(String, String)],
    mode: LinkMatchMode,
) -> Vec<MatchCandidate> {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    // Count how many entities each word appears in (lowercased)
    let mut word_counts: HashMap<String, usize> = HashMap::new();
    for (_, title) in known_entities {
        let mut seen = HashSet::new();
        for word in title.split_whitespace() {
            let lower = word.to_lowercase();
            // Strip trailing punctuation for counting
            let clean: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.len() >= 4 && seen.insert(clean.clone()) {
                *word_counts.entry(clean).or_insert(0) += 1;
            }
        }
    }

    let is_fuzzy = mode == LinkMatchMode::Fuzzy;
    let mut candidates = Vec::new();

    /// Helper: push a word-boundary candidate if the regex compiles.
    fn push_candidate(
        candidates: &mut Vec<MatchCandidate>,
        id: &str,
        title: &str,
        pattern: &str,
        case_insensitive: bool,
    ) {
        let prefix = if case_insensitive { "(?i)" } else { "" };
        let pat = format!(r"{prefix}\b{pattern}\b");
        if let Ok(re) = Regex::new(&pat) {
            candidates.push(MatchCandidate {
                entity_id: id.to_string(),
                entity_title: title.to_string(),
                regex: re,
            });
        }
    }

    for (id, title) in known_entities {
        // In fuzzy mode, skip very short entity titles for word/abbrev candidates
        let title_chars: usize = title.chars().count();
        let skip_fuzzy_extras = is_fuzzy && title_chars < MIN_FUZZY_TITLE_LEN;

        // 1. Full title match (always generated)
        push_candidate(&mut candidates, id, title, &regex::escape(title), true);

        if !skip_fuzzy_extras {
            let words: Vec<&str> = title.split_whitespace().collect();

            // 2. Unique words 4+ chars
            for word in &words {
                let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
                let lower = clean.to_lowercase();
                if lower.len() >= 4
                    && !stop.contains(lower.as_str())
                    && word_counts.get(&lower).copied().unwrap_or(0) == 1
                {
                    push_candidate(&mut candidates, id, title, &regex::escape(&clean), true);
                }
            }

            // 3. Abbreviation (first letter of each word, 3+ chars)
            if words.len() >= 3 {
                let abbrev: String = words
                    .iter()
                    .filter_map(|w| w.chars().next())
                    .filter(|c| c.is_alphabetic())
                    .map(|c| c.to_ascii_uppercase())
                    .collect();
                if abbrev.len() >= 3 {
                    push_candidate(&mut candidates, id, title, &regex::escape(&abbrev), false);
                }
            }
        }

        // Fuzzy-only enhancements
        if is_fuzzy && !skip_fuzzy_extras {
            // 4. Normalized title (diacritics stripped) — only if different
            let normalized = strip_diacritics(title);
            if normalized != *title {
                push_candidate(
                    &mut candidates,
                    id,
                    title,
                    &regex::escape(&normalized),
                    true,
                );
            }

            // 5. Prefix-stripped title (e.g., "St. Paul" → "Paul")
            if let Some(stripped) = strip_title_prefix(title) {
                push_candidate(&mut candidates, id, title, &regex::escape(&stripped), true);
            }
        }
    }

    candidates
}

/// Extract surrounding sentence context for a match at the given byte position.
fn extract_context(content: &str, start: usize, end: usize) -> String {
    // Find sentence boundaries (period+space, newline, or content bounds)
    let ctx_start = content[..start]
        .rfind(|c: char| c == '\n' || (c == '.' && start > 1))
        .map(|p| p + 1)
        .unwrap_or(0);
    let ctx_end = content[end..]
        .find(['\n', '.'])
        .map(|p| end + p + 1)
        .unwrap_or(content.len());
    content[ctx_start..ctx_end.min(content.len())]
        .trim()
        .to_string()
}

/// Run fuzzy string matching on content against known entities.
/// Returns (matched links, set of matched entity IDs).
fn string_match_links(
    content: &str,
    source_id: &str,
    candidates: &[MatchCandidate],
) -> (Vec<DetectedLink>, HashSet<String>) {
    let mut links = Vec::new();
    let mut matched_ids = HashSet::new();

    for candidate in candidates {
        if candidate.entity_id == source_id || matched_ids.contains(&candidate.entity_id) {
            continue;
        }
        if let Some(m) = candidate.regex.find(content) {
            let context = extract_context(content, m.start(), m.end());
            links.push(DetectedLink {
                target_id: candidate.entity_id.clone(),
                target_title: candidate.entity_title.clone(),
                mention_text: m.as_str().to_string(),
                context,
            });
            matched_ids.insert(candidate.entity_id.clone());
        }
    }

    (links, matched_ids)
}

/// Service for detecting entity mentions in documents using regex and string matching.
pub struct LinkDetector {
    batch_size: usize,
    match_mode: LinkMatchMode,
}

impl LinkDetector {
    /// Create a new LinkDetector with default batch size and fuzzy matching.
    pub fn new() -> Self {
        Self {
            batch_size: 5,
            match_mode: LinkMatchMode::Fuzzy,
        }
    }

    /// Set the link match mode.
    pub fn with_match_mode(mut self, mode: LinkMatchMode) -> Self {
        self.match_mode = mode;
        self
    }

    /// Returns the configured batch size for link detection
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Detect links in a single document.
    pub fn detect_links(
        &self,
        content: &str,
        source_id: &str,
        known_entities: &[(String, String)], // (id, title)
    ) -> Vec<DetectedLink> {
        // Most documents have few links; 4 is a reasonable default
        let mut links = Vec::with_capacity(4);

        // Build lookup map for O(1) access
        let id_to_title: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (id.as_str(), title.as_str()))
            .collect();

        // Extract manual [[id]] links
        for cap in MANUAL_LINK_REGEX.captures_iter(content) {
            let target_id = cap[1].to_string();
            if target_id != source_id {
                if let Some(&title) = id_to_title.get(target_id.as_str()) {
                    let mention_text = format!("[[{target_id}]]");
                    links.push(DetectedLink {
                        target_id,
                        target_title: title.to_string(),
                        mention_text,
                        context: String::new(),
                    });
                }
            }
        }

        // Extract [[Name]] wikilinks and resolve by title
        // Handles both [[Name]] and [[path|Display Name]] formats
        let title_to_id: HashMap<String, &str> = known_entities
            .iter()
            .map(|(id, title)| (title.to_lowercase(), id.as_str()))
            .collect();
        for cap in WIKILINK_REGEX.captures_iter(content) {
            let raw = &cap[1];
            // Skip if it's a hex ID (already handled above)
            if MANUAL_LINK_REGEX.is_match(&format!("[[{raw}]]")) {
                continue;
            }
            // Handle [[path|display]] format — extract display portion for title lookup
            let name = if let Some((_path, display)) = raw.split_once('|') {
                display
            } else {
                raw
            };
            if let Some(&target_id) = title_to_id.get(&name.to_lowercase()) {
                if target_id != source_id && !links.iter().any(|l| l.target_id == target_id) {
                    links.push(DetectedLink {
                        target_id: target_id.to_string(),
                        target_title: name.to_string(),
                        mention_text: format!("[[{raw}]]"),
                        context: String::new(),
                    });
                }
            }
        }

        if known_entities.is_empty() {
            return links;
        }

        // Fuzzy string matching
        let candidates = build_match_candidates(known_entities, self.match_mode);
        let (string_links, _matched_ids) = string_match_links(content, source_id, &candidates);
        for sl in string_links {
            if !links.iter().any(|l| l.target_id == sl.target_id) {
                links.push(sl);
            }
        }

        links
    }

    /// Batch detect links for multiple documents.
    /// Returns a HashMap of source_id -> `Vec<DetectedLink>`.
    pub fn detect_links_batch(
        &self,
        documents: &[(&str, &str, &str)], // (id, title, content)
        known_entities: &[(String, String)],
    ) -> HashMap<String, Vec<DetectedLink>> {
        let mut results: HashMap<String, Vec<DetectedLink>> = HashMap::new();

        // Build lookup map for O(1) access
        let id_to_title: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (id.as_str(), title.as_str()))
            .collect();

        // Initialize results and extract manual links + wikilinks
        let title_to_id: HashMap<String, &str> = known_entities
            .iter()
            .map(|(id, title)| (title.to_lowercase(), id.as_str()))
            .collect();
        for (id, _, content) in documents {
            let mut links = Vec::with_capacity(4);
            for cap in MANUAL_LINK_REGEX.captures_iter(content) {
                let target_id = cap[1].to_string();
                if target_id != *id {
                    if let Some(&title) = id_to_title.get(target_id.as_str()) {
                        let mention_text = format!("[[{target_id}]]");
                        links.push(DetectedLink {
                            target_id,
                            target_title: title.to_string(),
                            mention_text,
                            context: String::new(),
                        });
                    }
                }
            }
            // Resolve [[Name]] and [[path|Name]] wikilinks by title
            for cap in WIKILINK_REGEX.captures_iter(content) {
                let raw = &cap[1];
                if MANUAL_LINK_REGEX.is_match(&format!("[[{raw}]]")) {
                    continue;
                }
                let name = if let Some((_path, display)) = raw.split_once('|') {
                    display
                } else {
                    raw
                };
                if let Some(&target_id) = title_to_id.get(&name.to_lowercase()) {
                    if target_id != *id && !links.iter().any(|l| l.target_id == target_id) {
                        links.push(DetectedLink {
                            target_id: target_id.to_string(),
                            target_title: name.to_string(),
                            mention_text: format!("[[{raw}]]"),
                            context: String::new(),
                        });
                    }
                }
            }
            results.insert(id.to_string(), links);
        }

        if known_entities.is_empty() || documents.is_empty() {
            return results;
        }

        // Fuzzy string matching
        let candidates = build_match_candidates(known_entities, self.match_mode);
        for (id, _, content) in documents {
            let (string_links, _matched_ids) = string_match_links(content, id, &candidates);
            if let Some(links) = results.get_mut(*id) {
                for sl in string_links {
                    if !links.iter().any(|l| l.target_id == sl.target_id) {
                        links.push(sl);
                    }
                }
            }
        }

        results
    }
}

impl Default for LinkDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkMatchMode {
    /// Parse from a string value (e.g., from perspective.yaml).
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("exact") => Self::Exact,
            _ => Self::Fuzzy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_detector_manual_links() {
        let detector = LinkDetector::new();

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector.detect_links(content, "source1", &known);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "abc123");
        assert_eq!(links[0].target_title, "Test Doc");
    }

    #[test]
    fn test_link_detector_filters_self_references() {
        let detector = LinkDetector::new();

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        // source_id matches the link - should be filtered
        let links = detector.detect_links(content, "abc123", &known);
        assert!(links.is_empty());
    }

    #[test]
    fn test_link_detector_string_match() {
        let detector = LinkDetector::new();

        let content = "I met with John Doe yesterday.";
        let known = vec![("def456".to_string(), "John Doe".to_string())];

        let links = detector.detect_links(content, "source1", &known);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "def456");
        assert_eq!(links[0].target_title, "John Doe");
        assert!(links[0].context.contains("John Doe"));
    }

    #[test]
    fn test_link_detector_empty_entities() {
        let detector = LinkDetector::new();

        let content = "Some content.";
        let known: Vec<(String, String)> = vec![];

        let links = detector.detect_links(content, "source1", &known);
        assert!(links.is_empty());
    }

    #[test]
    fn test_link_detector_deduplicates() {
        let detector = LinkDetector::new();

        let content = "Test Doc mentioned twice. Test Doc again.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector.detect_links(content, "source1", &known);

        // Should only have one link despite two mentions
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_manual_link_regex() {
        // Valid links
        assert!(MANUAL_LINK_REGEX.is_match("[[abc123]]"));
        assert!(MANUAL_LINK_REGEX.is_match("See [[def456]] here"));

        // Invalid links
        assert!(!MANUAL_LINK_REGEX.is_match("[[ABC123]]")); // uppercase
        assert!(!MANUAL_LINK_REGEX.is_match("[[abc12]]")); // too short
        assert!(!MANUAL_LINK_REGEX.is_match("[[abc1234]]")); // too long
        assert!(!MANUAL_LINK_REGEX.is_match("[[ghijkl]]")); // invalid hex
    }

    #[test]
    fn test_detect_links_batch() {
        let detector = LinkDetector::new();

        let docs = vec![
            ("doc1", "Doc One", "I met John Doe."),
            ("doc2", "Doc Two", "No mentions here."),
        ];
        let known = vec![("person1".to_string(), "John Doe".to_string())];

        let results = detector.detect_links_batch(&docs, &known);

        assert_eq!(results.len(), 2);
        assert_eq!(
            results
                .get("doc1")
                .expect("doc1 should be in results")
                .len(),
            1
        );
        assert_eq!(
            results.get("doc1").expect("doc1 should be in results")[0].target_id,
            "person1"
        );
        assert!(results
            .get("doc2")
            .expect("doc2 should be in results")
            .is_empty());
    }

    #[test]
    fn test_detect_links_batch_with_manual_links() {
        let detector = LinkDetector::new();

        let docs = vec![("doc1", "Doc One", "See [[abc123]] for details.")];
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let results = detector.detect_links_batch(&docs, &known);

        assert_eq!(
            results
                .get("doc1")
                .expect("doc1 should be in results")
                .len(),
            1
        );
        assert_eq!(
            results.get("doc1").expect("doc1 should be in results")[0].target_id,
            "abc123"
        );
    }

    #[test]
    fn test_manual_link_unknown_id_ignored() {
        let detector = LinkDetector::new();

        let content = "See [[xyz123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector.detect_links(content, "source1", &known);

        // xyz123 not in known_entities, so no links detected
        assert!(links.is_empty());
    }

    #[test]
    fn test_multiple_manual_links() {
        let detector = LinkDetector::new();

        let content = "See [[abc123]] and [[def456]] for details.";
        let known = vec![
            ("abc123".to_string(), "Doc A".to_string()),
            ("def456".to_string(), "Doc B".to_string()),
        ];

        let links = detector.detect_links(content, "source1", &known);

        assert_eq!(links.len(), 2);
        let ids: Vec<&str> = links.iter().map(|l| l.target_id.as_str()).collect();
        assert!(ids.contains(&"abc123"));
        assert!(ids.contains(&"def456"));
    }

    #[test]
    fn test_batch_filters_self_references() {
        let detector = LinkDetector::new();

        // doc1 links to itself via [[doc001]]
        let docs = vec![("doc001", "Doc One", "See [[doc001]] for self-ref.")];
        let known = vec![("doc001".to_string(), "Doc One".to_string())];

        let results = detector.detect_links_batch(&docs, &known);

        // Self-reference should be filtered out
        assert!(results
            .get("doc001")
            .expect("doc001 should be in results")
            .is_empty());
    }

    // =========================================================================
    // Fuzzy string pre-filter tests
    // =========================================================================

    #[test]
    fn test_build_candidates_full_title() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        // Should have full title + unique words + abbreviation
        assert!(candidates
            .iter()
            .any(|c| c.regex.is_match("Delta Air Lines")));
    }

    #[test]
    fn test_string_match_exact_title() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, matched) =
            string_match_links("We flew Delta Air Lines to NYC.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
        assert!(matched.contains("e1"));
    }

    #[test]
    fn test_string_match_case_insensitive() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("We flew delta air lines to NYC.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_unique_word() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("The Delta subsidiary expanded.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_word_boundary() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        // "Deltaforce" should NOT match "Delta" as a word
        let (links, _) = string_match_links("The Deltaforce team arrived.", "src", &candidates);
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_ambiguous_word_excluded() {
        // "Mount" appears in both entities — should not be used for matching
        let entities = vec![
            ("e1".into(), "Mount Vesuvius".into()),
            ("e2".into(), "Mount St. Helens".into()),
        ];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        // "Mount" alone should not match either entity
        let (links, _) = string_match_links("The mount was visible from afar.", "src", &candidates);
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_abbreviation() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("DAL stock rose 5% today.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_abbreviation_needs_3_chars() {
        // Two-word title → abbreviation "AB" is only 2 chars, should not match
        let entities = vec![("e1".into(), "Acme Bricks".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("AB Corp is here.", "src", &candidates);
        // Should not match via abbreviation (only 2 chars)
        // May match via unique word "Acme" or "Bricks" though
        let abbrev_match = links.iter().any(|l| l.mention_text == "AB");
        assert!(!abbrev_match);
    }

    #[test]
    fn test_string_match_skips_self() {
        let entities = vec![("src".into(), "Self Document".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) =
            string_match_links("This is the Self Document itself.", "src", &candidates);
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_no_duplicate_links() {
        // Entity matches via both full title and unique word — should only appear once
        let entities = vec![("e1".into(), "Vesuvius".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links(
            "Vesuvius erupted. Vesuvius is a volcano.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_string_match_context_extraction() {
        let entities = vec![("e1".into(), "Acme Corporation".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links(
            "First line.\nWe partnered with Acme Corporation last year.\nThird line.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert!(links[0].context.contains("Acme Corporation"));
    }

    #[test]
    fn test_prefilter_finds_title_match() {
        let detector = LinkDetector::new();

        let content = "We met with Delta Air Lines representatives.";
        let known = vec![
            ("e1".to_string(), "Delta Air Lines".to_string()),
            ("e2".to_string(), "Unrelated Corp".to_string()),
        ];

        let links = detector.detect_links(content, "src", &known);

        // Delta Air Lines found by string matching
        assert!(links.iter().any(|l| l.target_id == "e1"));
        // Unrelated Corp not in content
        assert!(!links.iter().any(|l| l.target_id == "e2"));
    }

    #[test]
    fn test_batch_string_match() {
        let detector = LinkDetector::new();

        let docs = vec![
            ("doc1", "Doc One", "We visited Mount Vesuvius."),
            ("doc2", "Doc Two", "No entities here."),
        ];
        let known = vec![("e1".to_string(), "Mount Vesuvius".to_string())];

        let results = detector.detect_links_batch(&docs, &known);

        assert_eq!(results.get("doc1").unwrap().len(), 1);
        assert_eq!(results.get("doc1").unwrap()[0].target_id, "e1");
        assert!(results.get("doc2").unwrap().is_empty());
    }

    #[test]
    fn test_stop_words_excluded() {
        // "Great" is a stop word — should not match as a single word
        let entities = vec![("e1".into(), "The Great Wall".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("It was a great achievement.", "src", &candidates);
        assert!(links.is_empty());
    }

    // --- Wikilink resolution tests ---

    #[test]
    fn test_detect_wikilink_by_name() {
        let detector = LinkDetector::new();
        let content = "See [[John Doe]] for details.";
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        let links = detector.detect_links(content, "src001", &known);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "abc123");
        assert_eq!(links[0].mention_text, "[[John Doe]]");
    }

    #[test]
    fn test_detect_wikilink_case_insensitive() {
        let detector = LinkDetector::new();
        let content = "See [[john doe]] for details.";
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        let links = detector.detect_links(content, "src001", &known);
        assert!(links.iter().any(|l| l.target_id == "abc123"));
    }

    #[test]
    fn test_detect_wikilink_no_self_link() {
        let detector = LinkDetector::new();
        let content = "See [[John Doe]] for details.";
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        // source_id matches the target — should not create self-link
        let links = detector.detect_links(content, "abc123", &known);
        assert!(links.iter().all(|l| l.target_id != "abc123"));
    }

    #[test]
    fn test_detect_wikilink_and_hex_link_coexist() {
        let detector = LinkDetector::new();
        let content = "See [[abc123]] and [[Jane Smith]].";
        let known = vec![
            ("abc123".to_string(), "John Doe".to_string()),
            ("def456".to_string(), "Jane Smith".to_string()),
        ];
        let links = detector.detect_links(content, "src001", &known);
        assert!(links.iter().any(|l| l.target_id == "abc123"));
        assert!(links.iter().any(|l| l.target_id == "def456"));
    }

    #[test]
    fn test_batch_detect_wikilinks() {
        let detector = LinkDetector::new();
        let docs = vec![
            ("doc1", "Doc One", "See [[John Doe]] here."),
            ("doc2", "Doc Two", "No wikilinks."),
        ];
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        let results = detector.detect_links_batch(&docs, &known);
        assert!(results
            .get("doc1")
            .unwrap()
            .iter()
            .any(|l| l.target_id == "abc123"));
        assert!(results
            .get("doc2")
            .unwrap()
            .iter()
            .all(|l| l.target_id != "abc123"));
    }

    #[test]
    fn test_detect_wikilink_path_pipe_format() {
        let detector = LinkDetector::new();
        let content = "See [[people/john-doe|John Doe]] for details.";
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        let links = detector.detect_links(content, "src001", &known);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "abc123");
        assert_eq!(links[0].target_title, "John Doe");
    }

    #[test]
    fn test_batch_detect_wikilink_path_pipe_format() {
        let detector = LinkDetector::new();
        let docs = vec![("doc1", "Doc One", "See [[people/john-doe|John Doe]] here.")];
        let known = vec![("abc123".to_string(), "John Doe".to_string())];
        let results = detector.detect_links_batch(&docs, &known);
        let doc1_links = results.get("doc1").unwrap();
        assert_eq!(doc1_links.len(), 1);
        assert_eq!(doc1_links[0].target_id, "abc123");
    }

    // =========================================================================
    // Fuzzy mode tests
    // =========================================================================

    #[test]
    fn test_fuzzy_substring_match_isaiah_scroll() {
        // "Isaiah Scroll" in text should match "Isaiah" entity via full-title regex
        let detector = LinkDetector::new(); // default is Fuzzy
        let content = "The Isaiah Scroll was found in a cave.";
        let known = vec![("e1".to_string(), "Isaiah".to_string())];
        let links = detector.detect_links(content, "src", &known);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_fuzzy_substring_match_paul_epistles() {
        let detector = LinkDetector::new();
        let content = "The Paul epistles are important texts.";
        let known = vec![("e1".to_string(), "Paul".to_string())];
        let links = detector.detect_links(content, "src", &known);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_fuzzy_word_boundary_no_false_positive() {
        // "Isaiahtown" should NOT match "Isaiah"
        let detector = LinkDetector::new();
        let content = "We visited Isaiahtown last summer.";
        let known = vec![("e1".to_string(), "Isaiah".to_string())];
        let links = detector.detect_links(content, "src", &known);
        assert!(links.is_empty());
    }

    #[test]
    fn test_fuzzy_case_insensitive() {
        let detector = LinkDetector::new();
        let content = "We read about isaiah in the text.";
        let known = vec![("e1".to_string(), "Isaiah".to_string())];
        let links = detector.detect_links(content, "src", &known);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_fuzzy_normalized_diacritics() {
        // Text has ASCII "Rene", entity has "René" — fuzzy mode should match
        let entities = vec![("e1".into(), "René".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("We met Rene at the conference.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_fuzzy_normalized_diacritics_original_still_matches() {
        // Text has the original diacritics — should still match
        let entities = vec![("e1".into(), "René".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("We met René at the conference.", "src", &candidates);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_fuzzy_prefix_stripped_st() {
        // "St. Paul" entity should also match plain "Paul" in fuzzy mode
        let entities = vec![("e1".into(), "St. Paul".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("Paul wrote many letters.", "src", &candidates);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_fuzzy_prefix_stripped_mt() {
        let entities = vec![("e1".into(), "Mt. Vesuvius".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        let (links, _) = string_match_links("Vesuvius erupted in 79 AD.", "src", &candidates);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_fuzzy_min_length_skips_short_titles() {
        // Entity with 2-char title should not generate word/abbrev candidates
        // but full-title match still works
        let entities = vec![("e1".into(), "AI".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        // Full title "AI" should still match at word boundary
        let (links, _) = string_match_links("AI is transforming the world.", "src", &candidates);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_fuzzy_min_length_single_char_no_match_in_word() {
        // Single-char entity "I" — full title regex with \b should not match "I" inside words
        // but would match standalone "I" — this is expected behavior
        let entities = vec![("e1".into(), "I".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Fuzzy);
        // "I" appears as a word — full title match fires
        let (links, _) = string_match_links("I went to the store.", "src", &candidates);
        assert_eq!(links.len(), 1); // full title match still works
    }

    #[test]
    fn test_exact_mode_no_normalized_candidates() {
        // In exact mode, diacritics normalization should NOT be applied
        let entities = vec![("e1".into(), "René".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Exact);
        // "Rene" (without accent) should NOT match in exact mode
        let (links, _) = string_match_links("We met Rene at the conference.", "src", &candidates);
        assert!(links.is_empty());
    }

    #[test]
    fn test_exact_mode_no_prefix_stripping() {
        // In exact mode, "St. Paul" should NOT generate a "Paul" prefix-stripped candidate.
        // However, "Paul" (4+ chars, unique word) still matches via unique-word matching.
        // So we test with a word that appears in multiple entities to prevent unique-word match.
        let entities = vec![
            ("e1".into(), "St. Paul".into()),
            ("e2".into(), "Paul of Tarsus".into()),
        ];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Exact);
        // "Paul" alone should NOT match either entity in exact mode:
        // - No prefix-stripped candidate (exact mode)
        // - "Paul" appears in both entities, so unique-word matching skips it
        let (links, _) = string_match_links("Paul wrote many letters.", "src", &candidates);
        assert!(links.is_empty());
    }

    #[test]
    fn test_exact_mode_full_title_still_works() {
        // Exact mode should still match full titles
        let entities = vec![("e1".into(), "St. Paul".into())];
        let candidates = build_match_candidates(&entities, LinkMatchMode::Exact);
        let (links, _) = string_match_links("St. Paul wrote many letters.", "src", &candidates);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_fuzzy_ambiguous_same_name_different_entities_skipped() {
        // Two entities with the same title "Joshua" — both generate full-title
        // candidates. string_match_links deduplicates by entity_id, so both
        // could match. This tests that the system doesn't panic or produce
        // unexpected results with duplicate titles.
        let detector = LinkDetector::new();
        let content = "Joshua led the people.";
        let known = vec![
            ("e1".to_string(), "Joshua".to_string()),
            ("e2".to_string(), "Joshua".to_string()),
        ];
        let links = detector.detect_links(content, "src", &known);
        // Both entities match via full-title regex; both are valid links
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_fuzzy_perspective_config_exact() {
        // Verify LinkMatchMode::from_str_opt works
        assert_eq!(
            LinkMatchMode::from_str_opt(Some("exact")),
            LinkMatchMode::Exact
        );
        assert_eq!(
            LinkMatchMode::from_str_opt(Some("fuzzy")),
            LinkMatchMode::Fuzzy
        );
        assert_eq!(LinkMatchMode::from_str_opt(None), LinkMatchMode::Fuzzy);
        assert_eq!(
            LinkMatchMode::from_str_opt(Some("unknown")),
            LinkMatchMode::Fuzzy
        );
    }

    #[test]
    fn test_fuzzy_detector_with_match_mode() {
        let detector = LinkDetector::new().with_match_mode(LinkMatchMode::Exact);
        // In exact mode, "Rene" should NOT match "René"
        let content = "We met Rene at the conference.";
        let known = vec![("e1".to_string(), "René".to_string())];
        let links = detector.detect_links(content, "src", &known);
        assert!(links.is_empty());
    }

    #[test]
    fn test_strip_diacritics_basic() {
        assert_eq!(strip_diacritics("René"), "Rene");
        assert_eq!(strip_diacritics("naïve"), "naive");
        assert_eq!(strip_diacritics("São Paulo"), "Sao Paulo");
        assert_eq!(strip_diacritics("Ñoño"), "Nono");
        assert_eq!(strip_diacritics("über"), "uber");
        assert_eq!(strip_diacritics("plain text"), "plain text");
    }

    #[test]
    fn test_strip_title_prefix() {
        assert_eq!(strip_title_prefix("St. Paul"), Some("Paul".to_string()));
        assert_eq!(
            strip_title_prefix("Mt. Vesuvius"),
            Some("Vesuvius".to_string())
        );
        assert_eq!(strip_title_prefix("Dr. Smith"), Some("Smith".to_string()));
        // Too short after stripping
        assert_eq!(strip_title_prefix("St. AB"), None);
        // No prefix
        assert_eq!(strip_title_prefix("Paul"), None);
    }
}
