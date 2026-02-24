//! Link detection service for entity mentions.

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

        // Build prompt for LLM
        let entities_list: String = known_entities
            .iter()
            .filter(|(id, _)| id != source_id)
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

        // Build entities list (excluding docs being processed)
        let doc_ids: HashSet<&str> = documents.iter().map(|(id, _, _)| *id).collect();
        let entities_list: String = known_entities
            .iter()
            .filter(|(id, _)| !doc_ids.contains(id.as_str()))
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
        assert_eq!(links[0].context, "met with John Doe");
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
}
