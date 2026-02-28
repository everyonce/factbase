//! Cross-document fact validation.
//!
//! Validates facts in a document against the rest of the factbase using
//! per-fact semantic search and LLM-based conflict/staleness detection.
//!
//! Two modes:
//! - **Fact-pair mode** (`cross_validate_facts`): Uses pre-computed fact embeddings
//!   from the DB. Preferred when fact_embeddings table is populated.
//! - **Legacy mode** (`cross_validate_document`): Generates embeddings per-fact at
//!   check time. Used as fallback when fact_embeddings table is empty.

use std::collections::HashMap;

use serde::Deserialize;
use tracing::warn;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::models::{FactPair, QuestionType, ReviewQuestion, SearchResult, TemporalTagType};
use crate::patterns::{SOURCE_REF_CAPTURE_REGEX, TEMPORAL_TAG_CONTENT_REGEX};
use crate::processor::parse_source_definitions;

use super::facts::{extract_all_facts, FactLine};

/// Minimum similarity score for a search result to be considered relevant.
const RELEVANCE_THRESHOLD: f32 = 0.3;

/// Maximum facts per LLM batch call.
const BATCH_SIZE: usize = 10;

/// Maximum snippet length in prompt to avoid huge prompts.
const MAX_SNIPPET_LEN: usize = 200;

/// Maximum fact pairs per LLM batch call (default, overridden by config).
#[cfg(test)]
const PAIR_BATCH_SIZE: usize = 10;

/// A fact paired with its cross-document search results and source context.
struct FactWithContext {
    fact: FactLine,
    related: Vec<SearchResult>,
    /// Source footnote definitions referenced by this fact (e.g., "LinkedIn profile, scraped 2024-01-15").
    source_defs: Vec<String>,
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

// ---------------------------------------------------------------------------
// Fact-pair cross-validation (new approach using pre-computed embeddings)
// ---------------------------------------------------------------------------

/// Enriched fact pair with source context loaded from documents.
struct FactPairContext {
    /// The original fact pair.
    pair: FactPair,
    /// Document title for fact A.
    title_a: String,
    /// Document title for fact B.
    title_b: String,
    /// Source footnote definitions for fact A.
    source_defs_a: Vec<String>,
    /// Source footnote definitions for fact B.
    source_defs_b: Vec<String>,
    /// Temporal tag text on fact A's line (e.g., "@t[2020..2022]").
    temporal_a: Option<String>,
    /// Temporal tag text on fact B's line.
    temporal_b: Option<String>,
    /// Whether fact B's title appears in fact A's document (boost signal).
    title_b_in_doc_a: bool,
    /// Whether fact A's title appears in fact B's document.
    title_a_in_doc_b: bool,
}

/// Parsed LLM response for a fact-pair classification.
#[derive(Deserialize)]
struct PairCheckResult {
    pair: usize,
    status: String,
    #[serde(default)]
    reason: String,
}

/// Cached document context for source loading.
struct DocContext {
    title: String,
    content: String,
    source_map: HashMap<u32, String>,
}

/// Load and cache document context (title, content, source map).
fn load_doc_context(
    doc_id: &str,
    cache: &mut HashMap<String, DocContext>,
    db: &Database,
) -> Option<()> {
    if cache.contains_key(doc_id) {
        return Some(());
    }
    let doc = db.get_document(doc_id).ok()??;
    let source_defs = parse_source_definitions(&doc.content);
    let source_map: HashMap<u32, String> = source_defs
        .into_iter()
        .map(|d| {
            let text = if let Some(date) = &d.date {
                format!("[^{}]: {} {}, {}", d.number, d.source_type, d.context, date)
            } else if d.context.is_empty() {
                format!("[^{}]: {}", d.number, d.source_type)
            } else {
                format!("[^{}]: {} {}", d.number, d.source_type, d.context)
            };
            (d.number, text)
        })
        .collect();
    let title = extract_title(&doc.content, doc_id);
    cache.insert(
        doc_id.to_string(),
        DocContext {
            title,
            content: doc.content,
            source_map,
        },
    );
    Some(())
}

/// Extract source footnote definitions for a fact at a given line number.
fn get_source_defs_for_line(
    content: &str,
    line_number: usize,
    source_map: &HashMap<u32, String>,
) -> Vec<String> {
    let line = content.lines().nth(line_number.saturating_sub(1)).unwrap_or("");
    let refs: Vec<u32> = SOURCE_REF_CAPTURE_REGEX
        .captures_iter(line)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    refs.iter()
        .filter_map(|n| source_map.get(n).cloned())
        .collect()
}

/// Extract temporal tag text from a line (e.g., "@t[2020..2022]").
fn get_temporal_tag_on_line(content: &str, line_number: usize) -> Option<String> {
    let line = content.lines().nth(line_number.saturating_sub(1))?;
    TEMPORAL_TAG_CONTENT_REGEX
        .find(line)
        .map(|m| m.as_str().to_string())
}

/// Default template for the fact-pair cross-validate prompt.
const DEFAULT_PAIR_CROSS_VALIDATE_PROMPT: &str = "Compare these fact pairs from different knowledge base documents.\n\n\
For each pair, trace the evidence chain:\n\
1. Compare Fact A and Fact B — do they address the same claim?\n\
2. Consider source citations and temporal context for each\n\
3. Classify the relationship\n\n\
Statuses:\n\
- SUPPORTS: Fact B confirms or is consistent with Fact A\n\
- CONTRADICTS: Facts give different answers to the same question about the same entity\n\
- SUPERSEDES: Fact B provides newer information that replaces Fact A\n\
- CONSISTENT: Facts are about different aspects and don't conflict\n\n\
Common mistakes to avoid:\n\
✗ WRONG: Flagging as CONTRADICTS because the SOURCES are different. Two sources can confirm the same fact.\n\
✗ WRONG: Flagging as SUPERSEDES because one source is older. A 2019 source citing \
\"founded in 1924\" is NOT superseded — the fact is timeless.\n\
✗ WRONG: Flagging boundary-month overlaps as CONTRADICTS. \"Role A ends 2016-11\" + \
\"Role B starts 2016-11\" = normal transition.\n\
✗ WRONG: Flagging two DIFFERENT facts about the same entity as contradicting. \
\"Fleet size: 900\" and \"Destinations: 200\" coexist.\n\
✓ RIGHT: CONTRADICTS only when two sources give DIFFERENT answers to the SAME question \
about the SAME entity.\n\n\
{fact_pairs}\
---\n\nRespond ONLY with a JSON array. Each element must have: \
pair (number), status (SUPPORTS/CONTRADICTS/SUPERSEDES/CONSISTENT), \
reason (string).\n";

/// Build the LLM prompt for a batch of fact pairs.
fn build_pair_prompt(batch: &[&FactPairContext]) -> String {
    let mut fact_pairs = String::new();

    for (i, fpc) in batch.iter().enumerate() {
        let idx = i + 1;
        write_str!(fact_pairs, "Pair {idx}:\n");
        // Fact A
        write_str!(
            fact_pairs,
            "  Fact A (doc: \"{}\", line {}): \"{}\"",
            fpc.title_a,
            fpc.pair.fact_a.line_number,
            fpc.pair.fact_a.fact_text
        );
        if let Some(ref t) = fpc.temporal_a {
            write_str!(fact_pairs, " {t}");
        }
        fact_pairs.push('\n');
        if !fpc.source_defs_a.is_empty() {
            write_str!(fact_pairs, "  Sources A: {}\n", fpc.source_defs_a.join("; "));
        }
        // Fact B
        write_str!(
            fact_pairs,
            "  Fact B (doc: \"{}\", line {}): \"{}\"",
            fpc.title_b,
            fpc.pair.fact_b.line_number,
            fpc.pair.fact_b.fact_text
        );
        if let Some(ref t) = fpc.temporal_b {
            write_str!(fact_pairs, " {t}");
        }
        fact_pairs.push('\n');
        if !fpc.source_defs_b.is_empty() {
            write_str!(fact_pairs, "  Sources B: {}\n", fpc.source_defs_b.join("; "));
        }
        if fpc.title_b_in_doc_a || fpc.title_a_in_doc_b {
            write_str!(fact_pairs, "  (Documents cross-reference each other)\n");
        }
        fact_pairs.push('\n');
    }

    let prompts = crate::Config::load(None).unwrap_or_default().prompts;
    crate::config::prompts::resolve_prompt(
        &prompts,
        "cross_validate_pairs",
        DEFAULT_PAIR_CROSS_VALIDATE_PROMPT,
        &[("fact_pairs", &fact_pairs)],
    )
}

/// Parse LLM response for fact-pair classification.
fn parse_pair_response(response: &str) -> Vec<PairCheckResult> {
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
    match serde_json::from_str::<Vec<PairCheckResult>>(json_str) {
        Ok(results) => results,
        Err(e) => {
            warn!("Failed to parse fact-pair LLM response: {e}");
            Vec::new()
        }
    }
}

/// Determine which document should receive the review question.
///
/// Prefers the document with fewer source citations, or the one with
/// older/no temporal tags. Returns `(target_doc_id, target_line, cross_ref_title)`.
fn attribute_question(fpc: &FactPairContext) -> (&str, usize, &str) {
    let a_sources = fpc.source_defs_a.len();
    let b_sources = fpc.source_defs_b.len();

    // Fewer sources → more likely to need review
    if a_sources < b_sources {
        (
            &fpc.pair.fact_a.document_id,
            fpc.pair.fact_a.line_number,
            &fpc.title_b,
        )
    } else if b_sources < a_sources {
        (
            &fpc.pair.fact_b.document_id,
            fpc.pair.fact_b.line_number,
            &fpc.title_a,
        )
    } else {
        // Equal sources — attribute to fact A (arbitrary but deterministic)
        (
            &fpc.pair.fact_a.document_id,
            fpc.pair.fact_a.line_number,
            &fpc.title_b,
        )
    }
}

/// Convert a pair classification result into a review question.
fn pair_result_to_question(
    result: &PairCheckResult,
    batch: &[&FactPairContext],
) -> Option<(String, ReviewQuestion)> {
    let fpc = batch.get(result.pair.checked_sub(1)?)?;

    let qtype = match result.status.to_uppercase().as_str() {
        "CONTRADICTS" => QuestionType::Conflict,
        "SUPERSEDES" => QuestionType::Stale,
        _ => return None, // SUPPORTS and CONSISTENT produce no questions
    };

    let (target_doc_id, target_line, cross_ref_title) = attribute_question(fpc);

    let desc = format!(
        "Cross-check with {}: {} — {}",
        cross_ref_title,
        if target_doc_id == fpc.pair.fact_a.document_id {
            &fpc.pair.fact_a.fact_text
        } else {
            &fpc.pair.fact_b.fact_text
        },
        result.reason
    );

    Some((
        target_doc_id.to_string(),
        ReviewQuestion::new(qtype, Some(target_line), desc),
    ))
}

/// Cross-validate pre-computed fact pairs using LLM classification.
///
/// Returns a map of document ID → review questions generated for that document.
/// This is the preferred entry point when fact embeddings are available in the DB.
pub async fn cross_validate_facts(
    fact_pairs: &[FactPair],
    db: &Database,
    llm: &dyn LlmProvider,
    deadline: Option<std::time::Instant>,
    batch_size: usize,
) -> Result<HashMap<String, Vec<ReviewQuestion>>, FactbaseError> {
    if fact_pairs.is_empty() {
        return Ok(HashMap::new());
    }

    // Load document contexts (cached across pairs)
    let mut doc_cache: HashMap<String, DocContext> = HashMap::new();
    let mut enriched: Vec<FactPairContext> = Vec::with_capacity(fact_pairs.len());

    for pair in fact_pairs {
        if deadline.is_some_and(|d| std::time::Instant::now() > d) {
            break;
        }
        // Load both documents
        if load_doc_context(&pair.fact_a.document_id, &mut doc_cache, db).is_none() {
            continue;
        }
        if load_doc_context(&pair.fact_b.document_id, &mut doc_cache, db).is_none() {
            continue;
        }

        let ctx_a = &doc_cache[&pair.fact_a.document_id];
        let ctx_b = &doc_cache[&pair.fact_b.document_id];

        let source_defs_a =
            get_source_defs_for_line(&ctx_a.content, pair.fact_a.line_number, &ctx_a.source_map);
        let source_defs_b =
            get_source_defs_for_line(&ctx_b.content, pair.fact_b.line_number, &ctx_b.source_map);
        let temporal_a = get_temporal_tag_on_line(&ctx_a.content, pair.fact_a.line_number);
        let temporal_b = get_temporal_tag_on_line(&ctx_b.content, pair.fact_b.line_number);

        let title_b_in_doc_a = ctx_a
            .content
            .to_lowercase()
            .contains(&ctx_b.title.to_lowercase());
        let title_a_in_doc_b = ctx_b
            .content
            .to_lowercase()
            .contains(&ctx_a.title.to_lowercase());

        enriched.push(FactPairContext {
            pair: pair.clone(),
            title_a: ctx_a.title.clone(),
            title_b: ctx_b.title.clone(),
            source_defs_a,
            source_defs_b,
            temporal_a,
            temporal_b,
            title_b_in_doc_a,
            title_a_in_doc_b,
        });
    }

    if enriched.is_empty() {
        return Ok(HashMap::new());
    }

    // Build temporal context for STALE suppression on closed ranges.
    // We need this per-document for facts that have closed temporal tags.
    let mut doc_temporal_closed: HashMap<String, HashMap<usize, TemporalTagType>> = HashMap::new();
    for fpc in &enriched {
        for (doc_id, content) in [
            (&fpc.pair.fact_a.document_id, &doc_cache[&fpc.pair.fact_a.document_id].content),
            (&fpc.pair.fact_b.document_id, &doc_cache[&fpc.pair.fact_b.document_id].content),
        ] {
            if !doc_temporal_closed.contains_key(doc_id.as_str()) {
                let body = &content[..crate::patterns::body_end_offset(content)];
                let tags = crate::processor::parse_temporal_tags(body);
                let heading_map = super::stale::build_heading_temporal_map(body, &tags);
                doc_temporal_closed.insert(doc_id.clone(), heading_map);
            }
        }
    }

    let mut questions: HashMap<String, Vec<ReviewQuestion>> = HashMap::new();

    let effective_batch_size = batch_size.clamp(1, 50);

    for chunk in enriched.chunks(effective_batch_size) {
        if deadline.is_some_and(|d| std::time::Instant::now() > d) {
            break;
        }
        let batch: Vec<&FactPairContext> = chunk.iter().collect();
        let prompt = build_pair_prompt(&batch);

        let response = match llm.complete(&prompt).await {
            Ok(r) => r,
            Err(e) => {
                warn!("LLM call failed during fact-pair cross-validation: {e}");
                continue;
            }
        };

        let results = parse_pair_response(&response);
        for r in &results {
            let status_upper = r.status.to_uppercase();

            // Suppress SUPERSEDES for facts with closed temporal ranges
            if status_upper == "SUPERSEDES" {
                if let Some(fpc) = r.pair.checked_sub(1).and_then(|i| batch.get(i)) {
                    let (target_doc_id, target_line, _) = attribute_question(fpc);
                    if let Some(heading_map) = doc_temporal_closed.get(target_doc_id) {
                        if heading_map.contains_key(&target_line) {
                            continue;
                        }
                    }
                }
            }

            if let Some((doc_id, q)) = pair_result_to_question(&r, &batch) {
                questions.entry(doc_id).or_default().push(q);
            }
        }
    }

    Ok(questions)
}

/// Extract document title from content (first `# ` heading) or fall back to doc_id.
fn extract_title(content: &str, doc_id: &str) -> String {
    content
        .lines()
        .find(|l| l.starts_with("# "))
        .map_or_else(|| doc_id.to_string(), |l| crate::patterns::clean_title(&l[2..]))
}

/// Default template for the cross-validate prompt.
const DEFAULT_CROSS_VALIDATE_PROMPT: &str = "Validate these facts against evidence from other knowledge base documents.\n\n\
For each fact, trace the evidence chain:\n\
1. List each piece of related evidence with its source document title\n\
2. For each piece, classify it as SUPPORTS, CONTRADICTS, or SUPERSEDES the fact\n\
3. Derive the final status from the evidence chain\n\n\
Statuses:\n\
- CONSISTENT: All evidence SUPPORTS or is neutral\n\
- CONFLICT: At least one piece CONTRADICTS the fact itself (not just the source of the fact)\n\
- STALE: At least one piece SUPERSEDES with newer information\n\
- UNCERTAIN: Evidence exists but is ambiguous\n\n\
Common mistakes to avoid:\n\
✗ WRONG: Flagging a fact as STALE because the SOURCE is old. A 2019 source citing \
\"founded in 1924\" is NOT stale — the fact is timeless.\n\
✗ WRONG: Flagging boundary-month overlaps as CONFLICT. \"Role A ends 2016-11\" + \
\"Role B starts 2016-11\" = normal transition.\n\
✗ WRONG: Flagging two DIFFERENT facts about the same entity as conflicting. \
\"Fleet size: 900\" and \"Destinations: 200\" coexist.\n\
✓ RIGHT: CONFLICT only when two sources give DIFFERENT answers to the SAME question \
about the SAME entity.\n\n\
Document: {doc_title}\n---\n\
{fact_batch}\
---\n\nRespond ONLY with a JSON array. Each element must have: \
fact (number), status (CONSISTENT/CONFLICT/STALE/UNCERTAIN), \
reason (string), source_doc (string, empty if N/A).\n";

/// Build the LLM prompt for a batch of facts with their cross-document context.
fn build_prompt(doc_title: &str, batch: &[&FactWithContext]) -> String {
    let mut fact_batch = String::new();

    for (i, fwc) in batch.iter().enumerate() {
        let idx = i + 1;
        write_str!(
            fact_batch,
            "Fact {idx} (line {}): \"{}\"\nRelated information:\n",
            fwc.fact.line_number,
            fwc.fact.text
        );
        for r in &fwc.related {
            let snip = if r.snippet.len() > MAX_SNIPPET_LEN {
                format!("{}...", &r.snippet[..MAX_SNIPPET_LEN])
            } else {
                r.snippet.clone()
            };
            writeln_str!(fact_batch, "- [{}] \"{}\"", r.title, snip);
        }
        if !fwc.source_defs.is_empty() {
            write_str!(fact_batch, "Sources for this fact: ");
            for (j, sd) in fwc.source_defs.iter().enumerate() {
                if j > 0 {
                    fact_batch.push_str("; ");
                }
                fact_batch.push_str(sd);
            }
            fact_batch.push('\n');
        }
        fact_batch.push('\n');
    }

    let prompts = crate::Config::load(None).unwrap_or_default().prompts;
    crate::config::prompts::resolve_prompt(
        &prompts,
        "cross_validate",
        DEFAULT_CROSS_VALIDATE_PROMPT,
        &[("doc_title", doc_title), ("fact_batch", &fact_batch)],
    )
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
    doc_type: Option<&str>,
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: &dyn LlmProvider,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<ReviewQuestion>, FactbaseError> {
    let facts = extract_all_facts(content);
    if facts.is_empty() {
        return Ok(Vec::new());
    }

    // Build a lookup from footnote number → definition text for source context.
    let source_defs = parse_source_definitions(content);
    let source_map: std::collections::HashMap<u32, String> = source_defs
        .into_iter()
        .map(|d| {
            let text = if let Some(date) = &d.date {
                format!("[^{}]: {} {}, {}", d.number, d.source_type, d.context, date)
            } else if d.context.is_empty() {
                format!("[^{}]: {}", d.number, d.source_type)
            } else {
                format!("[^{}]: {} {}", d.number, d.source_type, d.context)
            };
            (d.number, text)
        })
        .collect();

    let content_lower = content.to_lowercase();
    let mut facts_with_context = Vec::with_capacity(facts.len());

    for fact in facts {
        // Check deadline before each embedding call
        if deadline.is_some_and(|d| std::time::Instant::now() > d) {
            break;
        }
        let fact_embedding = embedding.generate(&fact.text).await?;
        let search_results =
            db.search_semantic_paginated(&fact_embedding, 10, 0, None, None, Some(&fact.text))?;

        let related: Vec<_> = search_results
            .results
            .into_iter()
            .filter(|r| r.id != doc_id)
            .filter(|r| r.relevance_score >= RELEVANCE_THRESHOLD)
            // When source and result have different document types, require
            // the result's title to appear somewhere in the source document.
            // This filters out semantically-similar-but-logically-unrelated
            // noise while preserving genuine cross-type references. Checking
            // the full document (not just the fact line) enables bidirectional
            // discovery: if Acme's doc mentions "John Smith" anywhere, then
            // John's doc is relevant evidence when checking ANY of Acme's facts.
            // Same-type results pass through — they are more likely to be
            // genuinely related (e.g., two company docs referencing each other).
            .filter(|r| {
                let same_type = match (doc_type, r.doc_type.as_deref()) {
                    (Some(src), Some(res)) => src == res,
                    (None, None) => true,
                    _ => false,
                };
                if same_type {
                    return true;
                }
                let title_lower = r.title.to_lowercase();
                content_lower.contains(&title_lower)
            })
            .collect();

        if related.is_empty() {
            continue;
        }

        let fact_source_defs: Vec<String> = fact
            .source_refs
            .iter()
            .filter_map(|n| source_map.get(n).cloned())
            .collect();

        facts_with_context.push(FactWithContext {
            fact,
            related,
            source_defs: fact_source_defs,
        });
    }

    if facts_with_context.is_empty() {
        return Ok(Vec::new());
    }

    // Build temporal context to suppress false STALE flags on facts with
    // closed temporal ranges (e.g., historical events where old sources are expected).
    let body = &content[..crate::patterns::body_end_offset(content)];
    let temporal_tags = crate::processor::parse_temporal_tags(body);
    let heading_temporal_map = super::stale::build_heading_temporal_map(body, &temporal_tags);

    let doc_title = extract_title(content, doc_id);
    let mut questions = Vec::new();

    for chunk in facts_with_context.chunks(BATCH_SIZE) {
        // Check deadline before each LLM call
        if deadline.is_some_and(|d| std::time::Instant::now() > d) {
            break;
        }
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
            // Suppress STALE results for facts with closed temporal ranges —
            // old sources are expected for historical/completed events.
            if r.status.eq_ignore_ascii_case("STALE") {
                if let Some(fwc) = r.fact.checked_sub(1).and_then(|i| batch.get(i)) {
                    let ln = fwc.fact.line_number;
                    let has_closed = temporal_tags.iter().any(|t| {
                        t.line_number == ln
                            && matches!(
                                t.tag_type,
                                TemporalTagType::Range
                                    | TemporalTagType::PointInTime
                                    | TemporalTagType::Historical
                            )
                    }) || heading_temporal_map.get(&ln).is_some();
                    if has_closed {
                        continue;
                    }
                }
            }
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
    use crate::database::tests::{test_db, test_repo_in_db};
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::llm::test_helpers::MockLlm;
    use crate::models::Document;

    #[tokio::test]
    async fn test_empty_content_returns_no_questions() {
        let (db, _tmp) = test_db();
        let questions = cross_validate_document(
            "",
            "abc123",
            None,
            &db,
            &MockEmbedding::new(1024),
            &MockLlm::default(),
            None,
        )
        .await
        .unwrap();
        assert!(questions.is_empty());
    }

    #[tokio::test]
    async fn test_no_list_items_returns_no_questions() {
        let (db, _tmp) = test_db();
        let content = "# Title\n\nJust paragraphs here.";
        let questions = cross_validate_document(
            content,
            "abc123",
            None,
            &db,
            &MockEmbedding::new(1024),
            &MockLlm::default(),
            None,
        )
        .await
        .unwrap();
        assert!(questions.is_empty());
    }

    #[tokio::test]
    async fn test_no_relevant_results_returns_no_questions() {
        let (db, _tmp) = test_db();
        let content = "# Person\n\n- VP Engineering at Acme\n- Based in Seattle";
        let questions = cross_validate_document(
            content,
            "abc123",
            Some("person"),
            &db,
            &MockEmbedding::new(1024),
            &MockLlm::default(),
            None,
        )
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
                source_refs: vec![],
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
            source_defs: vec![],
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
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
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
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
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
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
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
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
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

    #[test]
    fn test_source_map_built_from_definitions() {
        let content = "# Person\n\n\
            - VP at Acme [^1]\n\
            - Based in Seattle\n\n\
            ---\n\
            [^1]: LinkedIn profile, scraped 2024-01-15\n";
        let defs = parse_source_definitions(content);
        let source_map: std::collections::HashMap<u32, String> = defs
            .into_iter()
            .map(|d| {
                let text = if let Some(date) = &d.date {
                    format!("[^{}]: {} {}, {}", d.number, d.source_type, d.context, date)
                } else if d.context.is_empty() {
                    format!("[^{}]: {}", d.number, d.source_type)
                } else {
                    format!("[^{}]: {} {}", d.number, d.source_type, d.context)
                };
                (d.number, text)
            })
            .collect();
        assert_eq!(source_map.len(), 1);
        assert!(source_map.get(&1).unwrap().contains("LinkedIn"));
    }

    #[test]
    fn test_source_defs_attached_to_fact_with_refs() {
        // Simulate the lookup logic from cross_validate_document
        let source_map: std::collections::HashMap<u32, String> = [
            (1, "[^1]: LinkedIn profile, scraped 2024-01-15".into()),
            (2, "[^2]: Internal wiki".into()),
        ]
        .into();
        let fact = FactLine {
            line_number: 3,
            text: "VP at Acme".into(),
            section: None,
            source_refs: vec![1],
        };
        let defs: Vec<String> = fact
            .source_refs
            .iter()
            .filter_map(|n| source_map.get(n).cloned())
            .collect();
        assert_eq!(defs.len(), 1);
        assert!(defs[0].contains("LinkedIn"));
    }

    #[test]
    fn test_source_defs_empty_when_no_refs() {
        let source_map: std::collections::HashMap<u32, String> =
            [(1, "[^1]: LinkedIn".into())].into();
        let fact = FactLine {
            line_number: 3,
            text: "Based in Seattle".into(),
            section: None,
            source_refs: vec![],
        };
        let defs: Vec<String> = fact
            .source_refs
            .iter()
            .filter_map(|n| source_map.get(n).cloned())
            .collect();
        assert!(defs.is_empty());
    }

    #[test]
    fn test_source_defs_multiple_refs() {
        let source_map: std::collections::HashMap<u32, String> = [
            (1, "[^1]: LinkedIn".into()),
            (2, "[^2]: Internal wiki".into()),
            (3, "[^3]: Press release".into()),
        ]
        .into();
        let fact = FactLine {
            line_number: 5,
            text: "Joined in 2020".into(),
            section: None,
            source_refs: vec![1, 3],
        };
        let defs: Vec<String> = fact
            .source_refs
            .iter()
            .filter_map(|n| source_map.get(n).cloned())
            .collect();
        assert_eq!(defs.len(), 2);
        assert!(defs[0].contains("LinkedIn"));
        assert!(defs[1].contains("Press release"));
    }

    #[test]
    fn test_build_prompt_multiple_source_defs_semicolon_separated() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 5,
                text: "Joined Acme in 2020".into(),
                section: None,
                source_refs: vec![1, 2],
            },
            related: vec![],
            source_defs: vec![
                "[^1]: LinkedIn profile, scraped 2024-01-15".into(),
                "[^2]: Press release, 2020-03-01".into(),
            ],
        };
        let prompt = build_prompt("Acme Corp", &[&fwc]);
        assert!(prompt.contains(
            "Sources for this fact: [^1]: LinkedIn profile, scraped 2024-01-15; [^2]: Press release, 2020-03-01"
        ));
    }

    #[test]
    fn test_build_prompt_source_context_after_related_info() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 3,
                text: "VP at Acme".into(),
                section: None,
                source_refs: vec![1],
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
            source_defs: vec!["[^1]: LinkedIn profile".into()],
        };
        let prompt = build_prompt("Acme Corp", &[&fwc]);
        let related_pos = prompt.find("[Jane Smith]").unwrap();
        let source_pos = prompt.find("Sources for this fact:").unwrap();
        assert!(
            source_pos > related_pos,
            "source context should appear after related info"
        );
    }

    /// Integration-style test: product fact sourced from a person — MockLlm returns
    /// CONSISTENT, verifying no stale questions are generated for the product fact
    /// even though the source person might be inactive.
    #[tokio::test]
    async fn test_cross_validate_product_fact_with_source_not_flagged_stale() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        // Insert a "person" document that could be stale
        let mut person_doc = Document::test_default();
        person_doc.id = "person1".to_string();
        person_doc.title = "Jane Smith".to_string();
        person_doc.content = "# Jane Smith\n\n- Left Acme Corp in 2024".to_string();
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person1", &vec![0.1; 1024]).unwrap();

        // Product document with a fact sourced from Jane
        let content = "# Acme Product\n\n\
            - Supports 10K concurrent users [^1]\n\n\
            ---\n\
            [^1]: Jane Smith, internal review 2024-06\n";

        // LLM returns CONSISTENT — the product fact is valid regardless of Jane's status
        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"CONSISTENT","reason":"Product capability confirmed","source_doc":""}]"#,
        );

        let questions =
            cross_validate_document(content, "prod01", Some("product"), &db, &MockEmbedding::new(1024), &llm, None)
                .await
                .unwrap();

        assert!(
            questions.is_empty(),
            "product fact with stale source should not generate questions"
        );
    }

    #[test]
    fn test_build_prompt_includes_source_context() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 3,
                text: "VP at Acme".into(),
                section: None,
                source_refs: vec![1],
            },
            related: vec![],
            source_defs: vec!["[^1]: LinkedIn profile, scraped 2024-01-15".into()],
        };
        let prompt = build_prompt("Acme Corp", &[&fwc]);
        assert!(
            prompt.contains("Sources for this fact: [^1]: LinkedIn profile, scraped 2024-01-15")
        );
    }

    #[test]
    fn test_build_prompt_no_source_context_when_empty() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 3,
                text: "Based in Seattle".into(),
                section: None,
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
        };
        let prompt = build_prompt("Acme Corp", &[&fwc]);
        assert!(!prompt.contains("Sources for this fact:"));
    }

    #[test]
    fn test_build_prompt_evidence_chain_structure() {
        let fwc = FactWithContext {
            fact: FactLine {
                line_number: 1,
                text: "fact".into(),
                section: None,
                source_refs: vec![],
            },
            related: vec![],
            source_defs: vec![],
        };
        let prompt = build_prompt("Doc", &[&fwc]);
        assert!(prompt.contains("evidence chain"));
        assert!(prompt.contains("SUPPORTS, CONTRADICTS, or SUPERSEDES"));
        assert!(prompt.contains("WRONG: Flagging a fact as STALE"));
        assert!(prompt.contains("WRONG: Flagging boundary-month"));
        assert!(prompt.contains("RIGHT: CONFLICT only when"));
    }

    /// Cross-type results are filtered when the result's title is not mentioned
    /// anywhere in the source document. This prevents semantically-similar-but-
    /// logically-unrelated documents from polluting cross-validation evidence.
    #[tokio::test]
    async fn test_cross_type_result_filtered_when_title_not_in_fact() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        // Insert a person document with doc_type = "person"
        let mut person_doc = Document::test_default();
        person_doc.id = "person1".to_string();
        person_doc.title = "Jane Smith".to_string();
        person_doc.doc_type = Some("person".to_string());
        person_doc.content = "# Jane Smith\n\n- Status: Inactive".to_string();
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person1", &vec![0.1; 1024]).unwrap();

        // Product document — neither fact NOR document mentions Jane Smith
        let content = "# Acme Platform\n\n- Supports utilization review\n";

        // MockLlm would flag STALE if it saw the person doc, but it should
        // never be called because the person doc gets filtered out.
        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"STALE","reason":"person inactive","source_doc":"Jane Smith"}]"#,
        );

        let questions = cross_validate_document(
            content,
            "prod01",
            Some("product"),
            &db,
            &MockEmbedding::new(1024),
            &llm,
            None,
        )
        .await
        .unwrap();

        assert!(
            questions.is_empty(),
            "cross-type result should be filtered when title not in document"
        );
    }

    /// Cross-type results are kept when the result's title IS mentioned in the
    /// source document, enabling genuine cross-type validation.
    #[tokio::test]
    async fn test_cross_type_result_kept_when_title_in_fact() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut person_doc = Document::test_default();
        person_doc.id = "person1".to_string();
        person_doc.title = "Jane Smith".to_string();
        person_doc.doc_type = Some("person".to_string());
        person_doc.content = "# Jane Smith\n\n- Status: Inactive".to_string();
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person1", &vec![0.1; 1024]).unwrap();

        // Product document — fact DOES mention Jane Smith
        let content = "# Acme Platform\n\n- Jane Smith leads the platform team\n";

        // LLM returns STALE — this time it's valid because Jane is mentioned
        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"STALE","reason":"Jane is now inactive","source_doc":"Jane Smith"}]"#,
        );

        let questions = cross_validate_document(
            content,
            "prod01",
            Some("product"),
            &db,
            &MockEmbedding::new(1024),
            &llm,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            questions.len(),
            1,
            "cross-type result should be kept when title appears in document"
        );
    }

    /// Bidirectional cross-type: a cross-type result is kept when the result's
    /// title appears elsewhere in the source document, even if NOT in the
    /// specific fact being validated. This enables discovering information gaps
    /// between related documents of different types.
    #[tokio::test]
    async fn test_cross_type_result_kept_when_title_in_document_not_in_fact() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        // Person doc says they're CEO at Acme
        let mut person_doc = Document::test_default();
        person_doc.id = "person1".to_string();
        person_doc.title = "John Smith".to_string();
        person_doc.doc_type = Some("person".to_string());
        person_doc.content = "# John Smith\n\n- CEO at Acme Corp @t[2020..]".to_string();
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person1", &vec![0.1; 1024]).unwrap();

        // Company doc mentions John Smith in one fact, but the fact being
        // validated ("raised $50M") does NOT mention John Smith.
        // The old title-in-fact filter would reject John's doc here.
        // The new title-in-document filter keeps it because "John Smith"
        // appears elsewhere in the company doc.
        let content = "# Acme Corp\n\n\
            - Raised $50M Series B @t[=2023-06]\n\
            - John Smith joined as advisor @t[=2019]\n";

        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"CONFLICT","reason":"John Smith is CEO not just advisor, doc missing leadership info","source_doc":"John Smith"}]"#,
        );

        let questions = cross_validate_document(
            content,
            "comp01",
            Some("company"),
            &db,
            &MockEmbedding::new(1024),
            &llm,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            questions.len(),
            1,
            "cross-type result should be kept when title appears in document even if not in the specific fact"
        );
    }

    /// Same-type cross-validation keeps all results regardless of title mention.
    #[tokio::test]
    async fn test_same_type_results_not_filtered() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut person_doc = Document::test_default();
        person_doc.id = "person2".to_string();
        person_doc.title = "Bob Jones".to_string();
        person_doc.doc_type = Some("person".to_string());
        person_doc.content = "# Bob Jones\n\n- Works at Acme".to_string();
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person2", &vec![0.1; 1024]).unwrap();

        // Another person document — fact doesn't mention Bob by name
        let content = "# Alice Brown\n\n- VP Engineering at Acme\n";

        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"CONSISTENT","reason":"both at Acme","source_doc":""}]"#,
        );

        let questions = cross_validate_document(
            content,
            "person3",
            Some("person"),
            &db,
            &MockEmbedding::new(1024),
            &llm,
            None,
        )
        .await
        .unwrap();

        // CONSISTENT returns no questions, but the key is the LLM WAS called
        // (same-type docs are not filtered regardless of title mention)
        assert!(questions.is_empty());
    }

    /// Non-person cross-type filtering: a project doc should not be polluted
    /// by an unrelated company doc that happens to match semantically.
    #[tokio::test]
    async fn test_cross_type_project_company_filtered_when_unrelated() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut company_doc = Document::test_default();
        company_doc.id = "comp01".to_string();
        company_doc.title = "Globex Industries".to_string();
        company_doc.doc_type = Some("company".to_string());
        company_doc.content = "# Globex Industries\n\n- Pivoted to AI in 2025".to_string();
        db.upsert_document(&company_doc).unwrap();
        db.upsert_embedding("comp01", &vec![0.1; 1024]).unwrap();

        // Project doc — fact does NOT mention Globex Industries
        let content = "# Project Atlas\n\n- Uses machine learning for predictions\n";

        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"STALE","reason":"company pivoted","source_doc":"Globex Industries"}]"#,
        );

        let questions = cross_validate_document(
            content,
            "proj01",
            Some("project"),
            &db,
            &MockEmbedding::new(1024),
            &llm,
            None,
        )
        .await
        .unwrap();

        assert!(
            questions.is_empty(),
            "unrelated cross-type result should be filtered out"
        );
    }

    /// STALE results from the LLM should be suppressed for facts with closed
    /// temporal ranges — old sources are expected for historical/completed events.
    #[tokio::test]
    async fn test_cross_validate_suppresses_stale_for_historical_facts() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut related_doc = Document::test_default();
        related_doc.id = "rel001".to_string();
        related_doc.title = "Roman Military".to_string();
        related_doc.content = "# Roman Military\n\n- Defeated at Adrianople".to_string();
        db.upsert_document(&related_doc).unwrap();
        db.upsert_embedding("rel001", &vec![0.1; 1024]).unwrap();

        // Document about a historical event with H1 temporal tag
        let content = "# Battle of Adrianople @t[=0378]\n\n\
            - Emperor Valens was killed [^1]\n\n\
            ---\n\
            [^1]: Burns, Thomas S., 1994\n";

        // LLM flags the fact as STALE because the source is from 1994
        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"STALE","reason":"Source from 1994 may be outdated","source_doc":""}]"#,
        );

        let questions = cross_validate_document(
            content, "bat001", None, &db, &MockEmbedding::new(1024), &llm, None,
        )
        .await
        .unwrap();

        assert!(
            questions.is_empty(),
            "STALE should be suppressed for facts under H1 with closed temporal tag"
        );
    }

    /// STALE results should still be generated for facts without closed temporal context.
    #[tokio::test]
    async fn test_cross_validate_stale_not_suppressed_without_temporal() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut related_doc = Document::test_default();
        related_doc.id = "rel002".to_string();
        related_doc.title = "Acme Corp".to_string();
        related_doc.content = "# Acme Corp\n\n- Changed CEO in 2025".to_string();
        db.upsert_document(&related_doc).unwrap();
        db.upsert_embedding("rel002", &vec![0.1; 1024]).unwrap();

        let content = "# Some Entity\n\n\
            - Works at Acme Corp [^1]\n\n\
            ---\n\
            [^1]: LinkedIn, 2020-01\n";

        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"STALE","reason":"Other docs suggest this changed","source_doc":"Acme Corp"}]"#,
        );

        let questions = cross_validate_document(
            content, "ent001", None, &db, &MockEmbedding::new(1024), &llm, None,
        )
        .await
        .unwrap();

        assert_eq!(
            questions.len(),
            1,
            "STALE should NOT be suppressed for facts without closed temporal context"
        );
    }

    /// Deadline already expired should cause cross_validate_document to return
    /// early without making any embedding or LLM calls.
    #[tokio::test]
    async fn test_cross_validate_deadline_stops_early() {
        let (db, _tmp) = test_db();
        let content = "# Entity\n\n- Fact one about something\n- Fact two about another thing\n";
        let llm = MockLlm::new(
            r#"[{"fact":1,"status":"CONFLICT","reason":"mismatch","source_doc":"other"}]"#,
        );
        // Deadline already in the past
        let deadline = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        let questions = cross_validate_document(
            content, "aaa", None, &db, &MockEmbedding::new(1024), &llm, deadline,
        )
        .await
        .unwrap();
        // Should return empty — deadline hit before any embedding calls
        assert!(questions.is_empty(), "expired deadline should skip all work");
    }

    // -----------------------------------------------------------------------
    // Fact-pair cross-validation tests
    // -----------------------------------------------------------------------

    use crate::models::{FactPair, FactSearchResult};

    fn make_fact(doc_id: &str, line: usize, text: &str) -> FactSearchResult {
        FactSearchResult {
            id: format!("{}_{}", doc_id, line),
            document_id: doc_id.to_string(),
            line_number: line,
            fact_text: text.to_string(),
            similarity: 0.9,
        }
    }

    fn make_pair(a: FactSearchResult, b: FactSearchResult) -> FactPair {
        FactPair {
            similarity: 0.9,
            fact_a: a,
            fact_b: b,
        }
    }

    #[test]
    fn test_parse_pair_response_valid() {
        let json = r#"[{"pair":1,"status":"CONTRADICTS","reason":"different values"}]"#;
        let results = parse_pair_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "CONTRADICTS");
        assert_eq!(results[0].pair, 1);
    }

    #[test]
    fn test_parse_pair_response_with_fences() {
        let json = "```json\n[{\"pair\":1,\"status\":\"SUPERSEDES\",\"reason\":\"newer\"}]\n```";
        let results = parse_pair_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "SUPERSEDES");
    }

    #[test]
    fn test_parse_pair_response_malformed() {
        let results = parse_pair_response("not json");
        assert!(results.is_empty());
    }

    #[test]
    fn test_build_pair_prompt_contains_facts() {
        let pair = make_pair(
            make_fact("doc1", 5, "VP at Acme"),
            make_fact("doc2", 10, "Left Acme in 2024"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "John Smith".into(),
            title_b: "Acme Corp".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let prompt = build_pair_prompt(&[&fpc]);
        assert!(prompt.contains("Pair 1:"));
        assert!(prompt.contains("Fact A (doc: \"John Smith\", line 5): \"VP at Acme\""));
        assert!(prompt.contains("Fact B (doc: \"Acme Corp\", line 10): \"Left Acme in 2024\""));
    }

    #[test]
    fn test_build_pair_prompt_includes_sources() {
        let pair = make_pair(
            make_fact("doc1", 3, "Founded in 1924"),
            make_fact("doc2", 7, "Established 1924"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec!["[^1]: Wikipedia, 2024-01".into()],
            source_defs_b: vec!["[^2]: Annual report".into()],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let prompt = build_pair_prompt(&[&fpc]);
        assert!(prompt.contains("Sources A: [^1]: Wikipedia, 2024-01"));
        assert!(prompt.contains("Sources B: [^2]: Annual report"));
    }

    #[test]
    fn test_build_pair_prompt_includes_temporal_tags() {
        let pair = make_pair(
            make_fact("doc1", 3, "CEO at Acme"),
            make_fact("doc2", 5, "Left Acme"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: Some("@t[2020..]".into()),
            temporal_b: Some("@t[=2024-06]".into()),
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let prompt = build_pair_prompt(&[&fpc]);
        assert!(prompt.contains("@t[2020..]"));
        assert!(prompt.contains("@t[=2024-06]"));
    }

    #[test]
    fn test_build_pair_prompt_cross_reference_note() {
        let pair = make_pair(
            make_fact("doc1", 3, "fact a"),
            make_fact("doc2", 5, "fact b"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: true,
            title_a_in_doc_b: false,
        };
        let prompt = build_pair_prompt(&[&fpc]);
        assert!(prompt.contains("Documents cross-reference each other"));
    }

    #[test]
    fn test_attribute_question_fewer_sources_gets_question() {
        let pair = make_pair(
            make_fact("doc1", 3, "fact a"),
            make_fact("doc2", 5, "fact b"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec![], // no sources
            source_defs_b: vec!["[^1]: source".into()], // has source
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let (target_doc, target_line, cross_ref) = attribute_question(&fpc);
        assert_eq!(target_doc, "doc1", "doc with fewer sources should get the question");
        assert_eq!(target_line, 3);
        assert_eq!(cross_ref, "Doc B");
    }

    #[test]
    fn test_attribute_question_b_fewer_sources() {
        let pair = make_pair(
            make_fact("doc1", 3, "fact a"),
            make_fact("doc2", 5, "fact b"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec!["[^1]: source".into()],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let (target_doc, target_line, cross_ref) = attribute_question(&fpc);
        assert_eq!(target_doc, "doc2");
        assert_eq!(target_line, 5);
        assert_eq!(cross_ref, "Doc A");
    }

    #[test]
    fn test_pair_result_to_question_contradicts() {
        let pair = make_pair(
            make_fact("doc1", 3, "VP at Acme"),
            make_fact("doc2", 5, "Left Acme"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec![],
            source_defs_b: vec!["[^1]: source".into()],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let r = PairCheckResult {
            pair: 1,
            status: "CONTRADICTS".into(),
            reason: "different role status".into(),
        };
        let (doc_id, q) = pair_result_to_question(&r, &[&fpc]).unwrap();
        assert_eq!(doc_id, "doc1"); // fewer sources
        assert_eq!(q.question_type, QuestionType::Conflict);
        assert!(q.description.contains("Doc B"));
        assert!(q.description.contains("VP at Acme"));
    }

    #[test]
    fn test_pair_result_to_question_supersedes() {
        let pair = make_pair(
            make_fact("doc1", 3, "Based in Seattle"),
            make_fact("doc2", 5, "Relocated to Austin"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "Doc A".into(),
            title_b: "Doc B".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let r = PairCheckResult {
            pair: 1,
            status: "SUPERSEDES".into(),
            reason: "newer location".into(),
        };
        let (_, q) = pair_result_to_question(&r, &[&fpc]).unwrap();
        assert_eq!(q.question_type, QuestionType::Stale);
    }

    #[test]
    fn test_pair_result_to_question_supports_returns_none() {
        let pair = make_pair(
            make_fact("doc1", 3, "fact"),
            make_fact("doc2", 5, "fact"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "A".into(),
            title_b: "B".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let r = PairCheckResult {
            pair: 1,
            status: "SUPPORTS".into(),
            reason: "".into(),
        };
        assert!(pair_result_to_question(&r, &[&fpc]).is_none());
    }

    #[test]
    fn test_pair_result_to_question_consistent_returns_none() {
        let pair = make_pair(
            make_fact("doc1", 3, "fact"),
            make_fact("doc2", 5, "fact"),
        );
        let fpc = FactPairContext {
            pair,
            title_a: "A".into(),
            title_b: "B".into(),
            source_defs_a: vec![],
            source_defs_b: vec![],
            temporal_a: None,
            temporal_b: None,
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let r = PairCheckResult {
            pair: 1,
            status: "CONSISTENT".into(),
            reason: "".into(),
        };
        assert!(pair_result_to_question(&r, &[&fpc]).is_none());
    }

    #[test]
    fn test_pair_result_to_question_invalid_pair_index() {
        let r = PairCheckResult {
            pair: 5,
            status: "CONTRADICTS".into(),
            reason: "bad".into(),
        };
        assert!(pair_result_to_question(&r, &[]).is_none());
    }

    #[test]
    fn test_pair_result_to_question_zero_pair_index() {
        let r = PairCheckResult {
            pair: 0,
            status: "CONTRADICTS".into(),
            reason: "bad".into(),
        };
        assert!(pair_result_to_question(&r, &[]).is_none());
    }

    #[test]
    fn test_get_source_defs_for_line() {
        let content = "# Title\n\n- VP at Acme [^1]\n- Based in Seattle\n\n---\n[^1]: LinkedIn profile, scraped 2024-01-15\n";
        let source_map: HashMap<u32, String> =
            [(1, "[^1]: LinkedIn profile, scraped 2024-01-15".into())].into();
        let defs = get_source_defs_for_line(content, 3, &source_map);
        assert_eq!(defs.len(), 1);
        assert!(defs[0].contains("LinkedIn"));
    }

    #[test]
    fn test_get_source_defs_for_line_no_refs() {
        let content = "# Title\n\n- Based in Seattle\n";
        let source_map: HashMap<u32, String> =
            [(1, "[^1]: LinkedIn".into())].into();
        let defs = get_source_defs_for_line(content, 3, &source_map);
        assert!(defs.is_empty());
    }

    #[test]
    fn test_get_temporal_tag_on_line() {
        let content = "# Title\n\n- CEO at Acme @t[2020..]\n";
        let tag = get_temporal_tag_on_line(content, 3);
        assert_eq!(tag.as_deref(), Some("@t[2020..]"));
    }

    #[test]
    fn test_get_temporal_tag_on_line_none() {
        let content = "# Title\n\n- Based in Seattle\n";
        let tag = get_temporal_tag_on_line(content, 3);
        assert!(tag.is_none());
    }

    #[tokio::test]
    async fn test_cross_validate_facts_empty_pairs() {
        let (db, _tmp) = test_db();
        let llm = MockLlm::default();
        let result = cross_validate_facts(&[], &db, &llm, None, PAIR_BATCH_SIZE).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_cross_validate_facts_contradicts_generates_question() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        // Insert two documents with different file paths
        let mut doc_a = Document::test_default();
        doc_a.id = "doc_a1".to_string();
        doc_a.file_path = "entity_a.md".to_string();
        doc_a.title = "Entity A".to_string();
        doc_a.content = "# Entity A\n\n- Revenue: $10M\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = Document::test_default();
        doc_b.id = "doc_b1".to_string();
        doc_b.file_path = "entity_b.md".to_string();
        doc_b.title = "Entity B".to_string();
        doc_b.content = "# Entity B\n\n- Revenue: $50M [^1]\n\n---\n[^1]: Annual report, 2024\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        let pairs = vec![make_pair(
            make_fact("doc_a1", 3, "Revenue: $10M"),
            make_fact("doc_b1", 3, "Revenue: $50M"),
        )];

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"different revenue figures"}]"#,
        );

        let result = cross_validate_facts(&pairs, &db, &llm, None, PAIR_BATCH_SIZE).await.unwrap();
        // Question should be on doc_a1 (fewer sources)
        assert!(result.contains_key("doc_a1"), "question should be attributed to doc with fewer sources");
        let qs = &result["doc_a1"];
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question_type, QuestionType::Conflict);
        assert!(qs[0].description.contains("Entity B"));
    }

    #[tokio::test]
    async fn test_cross_validate_facts_supports_no_question() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut doc_a = Document::test_default();
        doc_a.id = "doc_a2".to_string();
        doc_a.file_path = "doc_a2.md".to_string();
        doc_a.content = "# Doc A\n\n- Founded in 1924\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = Document::test_default();
        doc_b.id = "doc_b2".to_string();
        doc_b.file_path = "doc_b2.md".to_string();
        doc_b.content = "# Doc B\n\n- Established 1924\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        let pairs = vec![make_pair(
            make_fact("doc_a2", 3, "Founded in 1924"),
            make_fact("doc_b2", 3, "Established 1924"),
        )];

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"SUPPORTS","reason":"same founding year"}]"#,
        );

        let result = cross_validate_facts(&pairs, &db, &llm, None, PAIR_BATCH_SIZE).await.unwrap();
        assert!(result.is_empty(), "SUPPORTS should not generate questions");
    }

    #[tokio::test]
    async fn test_cross_validate_facts_supersedes_suppressed_for_historical() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut doc_a = Document::test_default();
        doc_a.id = "doc_a3".to_string();
        doc_a.file_path = "doc_a3.md".to_string();
        doc_a.content = "# Battle of Thermopylae @t[=480 BCE]\n\n- Spartans held the pass\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = Document::test_default();
        doc_b.id = "doc_b3".to_string();
        doc_b.file_path = "doc_b3.md".to_string();
        doc_b.content = "# Greek Wars\n\n- New archaeological evidence about Thermopylae\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        let pairs = vec![make_pair(
            make_fact("doc_a3", 3, "Spartans held the pass"),
            make_fact("doc_b3", 3, "New archaeological evidence about Thermopylae"),
        )];

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"SUPERSEDES","reason":"newer evidence"}]"#,
        );

        let result = cross_validate_facts(&pairs, &db, &llm, None, PAIR_BATCH_SIZE).await.unwrap();
        assert!(result.is_empty(), "SUPERSEDES should be suppressed for historical facts with closed temporal tags");
    }

    #[tokio::test]
    async fn test_cross_validate_facts_deadline_stops_early() {
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut doc = Document::test_default();
        doc.id = "doc_dl".to_string();
        doc.file_path = "doc_dl.md".to_string();
        doc.content = "# Doc\n\n- Fact\n".to_string();
        db.upsert_document(&doc).unwrap();

        let mut doc2 = Document::test_default();
        doc2.id = "doc_d2".to_string();
        doc2.file_path = "doc_d2.md".to_string();
        doc2.content = "# Doc2\n\n- Fact2\n".to_string();
        db.upsert_document(&doc2).unwrap();

        let pairs = vec![make_pair(
            make_fact("doc_dl", 3, "Fact"),
            make_fact("doc_d2", 3, "Fact2"),
        )];

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );

        // Deadline already expired
        let deadline = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        let result = cross_validate_facts(&pairs, &db, &llm, deadline, PAIR_BATCH_SIZE).await.unwrap();
        assert!(result.is_empty(), "expired deadline should skip all work");
    }

    /// Fallback path test: when fact_embeddings table is empty, check.rs
    /// should fall back to per-document cross-validation.
    #[tokio::test]
    async fn test_fallback_when_no_fact_embeddings() {
        let (db, _tmp) = test_db();
        // Verify fact embedding count is 0
        assert_eq!(db.get_fact_embedding_count().unwrap(), 0);
        // The fallback logic is in check.rs — this just verifies the precondition
    }

    // -----------------------------------------------------------------------
    // Batch size configuration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_pair_prompt_multiple_cross_doc_pairs() {
        let pair1 = make_pair(
            make_fact("doc_a", 3, "Revenue: $10M"),
            make_fact("doc_c", 5, "Revenue: $50M"),
        );
        let pair2 = make_pair(
            make_fact("doc_b", 7, "Founded in 1990"),
            make_fact("doc_d", 2, "Established 1985"),
        );
        let fpc1 = FactPairContext {
            pair: pair1,
            title_a: "Company A".into(),
            title_b: "Company C".into(),
            source_defs_a: vec![],
            source_defs_b: vec!["[^1]: Annual report".into()],
            temporal_a: None,
            temporal_b: Some("@t[=2024]".into()),
            title_b_in_doc_a: false,
            title_a_in_doc_b: false,
        };
        let fpc2 = FactPairContext {
            pair: pair2,
            title_a: "Company B".into(),
            title_b: "Company D".into(),
            source_defs_a: vec!["[^2]: Wikipedia".into()],
            source_defs_b: vec![],
            temporal_a: Some("@t[=1990]".into()),
            temporal_b: None,
            title_b_in_doc_a: true,
            title_a_in_doc_b: false,
        };
        let prompt = build_pair_prompt(&[&fpc1, &fpc2]);
        assert!(prompt.contains("Pair 1:"));
        assert!(prompt.contains("Pair 2:"));
        assert!(prompt.contains("Company A"));
        assert!(prompt.contains("Company C"));
        assert!(prompt.contains("Company B"));
        assert!(prompt.contains("Company D"));
        assert!(prompt.contains("@t[=2024]"));
        assert!(prompt.contains("@t[=1990]"));
        assert!(prompt.contains("Sources B: [^1]: Annual report"));
        assert!(prompt.contains("Sources A: [^2]: Wikipedia"));
    }

    #[test]
    fn test_parse_pair_response_multiple_results() {
        let json = r#"[
            {"pair":1,"status":"CONTRADICTS","reason":"different revenue"},
            {"pair":2,"status":"SUPPORTS","reason":"consistent dates"},
            {"pair":3,"status":"SUPERSEDES","reason":"newer info"}
        ]"#;
        let results = parse_pair_response(json);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pair, 1);
        assert_eq!(results[0].status, "CONTRADICTS");
        assert_eq!(results[1].pair, 2);
        assert_eq!(results[1].status, "SUPPORTS");
        assert_eq!(results[2].pair, 3);
        assert_eq!(results[2].status, "SUPERSEDES");
    }

    #[tokio::test]
    async fn test_cross_validate_facts_small_batch_size() {
        // With batch_size=1, each pair gets its own LLM call.
        // Two pairs should produce two LLM calls, but MockLlm returns
        // the same response for all calls.
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut doc_a = Document::test_default();
        doc_a.id = "bs_a".to_string();
        doc_a.file_path = "bs_a.md".to_string();
        doc_a.title = "Entity A".to_string();
        doc_a.content = "# Entity A\n\n- Revenue: $10M\n- Founded 1990\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = Document::test_default();
        doc_b.id = "bs_b".to_string();
        doc_b.file_path = "bs_b.md".to_string();
        doc_b.title = "Entity B".to_string();
        doc_b.content = "# Entity B\n\n- Revenue: $50M\n- Founded 1985\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        let pairs = vec![
            make_pair(
                make_fact("bs_a", 3, "Revenue: $10M"),
                make_fact("bs_b", 3, "Revenue: $50M"),
            ),
            make_pair(
                make_fact("bs_a", 4, "Founded 1990"),
                make_fact("bs_b", 4, "Founded 1985"),
            ),
        ];

        // MockLlm returns pair 1 as CONTRADICTS for each call
        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"CONTRADICTS","reason":"mismatch"}]"#,
        );

        // batch_size=1: each pair is its own batch, so pair index is always 1
        let result = cross_validate_facts(&pairs, &db, &llm, None, 1).await.unwrap();
        // Both pairs should generate questions (each batch has pair 1)
        let total_questions: usize = result.values().map(|qs| qs.len()).sum();
        assert_eq!(total_questions, 2, "batch_size=1 should process each pair separately");
    }

    #[tokio::test]
    async fn test_cross_validate_facts_batch_size_clamped() {
        // batch_size=0 should be clamped to 1 (minimum)
        let (db, _tmp) = test_db();
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        let mut doc_a = Document::test_default();
        doc_a.id = "cl_a".to_string();
        doc_a.file_path = "cl_a.md".to_string();
        doc_a.content = "# Doc A\n\n- Fact one\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = Document::test_default();
        doc_b.id = "cl_b".to_string();
        doc_b.file_path = "cl_b.md".to_string();
        doc_b.content = "# Doc B\n\n- Fact two\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        let pairs = vec![make_pair(
            make_fact("cl_a", 3, "Fact one"),
            make_fact("cl_b", 3, "Fact two"),
        )];

        let llm = MockLlm::new(
            r#"[{"pair":1,"status":"SUPPORTS","reason":"ok"}]"#,
        );

        // batch_size=0 should be clamped to 1, not panic
        let result = cross_validate_facts(&pairs, &db, &llm, None, 0).await.unwrap();
        assert!(result.is_empty(), "SUPPORTS generates no questions");
    }
}
