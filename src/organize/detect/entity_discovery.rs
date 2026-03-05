//! Entity discovery: detect frequently-mentioned names without their own document.
//!
//! Scans documents via LLM to find proper nouns and named concepts that appear
//! across multiple documents but don't have a dedicated entity document yet.

use crate::config::prompts::resolve_prompt;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::models::{Document, Perspective};
use crate::progress::ProgressReporter;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tracing::warn;

/// A suggested entity discovered across multiple documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedEntity {
    /// The candidate entity name.
    pub name: String,
    /// Suggested document type (from perspective classification), if available.
    pub suggested_type: Option<String>,
    /// Document IDs where this entity was mentioned.
    pub mentioned_in: Vec<String>,
    /// Confidence level: "high" or "medium".
    pub confidence: String,
}

const DEFAULT_ENTITY_DISCOVER_PROMPT: &str = "\
List all proper nouns, organization names, place names, and significant named concepts \
in this document that could be their own knowledge base entry. \
Exclude: the document's own title, generic terms, adjectives, common words.\n\n\
{content}\n\n\
Respond ONLY with a JSON array of strings. Example: [\"Acme Corp\", \"Springfield\", \"Project Orion\"]\n";

const DEFAULT_ENTITY_CLASSIFY_PROMPT: &str = "\
Given these knowledge base document types: {types_list}\n\n\
Classify each candidate entity name into the most appropriate type. \
Return confidence=low if the name doesn't clearly fit any type.\n\n\
Candidates: {candidates}\n\n\
Respond ONLY with a JSON array. Each element: {{\"name\": \"...\", \"type\": \"...\", \"confidence\": \"high|medium|low\"}}\n";

/// Maximum content length sent per document in the extraction prompt.
const MAX_CONTENT_LEN: usize = 40_000;

/// Batch size for LLM extraction calls.
const BATCH_SIZE: usize = 5;

/// Discover entities mentioned across documents that lack their own document.
///
/// Requires an LLM provider. Only returns candidates mentioned in >= 2 documents.
/// Returns `(suggestions, docs_processed)` so callers can track progress for resumption.
pub async fn discover_entities(
    docs: &[Document],
    existing_titles: &[String],
    llm: &dyn LlmProvider,
    perspective: Option<&Perspective>,
    progress: &ProgressReporter,
    doc_offset: usize,
    deadline: Option<Instant>,
) -> Result<(Vec<SuggestedEntity>, usize), FactbaseError> {
    let existing_lower: HashSet<String> = existing_titles.iter().map(|t| t.to_lowercase()).collect();

    let prompts = crate::Config::load(None).unwrap_or_default().prompts;

    // Phase 1: Extract candidate names from each document via LLM
    progress.phase("Discovering entity candidates");
    let mut name_to_docs: HashMap<String, Vec<String>> = HashMap::new();
    let remaining_docs = if doc_offset < docs.len() { &docs[doc_offset..] } else { &[] };
    let mut docs_processed: usize = 0;
    let mut deadline_hit = false;

    for (i, batch) in remaining_docs.chunks(BATCH_SIZE).enumerate() {
        if let Some(d) = deadline {
            if Instant::now() > d {
                deadline_hit = true;
                break;
            }
        }
        progress.report(
            doc_offset + i * BATCH_SIZE,
            docs.len(),
            "Extracting entity candidates",
        );

        for doc in batch {
            let content = &doc.content;
            if content.len() < 50 {
                docs_processed += 1;
                continue;
            }
            let truncated = if content.len() > MAX_CONTENT_LEN {
                &content[..MAX_CONTENT_LEN]
            } else {
                content
            };

            let prompt = resolve_prompt(
                &prompts,
                "entity_discover",
                DEFAULT_ENTITY_DISCOVER_PROMPT,
                &[("content", truncated)],
            );

            let response = match llm.complete(&prompt).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("Entity discovery LLM call failed for {}: {e}", doc.id);
                    docs_processed += 1;
                    continue;
                }
            };

            let candidates: Vec<String> = parse_string_array(&response);
            let doc_title_lower = doc.title.to_lowercase();

            for name in candidates {
                let key = name.trim().to_lowercase();
                if key.is_empty() || key == doc_title_lower || existing_lower.contains(&key) {
                    continue;
                }
                name_to_docs
                    .entry(key)
                    .or_default()
                    .push(doc.id.clone());
            }
            docs_processed += 1;
        }
    }

    // If deadline hit before processing all docs, return partial results
    if deadline_hit {
        return Ok((Vec::new(), docs_processed));
    }

    // Phase 2: Filter to candidates mentioned in >= 2 documents
    let frequent: Vec<(String, Vec<String>)> = name_to_docs
        .into_iter()
        .filter(|(_, doc_ids)| doc_ids.len() >= 2)
        .collect();

    if frequent.is_empty() {
        return Ok((Vec::new(), docs_processed));
    }

    // Phase 3: Classify candidates if perspective has allowed_types
    let allowed_types = perspective.and_then(|p| p.allowed_types.as_ref());

    let results = if let Some(types) = allowed_types {
        classify_candidates(&frequent, types, llm, &prompts, progress).await?
    } else {
        // No type classification — return all as high confidence
        frequent
            .into_iter()
            .map(|(name, doc_ids)| SuggestedEntity {
                name: capitalize_first(&name),
                suggested_type: None,
                mentioned_in: dedup_vec(doc_ids),
                confidence: "high".to_string(),
            })
            .collect()
    };

    Ok((results, docs_processed))
}

/// Classify candidates against allowed_types via LLM, filtering out low confidence.
async fn classify_candidates(
    candidates: &[(String, Vec<String>)],
    types: &[String],
    llm: &dyn LlmProvider,
    prompts: &crate::PromptsConfig,
    progress: &ProgressReporter,
) -> Result<Vec<SuggestedEntity>, FactbaseError> {
    progress.phase("Classifying entity candidates");

    let types_list = types.join(", ");
    let candidate_names: Vec<&str> = candidates.iter().map(|(n, _)| n.as_str()).collect();
    let candidates_str = candidate_names.join(", ");

    let prompt = resolve_prompt(
        prompts,
        "entity_classify",
        DEFAULT_ENTITY_CLASSIFY_PROMPT,
        &[("types_list", &types_list), ("candidates", &candidates_str)],
    );

    let response = match llm.complete(&prompt).await {
        Ok(r) => r,
        Err(e) => {
            warn!("Entity classification LLM call failed: {e}");
            // Fall back to unclassified
            return Ok(candidates
                .iter()
                .map(|(name, doc_ids)| SuggestedEntity {
                    name: capitalize_first(name),
                    suggested_type: None,
                    mentioned_in: dedup_vec(doc_ids.clone()),
                    confidence: "medium".to_string(),
                })
                .collect());
        }
    };

    // Parse classification response
    let classifications: Vec<ClassifyResult> = parse_classify_response(&response);
    let classify_map: HashMap<String, ClassifyResult> = classifications
        .into_iter()
        .map(|c| (c.name.to_lowercase(), c))
        .collect();

    let mut results = Vec::new();
    for (name, doc_ids) in candidates {
        let (suggested_type, confidence) = if let Some(c) = classify_map.get(name) {
            if c.confidence == "low" {
                continue; // Skip low confidence
            }
            (Some(c.r#type.clone()), c.confidence.clone())
        } else {
            (None, "medium".to_string())
        };

        results.push(SuggestedEntity {
            name: capitalize_first(name),
            suggested_type,
            mentioned_in: dedup_vec(doc_ids.clone()),
            confidence,
        });
    }

    Ok(results)
}

#[derive(Debug, Deserialize)]
struct ClassifyResult {
    name: String,
    r#type: String,
    confidence: String,
}

/// Parse a JSON array of strings from LLM response, tolerating markdown fences.
fn parse_string_array(response: &str) -> Vec<String> {
    let trimmed = crate::patterns::strip_markdown_fences(response);
    serde_json::from_str::<Vec<String>>(trimmed)
        .ok()
        .or_else(|| {
            trimmed.find('[').and_then(|start| {
                trimmed
                    .rfind(']')
                    .and_then(|end| serde_json::from_str(&trimmed[start..=end]).ok())
            })
        })
        .unwrap_or_default()
}

/// Parse classification JSON from LLM response.
fn parse_classify_response(response: &str) -> Vec<ClassifyResult> {
    let trimmed = crate::patterns::strip_markdown_fences(response);
    serde_json::from_str::<Vec<ClassifyResult>>(trimmed)
        .ok()
        .or_else(|| {
            trimmed.find('[').and_then(|start| {
                trimmed
                    .rfind(']')
                    .and_then(|end| serde_json::from_str(&trimmed[start..=end]).ok())
            })
        })
        .unwrap_or_default()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn dedup_vec(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::test_helpers::MockLlm;
    use crate::models::Document;
    use crate::progress::ProgressReporter;

    fn make_doc(id: &str, title: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            doc_type: Some("note".to_string()),
            repo_id: "test".to_string(),
            file_path: format!("{id}.md"),
            file_hash: String::new(),
            is_deleted: false,
            file_modified_at: None,
            indexed_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_discover_entities_basic() {
        // LLM returns different candidates for different docs, with overlap
        let llm = MockLlm::new(r#"["Acme Corp", "Springfield"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Acme Corp is based in Springfield and does things."),
            make_doc("d2", "Doc Two", "Springfield hosts Acme Corp headquarters and more stuff here."),
        ];
        let existing = vec!["Doc One".to_string(), "Doc Two".to_string()];
        let progress = ProgressReporter::Silent;

        let (results, processed) = discover_entities(&docs, &existing, &llm, None, &progress, 0, None)
            .await
            .unwrap();

        // Both candidates appear in both docs
        assert_eq!(results.len(), 2);
        assert_eq!(processed, 2);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Acme corp"));
        assert!(names.contains(&"Springfield"));
        for r in &results {
            assert_eq!(r.mentioned_in.len(), 2);
            assert_eq!(r.confidence, "high");
            assert!(r.suggested_type.is_none());
        }
    }

    #[tokio::test]
    async fn test_discover_filters_existing_entities() {
        let llm = MockLlm::new(r#"["Existing Entity", "New Entity"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Existing Entity and New Entity are mentioned here in this document."),
            make_doc("d2", "Doc Two", "New Entity and Existing Entity appear here too in this other document."),
        ];
        let existing = vec![
            "Doc One".to_string(),
            "Doc Two".to_string(),
            "Existing Entity".to_string(),
        ];
        let progress = ProgressReporter::Silent;

        let (results, _) = discover_entities(&docs, &existing, &llm, None, &progress, 0, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "New entity");
    }

    #[tokio::test]
    async fn test_discover_requires_frequency_two() {
        // Only one doc, so no candidate can reach frequency 2
        let llm = MockLlm::new(r#"["Lonely Entity"]"#);
        let docs = vec![make_doc("d1", "Doc One", "Lonely Entity is mentioned only once in this document content.")];
        let existing = vec!["Doc One".to_string()];
        let progress = ProgressReporter::Silent;

        let (results, _) = discover_entities(&docs, &existing, &llm, None, &progress, 0, None)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_discover_skips_short_content() {
        let llm = MockLlm::new(r#"["Something"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Short"),
            make_doc("d2", "Doc Two", "Also short"),
        ];
        let existing = vec![];
        let progress = ProgressReporter::Silent;

        let (results, _) = discover_entities(&docs, &existing, &llm, None, &progress, 0, None)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_discover_excludes_own_title() {
        let llm = MockLlm::new(r#"["Doc One", "Real Entity"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Doc One mentions Real Entity in this longer document content."),
            make_doc("d2", "Doc Two", "Real Entity and Doc One are both mentioned in this other document."),
        ];
        let existing = vec![];
        let progress = ProgressReporter::Silent;

        let (results, _) = discover_entities(&docs, &existing, &llm, None, &progress, 0, None)
            .await
            .unwrap();

        // "Doc One" appears in both docs but is a doc title for d1, so it gets filtered
        // when processing d1. But MockLlm returns same response for d2, so "Doc One"
        // only gets one doc_id (d2). "Real Entity" gets both.
        let real = results.iter().find(|r| r.name == "Real entity");
        assert!(real.is_some());
        assert_eq!(real.unwrap().mentioned_in.len(), 2);
    }

    #[tokio::test]
    async fn test_classify_filters_low_confidence() {
        // First call: extraction, second call: classification
        // MockLlm returns same response for all calls, so we test classification separately
        let classify_response = r#"[
            {"name": "Good Entity", "type": "companies", "confidence": "high"},
            {"name": "Bad Entity", "type": "unknown", "confidence": "low"}
        ]"#;
        let prompts = crate::PromptsConfig::default();
        let llm = MockLlm::new(classify_response);
        let candidates = vec![
            ("good entity".to_string(), vec!["d1".to_string(), "d2".to_string()]),
            ("bad entity".to_string(), vec!["d1".to_string(), "d3".to_string()]),
        ];
        let types = vec!["companies".to_string(), "regions".to_string()];
        let progress = ProgressReporter::Silent;

        let results = classify_candidates(&candidates, &types, &llm, &prompts, &progress)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Good entity");
        assert_eq!(results[0].suggested_type.as_deref(), Some("companies"));
        assert_eq!(results[0].confidence, "high");
    }

    #[test]
    fn test_parse_string_array_basic() {
        let result = parse_string_array(r#"["Foo", "Bar"]"#);
        assert_eq!(result, vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_parse_string_array_with_fences() {
        let result = parse_string_array("```json\n[\"Foo\", \"Bar\"]\n```");
        assert_eq!(result, vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_parse_string_array_with_surrounding_text() {
        let result = parse_string_array("Here are the results: [\"Foo\"] and more text");
        assert_eq!(result, vec!["Foo"]);
    }

    #[test]
    fn test_parse_string_array_invalid() {
        let result = parse_string_array("not json at all");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_classify_response() {
        let input = r#"[{"name": "X", "type": "t", "confidence": "high"}]"#;
        let results = parse_classify_response(input);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "X");
    }

    #[test]
    fn test_strip_markdown_fences() {
        use crate::patterns::strip_markdown_fences;
        assert_eq!(strip_markdown_fences("```json\n[]\n```"), "[]");
        assert_eq!(strip_markdown_fences("```\n[]\n```"), "[]");
        assert_eq!(strip_markdown_fences("[]"), "[]");
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("A"), "A");
    }

    #[test]
    fn test_dedup_vec() {
        assert_eq!(
            dedup_vec(vec!["b".into(), "a".into(), "b".into()]),
            vec!["a", "b"]
        );
    }

    #[tokio::test]
    async fn test_discover_entities_respects_deadline() {
        let llm = MockLlm::new(r#"["Acme Corp"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Acme Corp is mentioned in this longer document content."),
            make_doc("d2", "Doc Two", "Acme Corp is also mentioned in this other document content."),
        ];
        let existing = vec![];
        let progress = ProgressReporter::Silent;
        // Deadline already passed
        let deadline = Some(Instant::now() - std::time::Duration::from_secs(1));

        let (results, processed) = discover_entities(&docs, &existing, &llm, None, &progress, 0, deadline)
            .await
            .unwrap();

        assert!(results.is_empty());
        assert_eq!(processed, 0);
    }

    #[tokio::test]
    async fn test_discover_entities_respects_doc_offset() {
        let llm = MockLlm::new(r#"["Acme Corp"]"#);
        let docs = vec![
            make_doc("d1", "Doc One", "Acme Corp is mentioned in this longer document content."),
            make_doc("d2", "Doc Two", "Acme Corp is also mentioned in this other document content."),
        ];
        let existing = vec![];
        let progress = ProgressReporter::Silent;

        // Offset past all docs
        let (results, processed) = discover_entities(&docs, &existing, &llm, None, &progress, 100, None)
            .await
            .unwrap();

        assert!(results.is_empty());
        assert_eq!(processed, 0);
    }
}
