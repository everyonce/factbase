//! Helper functions for workflow dispatch.
//!
//! Utility functions used by the step dispatch logic: repo resolution,
//! perspective context, question batching, quality stats, etc.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::{Document, Perspective, QuestionType};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::config::workflows::{resolve_workflow_text, WorkflowsConfig};
use super::instructions::DEFAULT_BOOTSTRAP_PROMPT;
use super::super::helpers::{build_quality_stats, detect_weak_identification, resolve_doc_path};
use super::super::get_str_arg;

/// Resolve the filesystem path for a repository (first repo if none specified).
pub(super) fn resolve_repo_path(db: &Database, repo_id: Option<&str>) -> Option<std::path::PathBuf> {
    let repos = db.list_repositories().ok()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.map(|r| r.path)
}

// ---------------------------------------------------------------------------

/// Build the sub-agent fanout hint. For large queues (>200) the language is
/// directive ("DO IT NOW"); for smaller queues it remains a strong suggestion.
pub(super) fn subagent_fanout_hint(total_questions: usize, type_dist: &[(String, usize)]) -> String {
    let workers: String = type_dist
        .iter()
        .filter(|(_, c)| *c > 0)
        .enumerate()
        .map(|(i, (qt, _))| format!("- Worker {}: workflow(workflow='resolve', step=2, question_type='{qt}')", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    if total_questions > 200 {
        format!(
            "\n\n🚀 PARALLEL DISPATCH (MANDATORY for {total_questions} questions): If you have a use_subagent or delegate tool, USE IT NOW to spawn parallel workers. \
Spawn one worker per question type. This is the fastest path to clearing the queue.\n\
{workers}\n\
Each worker processes its type independently. Do NOT work serially — fan out immediately."
        )
    } else {
        format!(
            "\n\n🚀 PARALLEL DISPATCH: If you have a use_subagent or delegate tool, spawn one worker per question type — \
this is significantly faster than serial processing.\n\
{workers}\n\
Each worker processes its type independently."
        )
    }
}

/// Patch the `workflow` and `when_done` fields in a step response to use a different workflow name.
pub(super) fn rebrand_step(mut val: Value, old_name: &str, new_name: &str) -> Value {
    if let Some(obj) = val.as_object_mut() {
        obj.insert("workflow".into(), Value::String(new_name.into()));
        if let Some(wd) = obj.get("when_done").and_then(|v| v.as_str()).map(String::from) {
            obj.insert("when_done".into(), Value::String(wd.replace(
                &format!("workflow='{old_name}'"),
                &format!("workflow='{new_name}'"),
            )));
        }
        if let Some(instr) = obj.get("instruction").and_then(|v| v.as_str()).map(String::from) {
            obj.insert("instruction".into(), Value::String(instr.replace(
                &format!("workflow='{old_name}'"),
                &format!("workflow='{new_name}'"),
            )));
        }
    }
    val
}

/// Build a context string from perspective for embedding in instructions.
pub(super) fn perspective_context(p: &Option<Perspective>) -> String {
    let Some(p) = p else { return String::new() };
    let mut parts = Vec::new();
    if let Some(ref org) = p.organization {
        parts.push(format!("Organization: {org}"));
    }
    if let Some(ref focus) = p.focus {
        parts.push(format!("Focus: {focus}"));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("\n\nKnowledge base context: {}", parts.join(". "))
}

pub(super) fn stale_days(p: &Option<Perspective>) -> i64 {
    p.as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.stale_days)
        .map_or(365, |d| d as i64)
}

pub(super) fn required_fields_hint(p: &Option<Perspective>) -> String {
    let Some(fields) = p
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.required_fields.as_ref())
    else {
        return String::new();
    };
    let hints: Vec<String> = fields
        .iter()
        .map(|(doc_type, fields)| {
            format!("  - {} docs should have: {}", doc_type, fields.join(", "))
        })
        .collect();
    if hints.is_empty() {
        return String::new();
    }
    format!(
        "\n\nRequired fields per document type:\n{}",
        hints.join("\n")
    )
}

/// Resolve a workflow instruction with config override support.
pub(super) fn resolve(wf: &WorkflowsConfig, key: &str, default: &str, vars: &[(&str, &str)]) -> String {
    resolve_workflow_text(wf, key, default, vars)
}

/// Build the LLM prompt for domain-aware KB structure generation.
pub(super) fn build_bootstrap_prompt(
    domain: &str,
    entity_types: Option<&str>,
    prompts: &crate::config::PromptsConfig,
) -> String {
    let entity_hint = entity_types
        .map(|t| format!("\nThe user has suggested these entity types: {t}"))
        .unwrap_or_default();

    crate::config::prompts::resolve_prompt(
        prompts,
        "bootstrap",
        DEFAULT_BOOTSTRAP_PROMPT,
        &[("domain", domain), ("entity_types", &entity_hint)],
    )
}

/// Returns `None` for incremental, or `Some((reason, doc_count))` for full rebuild.
pub(super) fn detect_full_rebuild(db: &Database) -> Option<(String, usize)> {
    let config = crate::Config::load(None).unwrap_or_default();
    let doc_count = db.get_all_document_ids().ok()?.len();
    if doc_count == 0 {
        return None;
    }

    // Check dimension mismatch
    if let Ok(Some(stored_dim)) = db.get_stored_embedding_dim() {
        if stored_dim != config.embedding.dimension {
            return Some((
                format!(
                    "embedding dimension changed ({stored_dim} → {})",
                    config.embedding.dimension
                ),
                doc_count,
            ));
        }
    }

    // Check model change
    if let Ok(Some(stored_model)) = db.get_stored_embedding_model() {
        if stored_model != config.embedding.model {
            return Some((
                format!(
                    "embedding model changed ({stored_model} → {})",
                    config.embedding.model
                ),
                doc_count,
            ));
        }
    }

    // Check empty embeddings with existing documents
    if let Ok(chunk_count) = db.count_embedding_chunks() {
        if chunk_count == 0 {
            return Some(("no embeddings exist yet (first-time generation)".into(), doc_count));
        }
    }

    None
}

/// Minimum group size to surface as a repetitive pattern.
pub(super) const PATTERN_MIN_COUNT: usize = 4;

/// Normalize a question description by replacing variable parts with placeholders.
/// This groups questions that follow the same template (e.g., same weak-source wording
/// across hundreds of documents differing only in footnote number and source text).
pub(super) fn normalize_question_text(desc: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static RE_FOOTNOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\^(\d+)\]").unwrap());
    static RE_QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""[^"]+""#).unwrap());
    static RE_DATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b\d{4}(?:-\d{2}(?:-\d{2})?)?\b").unwrap());
    static RE_TEMPORAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"@t\[[^\]]+\]").unwrap());
    static RE_LINE_REF: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\(line \d+\)").unwrap());

    let s = RE_FOOTNOTE.replace_all(desc, "[^_]");
    let s = RE_QUOTED.replace_all(&s, "\"_\"");
    let s = RE_TEMPORAL.replace_all(&s, "@t[_]");
    let s = RE_DATE.replace_all(&s, "_DATE_");
    let s = RE_LINE_REF.replace_all(&s, "(line _)");
    s.into_owned()
}

/// Detect repetitive question patterns from a list of unanswered question JSON values.
/// Returns a JSON array of patterns where count_total > PATTERN_MIN_COUNT.
pub(super) fn detect_question_patterns(all_questions: &[Value], batch: &[Value]) -> Vec<Value> {
    // Group all questions by (type, normalized description)
    let mut totals: HashMap<(String, String), (usize, String)> = HashMap::new();
    for q in all_questions {
        let qtype = q["type"].as_str().unwrap_or("").to_string();
        let desc = q["description"].as_str().unwrap_or("");
        let key = (qtype, normalize_question_text(desc));
        totals
            .entry(key)
            .and_modify(|(c, _)| *c += 1)
            .or_insert((1, desc.to_string()));
    }

    // Count per-batch occurrences for patterns that exceed threshold
    let mut batch_counts: HashMap<(String, String), usize> = HashMap::new();
    for q in batch {
        let qtype = q["type"].as_str().unwrap_or("").to_string();
        let desc = q["description"].as_str().unwrap_or("");
        let key = (qtype, normalize_question_text(desc));
        *batch_counts.entry(key).or_insert(0) += 1;
    }

    let mut patterns: Vec<Value> = totals
        .iter()
        .filter(|(_, (count, _))| *count >= PATTERN_MIN_COUNT)
        .map(|((qtype, normalized), (count_total, example))| {
            let count_in_batch = batch_counts
                .get(&(qtype.clone(), normalized.clone()))
                .copied()
                .unwrap_or(0);
            serde_json::json!({
                "pattern": normalized,
                "question_type": qtype,
                "count_in_batch": count_in_batch,
                "count_total": count_total,
                "example": example,
                "suggestion": format!(
                    "These {} questions all follow the same pattern. \
                     Consider applying a consistent answer to all of them.",
                    count_total
                ),
            })
        })
        .collect();

    // Sort by count descending for readability
    patterns.sort_by(|a, b| {
        b["count_total"]
            .as_u64()
            .cmp(&a["count_total"].as_u64())
    });
    patterns
}

/// Priority ordering for question types within a batch.
/// Lower number = higher priority (processed first).
pub(super) fn question_type_priority(qt: &QuestionType) -> u8 {
    match qt {
        QuestionType::Temporal => 0,
        QuestionType::Missing => 1,
        QuestionType::Stale => 2,
        QuestionType::Conflict => 3,
        QuestionType::Ambiguous => 4,
        QuestionType::Precision => 5,
        QuestionType::Duplicate => 6,
        QuestionType::Corruption => 7,
        QuestionType::WeakSource => 8,
    }
}

/// Returns type-specific evidence guidance for Variant A.
pub(super) fn type_evidence_guidance(qt: &QuestionType) -> &'static str {
    match qt {
        QuestionType::Stale => "Search for the claim + current year. Cite a URL confirming or updating it. Wikipedia is acceptable for well-established facts.",
        QuestionType::Temporal => "Search for the specific event date. Cite a URL with the date. Format: @t[YYYY-MM-DD] per [source] ([URL]); verified YYYY-MM-DD",
        QuestionType::Ambiguous => "Check the KB first (factbase(op='get_entity'), read other docs). If the term is defined elsewhere in the KB, cite that doc. Only search externally if KB has no answer.",
        QuestionType::Conflict => "Read BOTH referenced documents (factbase(op='get_entity')). Search for the specific claim in each. Compare sources by recency and authority. If genuinely unresolvable, defer with analysis of both sides.",
        QuestionType::Precision => "Search for a quantitative replacement. If no specific number exists in sources, defer — do not guess.",
        QuestionType::Missing => "Find a source citation for this unsourced fact. Search for the specific claim and cite a URL.",
        QuestionType::Duplicate => "Identify the canonical entry by reading both documents. Cite which one should be kept.",
        QuestionType::Corruption => "Read the document to identify the corruption. Describe what needs to be fixed.",
        QuestionType::WeakSource => "Find the specific source using your available tools. Update the footnote with a specific, independently verifiable reference: URL, document path, page number, ISBN, RFC, channel/thread ID, etc. If you cannot find the source, change the footnote to '[^N]: UNVERIFIED — original claim: <original text>'. Do not invent specific-looking citations.",
    }
}

/// Load all documents with their disk content (preferred) or DB content (fallback),
/// filtered to only those that have a review queue section.
///
/// The `has_review_queue` DB flag can be stale when files are edited externally or
/// when check_repository writes questions to disk but the DB update is lost.
/// Reading from disk ensures the resolve workflow sees the filesystem truth.
pub(super) fn load_review_docs_from_disk(db: &Database) -> Vec<Document> {
    // Build repo_id → repo_path map
    let repos = db.list_repositories().unwrap_or_default();
    let repo_paths: HashMap<String, std::path::PathBuf> = repos
        .into_iter()
        .map(|r| (r.id.clone(), r.path))
        .collect();

    // Load ALL documents (not just has_review_queue=TRUE)
    let mut all_docs = Vec::new();
    for repo_id in repo_paths.keys() {
        if let Ok(docs) = db.get_documents_for_repo(repo_id) {
            all_docs.extend(docs.into_values().filter(|d| !d.is_deleted));
        }
    }

    // For each document, prefer disk content; keep only those with review queues
    all_docs
        .into_iter()
        .filter_map(|mut doc| {
            let abs_path = repo_paths.get(&doc.repo_id).map(|rp| rp.join(&doc.file_path));
            if let Some(disk) = abs_path.and_then(|p| std::fs::read_to_string(p).ok()) {
                doc.content = disk;
            }
            if doc.content.contains(crate::patterns::REVIEW_QUEUE_MARKER) {
                Some(doc)
            } else {
                None
            }
        })
        .collect()
}

/// Compute unanswered question type distribution from the review queue.
pub(super) fn compute_type_distribution(db: &Database) -> Vec<(QuestionType, usize)> {
    let docs = load_review_docs_from_disk(db);
    let (counts, _) = crate::mcp::tools::review::count_question_types(&docs);
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted
}

/// Compute recommended type processing order: fewest questions first (quick wins),
/// with difficulty as tiebreaker for equal counts.
pub(super) fn recommended_resolve_order(dist: &[(QuestionType, usize)]) -> Vec<String> {
    let mut with_questions: Vec<_> = dist.iter().filter(|(_, c)| *c > 0).collect();
    with_questions.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| question_type_priority(&a.0).cmp(&question_type_priority(&b.0)))
    });
    with_questions.iter().map(|(qt, _)| qt.to_string()).collect()
}

/// Load glossary terms from all repositories.
pub(super) fn load_all_glossary_terms(db: &Database) -> HashSet<String> {
    let types = ["definition", "glossary", "reference"];
    let mut terms = HashSet::new();
    for t in &types {
        if let Ok(docs) = db.list_documents(Some(t), None, None, 100) {
            for doc in &docs {
                terms.extend(crate::extract_defined_terms(&doc.content));
            }
        }
    }
    terms
}

/// Auto-dismiss a single question by marking it answered in the document.
pub(super) fn auto_dismiss_question(db: &Database, doc_id: &str, question_index: usize) -> Result<(), FactbaseError> {
    let doc = db.require_document(doc_id)?;
    let marker = "<!-- factbase:review -->";
    let Some(marker_pos) = doc.content.find(marker) else {
        return Ok(());
    };
    let (before, after) = doc.content.split_at(marker_pos);
    let queue_content = &after[marker.len()..];

    // Mark the question as answered with a glossary note
    let answer = "Defined in glossary — auto-resolved";
    let Some(modified) = super::super::review::answer::modify_question_in_queue(queue_content, question_index, answer, false) else {
        return Ok(());
    };
    let new_content = format!("{before}{marker}{modified}");
    let new_hash = crate::processor::content_hash(&new_content);
    db.update_document_content(doc_id, &new_content, &new_hash)?;

    // Also write to disk if possible
    if let Ok(file_path) = resolve_doc_path(db, &doc) {
        let _ = std::fs::write(&file_path, &new_content);
    }
    Ok(())
}

/// sorts them (grouped by document, then by type priority), and returns the
/// next batch. The agent just answers what it sees and calls step 2 again.
/// Build directive continuation guidance based on queue size and type distribution.
#[cfg(test)]
pub(super) fn build_continuation_guidance(
    remaining: usize,
    resolved_so_far: usize,
    batch_size: usize,
    type_distribution: &HashMap<QuestionType, usize>,
    type_filter: &[QuestionType],
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // Check if the currently-filtered type is fully cleared
    if !type_filter.is_empty() {
        let filtered_remaining: usize = type_filter
            .iter()
            .filter_map(|t| type_distribution.get(t))
            .sum();
        if filtered_remaining == 0 {
            // Find next type with remaining questions
            let active_filter_str = type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
            let mut others: Vec<_> = type_distribution
                .iter()
                .filter(|(qt, c)| **c > 0 && !type_filter.contains(qt))
                .collect();
            others.sort_by(|a, b| b.1.cmp(a.1));
            if let Some((next_type, next_count)) = others.first() {
                parts.push(format!(
                    "✅ {active_filter_str}: 0 remaining. Move to next type: {next_type} ({next_count} remaining). Call step=2 question_type={next_type}."
                ));
            }
        }
    }

    // Check if only weak-source remains
    let non_weak: usize = type_distribution
        .iter()
        .filter(|(qt, _)| **qt != QuestionType::WeakSource)
        .map(|(_, c)| c)
        .sum();
    let weak_count = type_distribution.get(&QuestionType::WeakSource).copied().unwrap_or(0);
    if non_weak == 0 && weak_count > 0 {
        parts.push(format!(
            "Only weak-source remains ({weak_count}). These follow repetitive patterns — maintain a consistent answer format to maximize throughput."
        ));
    }

    // Assertive continuation directive — agents quit too early without strong language
    if remaining > 500 {
        let batches_left = remaining.div_ceil(batch_size);
        let filter_hint = if type_filter.is_empty() {
            String::new()
        } else {
            let f = type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
            format!(" with question_type={f}")
        };
        parts.push(format!(
            "⚠️ DO NOT STOP. {remaining} questions remain (~{batches_left} batches). Call step=2{filter_hint} immediately. You have cleared {resolved_so_far} — the DB tracks your progress, nothing is lost. Only stop if your runtime forces you to."
        ));
    } else if remaining > 100 {
        let filter_hint = if type_filter.is_empty() {
            String::new()
        } else {
            let f = type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
            format!(" with question_type={f}")
        };
        parts.push(format!(
            "⚠️ DO NOT STOP. {remaining} questions remain. Call step=2{filter_hint} immediately. You have cleared {resolved_so_far} — keep going. Only stop if your runtime forces you to."
        ));
    }

    // Always add anti-early-stopping directive when there are remaining questions
    if remaining > 0 {
        parts.push("⚠️ Agents commonly quit too early. Unless your runtime is forcing you to stop, keep calling step 2. The DB tracks your progress — nothing is lost if you eventually hit a limit.".to_string());
    }

    // Context management hint — runtime-agnostic advice for long sessions
    if resolved_so_far > batch_size * 8 && remaining > batch_size * 2 {
        parts.push(
            "💡 CONTEXT MANAGEMENT: If your context is getting full, you can safely compact/summarize your earlier work. \
            Your progress is saved in the DB — answered questions never reappear. You only need to retain: \
            (1) call workflow(workflow='resolve', step=2) to get the next batch, \
            (2) answer each question with doc_id, question_index, answer, confidence. \
            Everything else (earlier batches, analysis, commentary) can be discarded."
                .to_string(),
        );
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Build quality stats JSON for a single entity by doc_id.
pub(super) fn entity_quality(db: &Database, doc_id: &str) -> Option<Value> {
    let doc = db.get_document(doc_id).ok()??;
    let outgoing_links = db.get_links_from(doc_id).unwrap_or_default();
    let incoming_links = db.get_links_to(doc_id).unwrap_or_default();
    let mut stats = build_quality_stats(&doc.content, outgoing_links.len(), incoming_links.len());
    let contexts: Vec<&str> = incoming_links
        .iter()
        .filter_map(|l| l.context.as_deref())
        .filter(|c| !c.is_empty())
        .collect();
    if let Some(suggested) = detect_weak_identification(&doc.title, &doc.content, &contexts) {
        let obj = stats.as_object_mut().unwrap();
        obj.insert("weak_identification".into(), Value::String(suggested));
        let score = obj["attention_score"].as_u64().unwrap_or(0);
        obj.insert("attention_score".into(), Value::Number((score + 3).into()));
    }
    Some(stats)
}

/// Build a bulk quality summary for all entities, sorted by attention_score descending.
pub(super) fn bulk_quality(db: &Database, doc_type: Option<&str>, repo: Option<&str>) -> Value {
    let docs = match db.list_documents(doc_type, repo, None, 200) {
        Ok(d) => d,
        Err(_) => return Value::Null,
    };
    if docs.is_empty() {
        return serde_json::json!([]);
    }
    let doc_ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let links_map = db.get_links_for_documents(&doc_ids).unwrap_or_default();

    let mut items: Vec<Value> = docs
        .iter()
        .filter(|doc| !crate::patterns::is_reference_doc(&doc.content))
        .map(|doc| {
            let empty = (Vec::new(), Vec::new());
            let (outgoing, incoming) = links_map
                .get(&doc.id)
                .unwrap_or(&empty);
            let mut stats = build_quality_stats(&doc.content, outgoing.len(), incoming.len());
            let obj = stats.as_object_mut().unwrap();
            obj.insert("id".into(), Value::String(doc.id.clone()));
            obj.insert("title".into(), Value::String(doc.title.clone()));
            if let Some(ref t) = doc.doc_type {
                obj.insert("type".into(), Value::String(t.clone()));
            }
            let contexts: Vec<&str> = incoming
                .iter()
                .filter_map(|l| l.context.as_deref())
                .filter(|c| !c.is_empty())
                .collect();
            if let Some(suggested) =
                detect_weak_identification(&doc.title, &doc.content, &contexts)
            {
                obj.insert("weak_identification".into(), Value::String(suggested));
                let score = obj["attention_score"].as_u64().unwrap_or(0);
                obj.insert("attention_score".into(), Value::Number((score + 3).into()));
            }
            stats
        })
        .collect();

    items.sort_by(|a, b| {
        let sa = a["attention_score"].as_u64().unwrap_or(0);
        let sb = b["attention_score"].as_u64().unwrap_or(0);
        sb.cmp(&sa)
    });
    Value::Array(items)
}

/// Parse the `skip` parameter into a list of step names to skip.
pub(super) fn parse_skip_steps(args: &Value) -> Vec<String> {
    if let Some(arr) = args.get("skip").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
            .collect()
    } else if let Some(s) = get_str_arg(args, "skip") {
        s.split(',').map(|s| s.trim().to_lowercase()).collect()
    } else {
        Vec::new()
    }
}

/// Map logical step names to their step numbers for the improve workflow.
pub(super) const IMPROVE_STEPS: &[&str] = &["cleanup", "resolve", "enrich", "check"];

/// Compute the effective step sequence, skipping any steps in `skip`.
pub(super) fn effective_steps(skip: &[String]) -> Vec<(usize, &'static str)> {
    IMPROVE_STEPS
        .iter()
        .enumerate()
        .filter(|(_, name)| !skip.contains(&name.to_string()))
        .map(|(i, name)| (i + 1, *name))
        .collect()
}
