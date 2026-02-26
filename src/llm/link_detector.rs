//! Link detection service for entity mentions.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::ollama::LlmProvider;
use crate::error::FactbaseError;
use crate::patterns::MANUAL_LINK_REGEX;

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
    "about", "after", "also", "back", "been", "being", "came", "come", "could", "does",
    "down", "each", "even", "every", "find", "first", "from", "gave", "goes", "going",
    "good", "great", "have", "here", "high", "into", "just", "keep", "know", "last",
    "left", "like", "line", "long", "look", "made", "make", "many", "more", "most",
    "much", "must", "name", "next", "note", "only", "open", "other", "over", "part",
    "said", "same", "show", "side", "some", "such", "take", "tell", "than", "that",
    "them", "then", "they", "this", "time", "took", "turn", "upon", "very", "want",
    "well", "went", "were", "what", "when", "will", "with", "word", "work", "year",
    "your", "about", "above", "after", "again", "along", "being", "below", "between",
    "both", "could", "doing", "during", "every", "found", "given", "going", "group",
    "having", "house", "large", "later", "never", "often", "order", "other", "place",
    "point", "right", "shall", "should", "since", "small", "state", "still", "their",
    "there", "these", "thing", "think", "those", "three", "through", "under", "until",
    "using", "where", "which", "while", "world", "would",
];

/// A match candidate mapping a pattern string to an entity.
struct MatchCandidate {
    entity_id: String,
    entity_title: String,
    regex: Regex,
}

/// Build match candidates from known entities for fuzzy pre-filtering.
///
/// For each entity, generates:
/// 1. Full title match (case-insensitive, word boundary)
/// 2. Unique words (4+ chars, not stop words, not shared across entities)
/// 3. Abbreviation from first letters of each word (3+ chars)
fn build_match_candidates(known_entities: &[(String, String)]) -> Vec<MatchCandidate> {
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

    let mut candidates = Vec::new();

    for (id, title) in known_entities {
        // 1. Full title match
        let escaped = regex::escape(title);
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{escaped}\b")) {
            candidates.push(MatchCandidate {
                entity_id: id.clone(),
                entity_title: title.clone(),
                regex: re,
            });
        }

        let words: Vec<&str> = title.split_whitespace().collect();

        // 2. Unique words 4+ chars
        for word in &words {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            let lower = clean.to_lowercase();
            if lower.len() >= 4
                && !stop.contains(lower.as_str())
                && word_counts.get(&lower).copied().unwrap_or(0) == 1
            {
                let escaped = regex::escape(&clean);
                if let Ok(re) = Regex::new(&format!(r"(?i)\b{escaped}\b")) {
                    candidates.push(MatchCandidate {
                        entity_id: id.clone(),
                        entity_title: title.clone(),
                        regex: re,
                    });
                }
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
                let escaped = regex::escape(&abbrev);
                if let Ok(re) = Regex::new(&format!(r"\b{escaped}\b")) {
                    candidates.push(MatchCandidate {
                        entity_id: id.clone(),
                        entity_title: title.clone(),
                        regex: re,
                    });
                }
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
        .find(|c: char| c == '\n' || c == '.')
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

/// Service for detecting entity mentions in documents.

/// Default template for single-document link detection.
const DEFAULT_LINK_DETECT_PROMPT: &str = r#"Analyze this document and find mentions of these known entities. Return ONLY a JSON array.

Known entities:
{entities_list}

Document:
{content}

Return a JSON array of objects with "entity" (exact title from list) and "context" (surrounding text). 
Only include entities that are clearly mentioned. Return [] if none found.
Example: [{"entity": "John Doe", "context": "met with John Doe yesterday"}]"#;

/// Default template for batch link detection.
const DEFAULT_LINK_DETECT_BATCH_PROMPT: &str = r#"Analyze these documents and find mentions of known entities. Return ONLY a JSON object.

Known entities:
{entities_list}

Documents:
{docs_section}

Return a JSON object where keys are DOC_IDs and values are arrays of {"entity": "exact title", "context": "surrounding text"}.
Only include entities clearly mentioned. Use empty array [] for docs with no matches.
Example: {"abc123": [{"entity": "John Doe", "context": "met John"}], "def456": []}"#;

pub struct LinkDetector {
    llm: Box<dyn LlmProvider>,
    max_content_length: usize,
    batch_size: usize,
}

#[derive(Deserialize)]
struct LlmLinkResult {
    entity: String,
    context: String,
}

impl LinkDetector {
    /// Create a new LinkDetector with the given LLM provider.
    pub fn new(llm: Box<dyn LlmProvider>) -> Self {
        Self::with_config(llm, 8_000, 5)
    }

    /// Create a new LinkDetector with custom content length and batch size.
    pub fn with_config(
        llm: Box<dyn LlmProvider>,
        max_content_length: usize,
        batch_size: usize,
    ) -> Self {
        Self {
            llm,
            max_content_length,
            batch_size,
        }
    }

    /// Returns the configured batch size for link detection
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Detect links in a single document.
    pub async fn detect_links(
        &self,
        content: &str,
        source_id: &str,
        known_entities: &[(String, String)], // (id, title)
    ) -> Result<Vec<DetectedLink>, FactbaseError> {
        // Most documents have few links; 4 is a reasonable default
        let mut links = Vec::with_capacity(4);

        // Build lookup maps for O(1) access
        let id_to_title: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (id.as_str(), title.as_str()))
            .collect();
        let title_to_id: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (title.as_str(), id.as_str()))
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

        // Skip LLM if no known entities
        if known_entities.is_empty() {
            return Ok(links);
        }

        // Fuzzy string pre-filter: catch obvious matches before LLM
        let candidates = build_match_candidates(known_entities);
        let (string_links, matched_ids) = string_match_links(content, source_id, &candidates);
        for sl in string_links {
            if !links.iter().any(|l| l.target_id == sl.target_id) {
                links.push(sl);
            }
        }

        // Build prompt for LLM — exclude already-matched entities
        let entities_list: String = known_entities
            .iter()
            .filter(|(id, _)| id != source_id && !matched_ids.contains(id))
            .map(|(id, title)| format!("- {title} (id: {id})"))
            .collect::<Vec<_>>()
            .join("\n");

        if entities_list.is_empty() {
            return Ok(links);
        }

        let prompts = crate::Config::load(None).unwrap_or_default().prompts;
        let prompt = crate::config::prompts::resolve_prompt(
            &prompts,
            "link_detect",
            DEFAULT_LINK_DETECT_PROMPT,
            &[("entities_list", &entities_list), ("content", content)],
        );

        let response = self.llm.complete(&prompt).await?;

        // Parse JSON response - try direct parse first, then extract from text
        let results: Option<Vec<LlmLinkResult>> =
            serde_json::from_str(&response).ok().or_else(|| {
                response.find('[').and_then(|start| {
                    response
                        .rfind(']')
                        .and_then(|end| serde_json::from_str(&response[start..=end]).ok())
                })
            });

        if let Some(results) = results {
            for result in results {
                if let Some(&id) = title_to_id.get(result.entity.as_str()) {
                    if id != source_id && !links.iter().any(|l| l.target_id == id) {
                        links.push(DetectedLink {
                            target_id: id.to_string(),
                            target_title: result.entity.clone(),
                            mention_text: result.entity,
                            context: result.context,
                        });
                    }
                }
            }
        }

        Ok(links)
    }

    /// Batch detect links for multiple documents in a single LLM call.
    /// Returns a HashMap of source_id -> `Vec<DetectedLink>`.
    pub async fn detect_links_batch(
        &self,
        documents: &[(&str, &str, &str)], // (id, title, content)
        known_entities: &[(String, String)],
    ) -> Result<HashMap<String, Vec<DetectedLink>>, FactbaseError> {
        let mut results: HashMap<String, Vec<DetectedLink>> = HashMap::new();

        // Build lookup map for O(1) access
        let id_to_title: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (id.as_str(), title.as_str()))
            .collect();

        // Initialize results and extract manual links
        for (id, _, content) in documents {
            // Most documents have few links; 4 is a reasonable default
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
            results.insert(id.to_string(), links);
        }

        if known_entities.is_empty() || documents.is_empty() {
            return Ok(results);
        }

        // Fuzzy string pre-filter: catch obvious matches before LLM
        let candidates = build_match_candidates(known_entities);
        let mut all_matched_ids: HashSet<String> = HashSet::new();
        for (id, _, content) in documents {
            let (string_links, matched_ids) = string_match_links(content, id, &candidates);
            if let Some(links) = results.get_mut(*id) {
                for sl in string_links {
                    if !links.iter().any(|l| l.target_id == sl.target_id) {
                        links.push(sl);
                    }
                }
            }
            all_matched_ids.extend(matched_ids);
        }

        // Build entities list (excluding docs being processed and pre-matched entities)
        let doc_ids: HashSet<&str> = documents.iter().map(|(id, _, _)| *id).collect();
        let entities_list: String = known_entities
            .iter()
            .filter(|(id, _)| !doc_ids.contains(id.as_str()) && !all_matched_ids.contains(id))
            .map(|(id, title)| format!("- {title} (id: {id})"))
            .collect::<Vec<_>>()
            .join("\n");

        if entities_list.is_empty() {
            return Ok(results);
        }

        // Build batch prompt
        // Use configured max_content_length per doc
        let max_content_len = self.max_content_length;
        let docs_section: String = documents
            .iter()
            .map(|(id, title, content)| {
                let truncated = if content.len() > max_content_len {
                    &content[..max_content_len]
                } else {
                    content
                };
                format!("=== DOC_ID: {id} ===\nTitle: {title}\n{truncated}")
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompts = crate::Config::load(None).unwrap_or_default().prompts;
        let prompt = crate::config::prompts::resolve_prompt(
            &prompts,
            "link_detect_batch",
            DEFAULT_LINK_DETECT_BATCH_PROMPT,
            &[("entities_list", &entities_list), ("docs_section", &docs_section)],
        );

        let response = self.llm.complete(&prompt).await?;

        // Build title->id lookup for merge
        let title_to_id: HashMap<&str, &str> = known_entities
            .iter()
            .map(|(id, title)| (title.as_str(), id.as_str()))
            .collect();

        // Parse batch response - try direct parse first, then extract from text
        let batch: Option<HashMap<String, Vec<LlmLinkResult>>> =
            serde_json::from_str(&response).ok().or_else(|| {
                response.find('{').and_then(|start| {
                    response
                        .rfind('}')
                        .and_then(|end| serde_json::from_str(&response[start..=end]).ok())
                })
            });

        if let Some(batch) = batch {
            Self::merge_batch_results(&mut results, batch, &title_to_id);
        }

        Ok(results)
    }

    fn merge_batch_results(
        results: &mut HashMap<String, Vec<DetectedLink>>,
        batch: HashMap<String, Vec<LlmLinkResult>>,
        title_to_id: &HashMap<&str, &str>,
    ) {
        for (doc_id, llm_links) in batch {
            if let Some(links) = results.get_mut(&doc_id) {
                for result in llm_links {
                    if let Some(&id) = title_to_id.get(result.entity.as_str()) {
                        if id != doc_id && !links.iter().any(|l| l.target_id == id) {
                            links.push(DetectedLink {
                                target_id: id.to_string(),
                                target_title: result.entity.clone(),
                                mention_text: result.entity,
                                context: result.context,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::test_helpers::MockLlm;

    #[tokio::test]
    async fn test_link_detector_manual_links() {
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "abc123");
        assert_eq!(links[0].target_title, "Test Doc");
    }

    #[tokio::test]
    async fn test_link_detector_filters_self_references() {
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        // source_id matches the link - should be filtered
        let links = detector
            .detect_links(content, "abc123", &known)
            .await
            .expect("detect_links should succeed");
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_llm_json_response() {
        let mock = MockLlm::new(r#"[{"entity": "John Doe", "context": "met with John Doe"}]"#);
        let detector = LinkDetector::new(Box::new(mock));

        let content = "I met with John Doe yesterday.";
        let known = vec![("def456".to_string(), "John Doe".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "def456");
        assert_eq!(links[0].target_title, "John Doe");
        // Pre-filter catches this match; context is the surrounding sentence
        assert!(links[0].context.contains("John Doe"));
    }

    #[tokio::test]
    async fn test_link_detector_extracts_json_from_text() {
        let mock = MockLlm::new(
            r#"Here are the results: [{"entity": "Project X", "context": "working on Project X"}]"#,
        );
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Currently working on Project X.";
        let known = vec![("proj01".to_string(), "Project X".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "proj01");
    }

    #[tokio::test]
    async fn test_link_detector_handles_malformed_json() {
        let mock = MockLlm::new("This is not valid JSON at all");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Some content.";
        let known = vec![("abc123".to_string(), "Test".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed even with malformed JSON");
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_empty_entities() {
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Some content.";
        let known: Vec<(String, String)> = vec![];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed with empty entities");
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_deduplicates() {
        let mock = MockLlm::new(
            r#"[{"entity": "Test Doc", "context": "first mention"}, {"entity": "Test Doc", "context": "second mention"}]"#,
        );
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Test Doc mentioned twice.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

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

    #[tokio::test]
    async fn test_detect_links_batch() {
        let mock = MockLlm::new(
            r#"{"doc1": [{"entity": "John Doe", "context": "met John"}], "doc2": []}"#,
        );
        let detector = LinkDetector::new(Box::new(mock));

        let docs = vec![
            ("doc1", "Doc One", "I met John Doe."),
            ("doc2", "Doc Two", "No mentions here."),
        ];
        let known = vec![("person1".to_string(), "John Doe".to_string())];

        let results = detector
            .detect_links_batch(&docs, &known)
            .await
            .expect("detect_links_batch should succeed");

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

    #[tokio::test]
    async fn test_detect_links_batch_with_manual_links() {
        let mock = MockLlm::new(r#"{}"#);
        let detector = LinkDetector::new(Box::new(mock));

        let docs = vec![("doc1", "Doc One", "See [[abc123]] for details.")];
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let results = detector
            .detect_links_batch(&docs, &known)
            .await
            .expect("detect_links_batch should succeed");

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

    #[tokio::test]
    async fn test_manual_link_unknown_id_ignored() {
        // Manual link [[xyz123]] where xyz123 is NOT in known_entities
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[xyz123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

        // xyz123 not in known_entities, so no links detected
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_manual_links() {
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[abc123]] and [[def456]] for details.";
        let known = vec![
            ("abc123".to_string(), "Doc A".to_string()),
            ("def456".to_string(), "Doc B".to_string()),
        ];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .expect("detect_links should succeed");

        assert_eq!(links.len(), 2);
        let ids: Vec<&str> = links.iter().map(|l| l.target_id.as_str()).collect();
        assert!(ids.contains(&"abc123"));
        assert!(ids.contains(&"def456"));
    }

    #[tokio::test]
    async fn test_batch_filters_self_references() {
        let mock = MockLlm::new(r#"{}"#);
        let detector = LinkDetector::new(Box::new(mock));

        // doc1 links to itself via [[doc001]]
        let docs = vec![("doc001", "Doc One", "See [[doc001]] for self-ref.")];
        let known = vec![("doc001".to_string(), "Doc One".to_string())];

        let results = detector
            .detect_links_batch(&docs, &known)
            .await
            .expect("detect_links_batch should succeed");

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
        let candidates = build_match_candidates(&entities);
        // Should have full title + unique words + abbreviation
        assert!(candidates.iter().any(|c| c.regex.is_match("Delta Air Lines")));
    }

    #[test]
    fn test_string_match_exact_title() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities);
        let (links, matched) = string_match_links(
            "We flew Delta Air Lines to NYC.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
        assert!(matched.contains("e1"));
    }

    #[test]
    fn test_string_match_case_insensitive() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "We flew delta air lines to NYC.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_unique_word() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "The Delta subsidiary expanded.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_word_boundary() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities);
        // "Deltaforce" should NOT match "Delta" as a word
        let (links, _) = string_match_links(
            "The Deltaforce team arrived.",
            "src",
            &candidates,
        );
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_ambiguous_word_excluded() {
        // "Mount" appears in both entities — should not be used for matching
        let entities = vec![
            ("e1".into(), "Mount Vesuvius".into()),
            ("e2".into(), "Mount St. Helens".into()),
        ];
        let candidates = build_match_candidates(&entities);
        // "Mount" alone should not match either entity
        let (links, _) = string_match_links(
            "The mount was visible from afar.",
            "src",
            &candidates,
        );
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_abbreviation() {
        let entities = vec![("e1".into(), "Delta Air Lines".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "DAL stock rose 5% today.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "e1");
    }

    #[test]
    fn test_string_match_abbreviation_needs_3_chars() {
        // Two-word title → abbreviation "AB" is only 2 chars, should not match
        let entities = vec![("e1".into(), "Acme Bricks".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links("AB Corp is here.", "src", &candidates);
        // Should not match via abbreviation (only 2 chars)
        // May match via unique word "Acme" or "Bricks" though
        let abbrev_match = links.iter().any(|l| l.mention_text == "AB");
        assert!(!abbrev_match);
    }

    #[test]
    fn test_string_match_skips_self() {
        let entities = vec![("src".into(), "Self Document".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "This is the Self Document itself.",
            "src",
            &candidates,
        );
        assert!(links.is_empty());
    }

    #[test]
    fn test_string_match_no_duplicate_links() {
        // Entity matches via both full title and unique word — should only appear once
        let entities = vec![("e1".into(), "Vesuvius".into())];
        let candidates = build_match_candidates(&entities);
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
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "First line.\nWe partnered with Acme Corporation last year.\nThird line.",
            "src",
            &candidates,
        );
        assert_eq!(links.len(), 1);
        assert!(links[0].context.contains("Acme Corporation"));
    }

    #[tokio::test]
    async fn test_prefilter_reduces_llm_entities() {
        // The mock LLM returns empty — all matches should come from pre-filter
        let mock = MockLlm::new("[]");
        let detector = LinkDetector::new(Box::new(mock));

        let content = "We met with Delta Air Lines representatives.";
        let known = vec![
            ("e1".to_string(), "Delta Air Lines".to_string()),
            ("e2".to_string(), "Unrelated Corp".to_string()),
        ];

        let links = detector
            .detect_links(content, "src", &known)
            .await
            .expect("should succeed");

        // Delta Air Lines found by pre-filter
        assert!(links.iter().any(|l| l.target_id == "e1"));
        // Unrelated Corp not in content
        assert!(!links.iter().any(|l| l.target_id == "e2"));
    }

    #[tokio::test]
    async fn test_prefilter_merges_with_llm() {
        // LLM finds an indirect reference the pre-filter can't
        let mock = MockLlm::new(
            r#"[{"entity": "Unrelated Corp", "context": "the parent company"}]"#,
        );
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Delta Air Lines is owned by the parent company.";
        let known = vec![
            ("e1".to_string(), "Delta Air Lines".to_string()),
            ("e2".to_string(), "Unrelated Corp".to_string()),
        ];

        let links = detector
            .detect_links(content, "src", &known)
            .await
            .expect("should succeed");

        // Both should be found: e1 by pre-filter, e2 by LLM
        assert_eq!(links.len(), 2);
    }

    #[tokio::test]
    async fn test_batch_prefilter() {
        let mock = MockLlm::new(r#"{}"#);
        let detector = LinkDetector::new(Box::new(mock));

        let docs = vec![
            ("doc1", "Doc One", "We visited Mount Vesuvius."),
            ("doc2", "Doc Two", "No entities here."),
        ];
        let known = vec![("e1".to_string(), "Mount Vesuvius".to_string())];

        let results = detector
            .detect_links_batch(&docs, &known)
            .await
            .expect("should succeed");

        assert_eq!(results.get("doc1").unwrap().len(), 1);
        assert_eq!(results.get("doc1").unwrap()[0].target_id, "e1");
        assert!(results.get("doc2").unwrap().is_empty());
    }

    #[test]
    fn test_stop_words_excluded() {
        // "Great" is a stop word — should not match as a single word
        let entities = vec![("e1".into(), "The Great Wall".into())];
        let candidates = build_match_candidates(&entities);
        let (links, _) = string_match_links(
            "It was a great achievement.",
            "src",
            &candidates,
        );
        assert!(links.is_empty());
    }
}
