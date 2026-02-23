//! Cross-document fact validation.
//!
//! Validates facts in a document against the rest of the factbase using
//! per-fact semantic search and LLM-based conflict/staleness detection.

use serde::Deserialize;
use tracing::warn;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::models::{QuestionType, ReviewQuestion, SearchResult};

use super::facts::{extract_all_facts, FactLine};

/// Minimum similarity score for a search result to be considered relevant.
const RELEVANCE_THRESHOLD: f32 = 0.3;

/// Maximum facts per LLM batch call.
const BATCH_SIZE: usize = 10;

/// Maximum snippet length in prompt to avoid huge prompts.
const MAX_SNIPPET_LEN: usize = 200;

/// A fact paired with its cross-document search results.
struct FactWithContext {
    fact: FactLine,
    related: Vec<SearchResult>,
}

/// Parsed LLM response for a single fact classification.
#[derive(Deserialize)]
struct CrossCheckResult {
    fact: usize,
    status: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    source_doc: String,
}

/// Extract document title from content (first `# ` heading) or fall back to doc_id.
fn extract_title(content: &str, doc_id: &str) -> String {
    content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l[2..].trim().to_string())
        .unwrap_or_else(|| doc_id.to_string())
}

/// Build the LLM prompt for a batch of facts with their cross-document context.
fn build_prompt(doc_title: &str, batch: &[&FactWithContext]) -> String {
    let mut prompt = String::from(
        "You are validating facts from a knowledge base document. For each fact below, \
         I've included relevant information from other documents in the knowledge base.\n\n\
         Determine if each fact is:\n\
         - CONSISTENT: agrees with or is not contradicted by other sources\n\
         - CONFLICT: directly contradicts information in another document\n\
         - STALE: may have been true but other sources suggest it's no longer current\n\
         - UNCERTAIN: insufficient information to validate\n\n\
         For CONFLICT and STALE, cite the specific document and fact that disagrees.\n\n",
    );

    prompt.push_str(&format!("Document: {doc_title}\n---\n"));

    for (i, fwc) in batch.iter().enumerate() {
        let idx = i + 1;
        prompt.push_str(&format!(
            "Fact {idx} (line {}): \"{}\"\nRelated information:\n",
            fwc.fact.line_number, fwc.fact.text
        ));
        for r in &fwc.related {
            let snip = if r.snippet.len() > MAX_SNIPPET_LEN {
                format!("{}...", &r.snippet[..MAX_SNIPPET_LEN])
            } else {
                r.snippet.clone()
            };
            prompt.push_str(&format!("- [{}] \"{}\"\n", r.title, snip));
        }
        prompt.push('\n');
    }

    prompt.push_str(
        "---\n\nRespond ONLY with a JSON array. Each element must have: \
         fact (number), status (CONSISTENT/CONFLICT/STALE/UNCERTAIN), \
         reason (string), source_doc (string, empty if N/A).\n",
    );
    prompt
}

/// Parse the LLM JSON response, tolerating markdown fences and partial output.
fn parse_llm_response(response: &str) -> Vec<CrossCheckResult> {
    // Strip markdown code fences if present
    let trimmed = response.trim();
    let json_str = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };

    match serde_json::from_str::<Vec<CrossCheckResult>>(json_str) {
        Ok(results) => results,
        Err(e) => {
            warn!("Failed to parse cross-validation LLM response: {e}");
            Vec::new()
        }
    }
}

/// Convert a CrossCheckResult into a ReviewQuestion, if actionable.
fn result_to_question(
    result: &CrossCheckResult,
    batch: &[&FactWithContext],
) -> Option<ReviewQuestion> {
    // fact index is 1-based in the prompt
    let fwc = batch.get(result.fact.checked_sub(1)?)?;

    let qtype = match result.status.to_uppercase().as_str() {
        "CONFLICT" => QuestionType::Conflict,
        "STALE" => QuestionType::Stale,
        _ => return None, // CONSISTENT and UNCERTAIN produce no questions
    };

    let desc = if result.source_doc.is_empty() {
        format!("Cross-check: {} — {}", fwc.fact.text, result.reason)
    } else {
        format!(
            "Cross-check with {}: {} — {}",
            result.source_doc, fwc.fact.text, result.reason
        )
    };

    Some(ReviewQuestion::new(qtype, Some(fwc.fact.line_number), desc))
}

/// Validate facts in a document against the rest of the factbase.
///
/// For each fact line, generates an embedding, searches for related documents,
/// and uses the LLM to detect conflicts or stale information.
pub async fn cross_validate_document(
    content: &str,
    doc_id: &str,
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: &dyn LlmProvider,
) -> Result<Vec<ReviewQuestion>, FactbaseError> {
    let facts = extract_all_facts(content);
    if facts.is_empty() {
        return Ok(Vec::new());
    }

    let mut facts_with_context = Vec::with_capacity(facts.len());

    for fact in facts {
        let fact_embedding = embedding.generate(&fact.text).await?;
        let search_results =
            db.search_semantic_paginated(&fact_embedding, 10, 0, None, None, Some(&fact.text))?;

        let related: Vec<_> = search_results
            .results
            .into_iter()
            .filter(|r| r.id != doc_id)
            .filter(|r| r.relevance_score >= RELEVANCE_THRESHOLD)
            .collect();

        if related.is_empty() {
            continue;
        }

        facts_with_context.push(FactWithContext { fact, related });
    }

    if facts_with_context.is_empty() {
        return Ok(Vec::new());
    }

    let doc_title = extract_title(content, doc_id);
    let mut questions = Vec::new();

    for chunk in facts_with_context.chunks(BATCH_SIZE) {
        let batch: Vec<&FactWithContext> = chunk.iter().collect();
        let prompt = build_prompt(&doc_title, &batch);

        let response = match llm.complete(&prompt).await {
            Ok(r) => r,
            Err(e) => {
                warn!("LLM call failed during cross-validation: {e}");
                continue;
            }
        };

        let results = parse_llm_response(&response);
        for r in &results {
            if let Some(q) = result_to_question(r, &batch) {
                questions.push(q);
            }
        }
    }

    Ok(questions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use std::future::Future;
    use std::pin::Pin;

    type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

    struct MockLlm;
    impl LlmProvider for MockLlm {
        fn complete<'a>(
            &'a self,
            _prompt: &'a str,
        ) -> BoxFuture<'a, Result<String, FactbaseError>> {
            Box::pin(async { Ok("[]".into()) })
        }
    }

    #[tokio::test]
    async fn test_empty_content_returns_no_questions() {
        let (db, _tmp) = test_db();
        let questions = cross_validate_document("", "abc123", &db, &MockEmbedding::new(1024), &MockLlm)
            .await
            .unwrap();
        assert!(questions.is_empty());
    }

    #[tokio::test]
    async fn test_no_list_items_returns_no_questions() {
        let (db, _tmp) = test_db();
        let content = "# Title\n\nJust paragraphs here.";
        let questions = cross_validate_document(content, "abc123", &db, &MockEmbedding::new(1024), &MockLlm)
            .await
            .unwrap();
        assert!(questions.is_empty());
    }

    #[tokio::test]
    async fn test_no_relevant_results_returns_no_questions() {
        let (db, _tmp) = test_db();
        let content = "# Person\n\n- VP Engineering at Acme\n- Based in Seattle";
        let questions = cross_validate_document(content, "abc123", &db, &MockEmbedding::new(1024), &MockLlm)
            .await
            .unwrap();
        assert!(questions.is_empty());
    }

    #[test]
    fn test_extract_title_from_heading() {
        assert_eq!(extract_title("# My Doc\n\nContent", "abc"), "My Doc");
    }

    #[test]
    fn test_extract_title_fallback_to_id() {
        assert_eq!(extract_title("No heading here", "abc123"), "abc123");
    }

    #[test]
    fn test_build_prompt_contains_facts() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 5,
                text: "VP at Acme".into(),
                section: None,
            },
            related: vec![SearchResult {
                id: "def456".into(),
                title: "Jane Smith".into(),
                doc_type: None,
                file_path: "people/jane.md".into(),
                relevance_score: 0.8,
                snippet: "Left Acme in 2024".into(),
                highlighted_snippet: None,
                chunk_index: None,
                chunk_start: None,
                chunk_end: None,
            }],
        };
        let prompt = build_prompt("Acme Corp", &[&fwc]);
        assert!(prompt.contains("Fact 1 (line 5)"));
        assert!(prompt.contains("VP at Acme"));
        assert!(prompt.contains("[Jane Smith]"));
        assert!(prompt.contains("Left Acme in 2024"));
    }

    #[test]
    fn test_parse_llm_response_valid_json() {
        let json = r#"[{"fact":1,"status":"CONFLICT","reason":"outdated","source_doc":"jane"}]"#;
        let results = parse_llm_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "CONFLICT");
    }

    #[test]
    fn test_parse_llm_response_with_fences() {
        let json = "```json\n[{\"fact\":1,\"status\":\"STALE\",\"reason\":\"old\",\"source_doc\":\"x\"}]\n```";
        let results = parse_llm_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "STALE");
    }

    #[test]
    fn test_parse_llm_response_malformed() {
        let results = parse_llm_response("not json at all");
        assert!(results.is_empty());
    }

    #[test]
    fn test_result_to_question_conflict() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 10,
                text: "VP at Acme".into(),
                section: None,
            },
            related: vec![],
        };
        let r = CrossCheckResult {
            fact: 1,
            status: "CONFLICT".into(),
            reason: "Jane left Acme".into(),
            source_doc: "jane-smith".into(),
        };
        let q = result_to_question(&r, &[&fwc]).unwrap();
        assert_eq!(q.question_type, QuestionType::Conflict);
        assert_eq!(q.line_ref, Some(10));
        assert!(q.description.contains("jane-smith"));
    }

    #[test]
    fn test_result_to_question_stale() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 3,
                text: "Based in Seattle".into(),
                section: None,
            },
            related: vec![],
        };
        let r = CrossCheckResult {
            fact: 1,
            status: "STALE".into(),
            reason: "Relocated to Austin".into(),
            source_doc: "".into(),
        };
        let q = result_to_question(&r, &[&fwc]).unwrap();
        assert_eq!(q.question_type, QuestionType::Stale);
        assert!(q.description.contains("Based in Seattle"));
    }

    #[test]
    fn test_result_to_question_consistent_returns_none() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 1,
                text: "fact".into(),
                section: None,
            },
            related: vec![],
        };
        let r = CrossCheckResult {
            fact: 1,
            status: "CONSISTENT".into(),
            reason: "".into(),
            source_doc: "".into(),
        };
        assert!(result_to_question(&r, &[&fwc]).is_none());
    }

    #[test]
    fn test_result_to_question_uncertain_returns_none() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 1,
                text: "fact".into(),
                section: None,
            },
            related: vec![],
        };
        let r = CrossCheckResult {
            fact: 1,
            status: "UNCERTAIN".into(),
            reason: "".into(),
            source_doc: "".into(),
        };
        assert!(result_to_question(&r, &[&fwc]).is_none());
    }

    #[test]
    fn test_result_to_question_invalid_fact_index() {
        let r = CrossCheckResult {
            fact: 5,
            status: "CONFLICT".into(),
            reason: "bad".into(),
            source_doc: "".into(),
        };
        assert!(result_to_question(&r, &[]).is_none());
    }

    #[test]
    fn test_result_to_question_zero_fact_index() {
        let r = CrossCheckResult {
            fact: 0,
            status: "CONFLICT".into(),
            reason: "bad".into(),
            source_doc: "".into(),
        };
        // fact 0 with checked_sub(1) returns None
        assert!(result_to_question(&r, &[]).is_none());
    }
}
