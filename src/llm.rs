use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::FactbaseError;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String, FactbaseError>;
}

pub struct OllamaLlm {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl OllamaLlm {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaLlm {
    async fn complete(&self, prompt: &str) -> Result<String, FactbaseError> {
        let url = format!("{}/api/generate", self.base_url);
        let req = GenerateRequest {
            model: &self.model,
            prompt,
            stream: false,
        };

        let resp = match self.client.post(&url).json(&req).send().await {
            Ok(r) => r,
            Err(_) => {
                eprintln!("Error: Failed to connect to Ollama at {}", self.base_url);
                eprintln!("Ensure Ollama is running: ollama serve");
                std::process::exit(1);
            }
        };

        if !resp.status().is_success() {
            eprintln!("Error: Ollama returned status {}", resp.status());
            eprintln!(
                "Ensure model '{}' is available: ollama pull {}",
                self.model, self.model
            );
            std::process::exit(1);
        }

        let body: GenerateResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error: Failed to parse Ollama response: {}", e);
                std::process::exit(1);
            }
        };

        Ok(body.response)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedLink {
    pub target_id: String,
    pub target_title: String,
    pub mention_text: String,
    pub context: String,
}

pub struct LinkDetector {
    llm: Box<dyn LlmProvider>,
}

#[derive(Deserialize)]
struct LlmLinkResult {
    entity: String,
    context: String,
}

impl LinkDetector {
    pub fn new(llm: Box<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    pub async fn detect_links(
        &self,
        content: &str,
        source_id: &str,
        known_entities: &[(String, String)], // (id, title)
    ) -> Result<Vec<DetectedLink>, FactbaseError> {
        let mut links = Vec::new();

        // Extract manual [[id]] links
        let manual_re = Regex::new(r"\[\[([a-f0-9]{6})\]\]").unwrap();
        for cap in manual_re.captures_iter(content) {
            let target_id = cap[1].to_string();
            if target_id != source_id {
                if let Some((_, title)) = known_entities.iter().find(|(id, _)| id == &target_id) {
                    links.push(DetectedLink {
                        target_id: target_id.clone(),
                        target_title: title.clone(),
                        mention_text: format!("[[{}]]", target_id),
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
            .map(|(id, title)| format!("- {} (id: {})", title, id))
            .collect::<Vec<_>>()
            .join("\n");

        if entities_list.is_empty() {
            return Ok(links);
        }

        let prompt = format!(
            r#"Analyze this document and find mentions of these known entities. Return ONLY a JSON array.

Known entities:
{}

Document:
{}

Return a JSON array of objects with "entity" (exact title from list) and "context" (surrounding text). 
Only include entities that are clearly mentioned. Return [] if none found.
Example: [{{"entity": "John Doe", "context": "met with John Doe yesterday"}}]"#,
            entities_list, content
        );

        let response = self.llm.complete(&prompt).await?;

        // Parse JSON response
        if let Ok(results) = serde_json::from_str::<Vec<LlmLinkResult>>(&response) {
            for result in results {
                if let Some((id, title)) = known_entities.iter().find(|(_, t)| t == &result.entity)
                {
                    if id != source_id && !links.iter().any(|l| l.target_id == *id) {
                        links.push(DetectedLink {
                            target_id: id.clone(),
                            target_title: title.clone(),
                            mention_text: result.entity,
                            context: result.context,
                        });
                    }
                }
            }
        } else {
            // Try to extract JSON from response
            if let Some(start) = response.find('[') {
                if let Some(end) = response.rfind(']') {
                    let json_str = &response[start..=end];
                    if let Ok(results) = serde_json::from_str::<Vec<LlmLinkResult>>(json_str) {
                        for result in results {
                            if let Some((id, title)) =
                                known_entities.iter().find(|(_, t)| t == &result.entity)
                            {
                                if id != source_id && !links.iter().any(|l| l.target_id == *id) {
                                    links.push(DetectedLink {
                                        target_id: id.clone(),
                                        target_title: title.clone(),
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

        Ok(links)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_llm_new() {
        let llm = OllamaLlm::new("http://localhost:11434", "llama3");
        assert_eq!(llm.base_url, "http://localhost:11434");
        assert_eq!(llm.model, "llama3");
    }

    // Mock LLM for testing LinkDetector
    struct MockLlm {
        response: String,
    }

    #[async_trait]
    impl LlmProvider for MockLlm {
        async fn complete(&self, _prompt: &str) -> Result<String, FactbaseError> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn test_link_detector_manual_links() {
        let mock = MockLlm {
            response: "[]".to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "abc123");
        assert_eq!(links[0].target_title, "Test Doc");
    }

    #[tokio::test]
    async fn test_link_detector_filters_self_references() {
        let mock = MockLlm {
            response: "[]".to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "See [[abc123]] for details.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        // source_id matches the link - should be filtered
        let links = detector
            .detect_links(content, "abc123", &known)
            .await
            .unwrap();
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_llm_json_response() {
        let mock = MockLlm {
            response: r#"[{"entity": "John Doe", "context": "met with John Doe"}]"#.to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "I met with John Doe yesterday.";
        let known = vec![("def456".to_string(), "John Doe".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "def456");
        assert_eq!(links[0].target_title, "John Doe");
        assert_eq!(links[0].context, "met with John Doe");
    }

    #[tokio::test]
    async fn test_link_detector_extracts_json_from_text() {
        let mock = MockLlm {
            response: r#"Here are the results: [{"entity": "Project X", "context": "working on Project X"}]"#.to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Currently working on Project X.";
        let known = vec![("proj01".to_string(), "Project X".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "proj01");
    }

    #[tokio::test]
    async fn test_link_detector_handles_malformed_json() {
        let mock = MockLlm {
            response: "This is not valid JSON at all".to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Some content.";
        let known = vec![("abc123".to_string(), "Test".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_empty_entities() {
        let mock = MockLlm {
            response: "[]".to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Some content.";
        let known: Vec<(String, String)> = vec![];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn test_link_detector_deduplicates() {
        let mock = MockLlm {
            response: r#"[{"entity": "Test Doc", "context": "first mention"}, {"entity": "Test Doc", "context": "second mention"}]"#.to_string(),
        };
        let detector = LinkDetector::new(Box::new(mock));

        let content = "Test Doc mentioned twice.";
        let known = vec![("abc123".to_string(), "Test Doc".to_string())];

        let links = detector
            .detect_links(content, "source1", &known)
            .await
            .unwrap();

        // Should only have one link despite two mentions
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_manual_link_regex() {
        let re = Regex::new(r"\[\[([a-f0-9]{6})\]\]").unwrap();

        // Valid links
        assert!(re.is_match("[[abc123]]"));
        assert!(re.is_match("See [[def456]] here"));

        // Invalid links
        assert!(!re.is_match("[[ABC123]]")); // uppercase
        assert!(!re.is_match("[[abc12]]")); // too short
        assert!(!re.is_match("[[abc1234]]")); // too long
        assert!(!re.is_match("[[ghijkl]]")); // invalid hex
    }
}
