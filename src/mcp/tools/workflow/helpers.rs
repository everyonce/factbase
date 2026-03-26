//! Helper functions for workflow dispatch.
//!
//! Utility functions used by the step dispatch logic: repo resolution,
//! perspective context, question batching, quality stats, etc.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::{Document, Perspective, QuestionType};
use serde_json::Value;
use std::collections::HashMap;

use super::super::get_str_arg;
use super::super::helpers::{build_quality_stats, detect_weak_identification, resolve_doc_path};
use super::instructions::DEFAULT_BOOTSTRAP_PROMPT;
use crate::config::workflows::{resolve_workflow_text, WorkflowsConfig};

/// Resolve the filesystem path for a repository (first repo if none specified).
pub(super) fn resolve_repo_path(
    db: &Database,
    repo_id: Option<&str>,
) -> Option<std::path::PathBuf> {
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
pub(super) fn subagent_fanout_hint(
    total_questions: usize,
    type_dist: &[(String, usize)],
) -> String {
    let workers: String = type_dist
        .iter()
        .filter(|(_, c)| *c > 0)
        .enumerate()
        .map(|(i, (qt, _))| {
            format!(
                "- Worker {}: workflow(workflow='resolve', step=2, question_type='{qt}')",
                i + 1
            )
        })
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
        if let Some(wd) = obj
            .get("when_done")
            .and_then(|v| v.as_str())
            .map(String::from)
        {
            obj.insert(
                "when_done".into(),
                Value::String(wd.replace(
                    &format!("workflow='{old_name}'"),
                    &format!("workflow='{new_name}'"),
                )),
            );
        }
        if let Some(instr) = obj
            .get("instruction")
            .and_then(|v| v.as_str())
            .map(String::from)
        {
            obj.insert(
                "instruction".into(),
                Value::String(instr.replace(
                    &format!("workflow='{old_name}'"),
                    &format!("workflow='{new_name}'"),
                )),
            );
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

/// Returns true if the perspective is configured with the Obsidian preset.
pub(super) fn is_obsidian_format(p: &Option<Perspective>) -> bool {
    p.as_ref()
        .and_then(|p| p.format.as_ref())
        .and_then(|f| f.preset.as_deref())
        == Some("obsidian")
}

/// Write Obsidian CSS/app.json/gitignore files for `path` if the repo uses the
/// obsidian preset.  Returns the `obsidian_git_setup` JSON block to include in
/// the workflow response, or `None` if the preset is not obsidian.
pub(super) fn apply_obsidian_files(path: &str) -> Option<Value> {
    let repo_path = std::path::Path::new(path);
    let is_obsidian = crate::models::load_perspective_from_file(repo_path)
        .and_then(|p| p.format)
        .map(|f| f.preset.as_deref() == Some("obsidian"))
        .unwrap_or(false);
    if !is_obsidian {
        return None;
    }
    if let Err(e) = crate::models::write_obsidian_css_snippet(repo_path) {
        tracing::warn!("Failed to write Obsidian CSS snippet: {e}");
    }
    if let Err(e) = crate::models::write_obsidian_app_json(repo_path) {
        tracing::warn!("Failed to write Obsidian app.json: {e}");
    }
    if let Err(e) = crate::models::ensure_obsidian_gitignore(repo_path) {
        tracing::warn!("Failed to update .gitignore for Obsidian: {e}");
    }
    Some(serde_json::json!({
        "note": "Obsidian preset detected. Wrote .obsidian/snippets/factbase.css, .obsidian/app.json, and updated .gitignore to track them.",
        "action": "Commit these files so git pull on any Obsidian machine gets the CSS and pre-enabled snippet state.",
        "files_to_commit": [
            ".obsidian/snippets/factbase.css",
            ".obsidian/app.json",
            ".gitignore"
        ],
        "suggested_commit_message": "chore: add Obsidian CSS snippet and pre-enable state"
    }))
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
pub(super) fn resolve(
    wf: &WorkflowsConfig,
    key: &str,
    default: &str,
    vars: &[(&str, &str)],
) -> String {
    resolve_workflow_text(wf, key, default, vars)
}

/// Build the LLM prompt for domain-aware KB structure generation.
pub(super) fn build_bootstrap_prompt(
    domain: &str,
    entity_types: Option<&str>,
    prompts: &crate::config::PromptsConfig,
    repo_path: Option<&std::path::Path>,
) -> String {
    let entity_hint = entity_types
        .map(|t| format!("\nThe user has suggested these entity types: {t}"))
        .unwrap_or_default();

    let file_override = repo_path
        .and_then(|p| crate::config::prompts::load_file_override(p, "prompts/bootstrap.txt"));
    let default = file_override.as_deref().unwrap_or(DEFAULT_BOOTSTRAP_PROMPT);
    crate::config::prompts::resolve_prompt(
        prompts,
        "bootstrap",
        default,
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
            return Some((
                "no embeddings exist yet (first-time generation)".into(),
                doc_count,
            ));
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
    static RE_FOOTNOTE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[\^(\d+)\]").expect("footnote regex"));
    static RE_QUOTED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""[^"]+""#).expect("quoted text regex"));
    static RE_DATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b\d{4}(?:-\d{2}(?:-\d{2})?)?\b").expect("date regex"));
    static RE_TEMPORAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"@t\[[^\]]+\]").expect("temporal tag regex"));
    static RE_LINE_REF: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\(line \d+\)").expect("line ref regex"));

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
    patterns.sort_by(|a, b| b["count_total"].as_u64().cmp(&a["count_total"].as_u64()));
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
        QuestionType::WeakSource => "Your job is to MAKE THE CITATION MORE SPECIFIC, not to explain why it's already sufficient. If the source is a known tool with a predictable URL pattern, construct the full URL (e.g., 'Phonetool lookup' → 'Phonetool (https://phonetool.amazon.com/users/{alias}), YYYY-MM-DD'; 'Slack channel' → 'Slack #channel-name, @author, YYYY-MM-DD'). If you can find the actual source via search tools, cite the specific result. NEVER answer 'citation is sufficient' or 'no public URL available'. If you truly cannot improve it, change to '[^N]: UNVERIFIED — original claim: <original text>'. Do not invent specific-looking citations.",
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
    let repo_paths: HashMap<String, std::path::PathBuf> =
        repos.into_iter().map(|r| (r.id.clone(), r.path)).collect();

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
            let abs_path = repo_paths
                .get(&doc.repo_id)
                .map(|rp| rp.join(&doc.file_path));
            if let Some(disk) = abs_path.and_then(|p| std::fs::read_to_string(p).ok()) {
                doc.content = disk;
            }
            if crate::patterns::has_review_section(&doc.content) {
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
    with_questions
        .iter()
        .map(|(qt, _)| qt.to_string())
        .collect()
}

/// Auto-dismiss a single question by marking it answered in the document.
pub(super) fn auto_dismiss_question(
    db: &Database,
    doc_id: &str,
    question_index: usize,
) -> Result<(), FactbaseError> {
    let doc = db.require_document(doc_id)?;
    let marker = "<!-- factbase:review -->";
    let Some(marker_pos) = doc.content.find(marker) else {
        return Ok(());
    };
    let (before, after) = doc.content.split_at(marker_pos);
    let queue_content = &after[marker.len()..];

    // Mark the question as answered with a glossary note
    let answer = "Defined in glossary — auto-resolved";
    let Some(modified) = super::super::review::modify_question_in_queue(
        queue_content,
        question_index,
        answer,
        false,
    ) else {
        return Ok(());
    };
    let new_content = format!("{before}{marker}{modified}");
    let new_hash = crate::processor::content_hash(&new_content);
    db.update_document_content(doc_id, &new_content, &new_hash)?;

    // Best-effort write to disk — log warning on failure but don't fail the operation
    // since the database is already updated
    if let Ok(file_path) = resolve_doc_path(db, &doc) {
        if let Err(e) = std::fs::write(&file_path, &new_content) {
            tracing::warn!(
                "Failed to write auto-resolved question to disk for {}: {e}",
                doc_id
            );
        }
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
            let active_filter_str = type_filter
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",");
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
    let weak_count = type_distribution
        .get(&QuestionType::WeakSource)
        .copied()
        .unwrap_or(0);
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
            let f = type_filter
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(" with question_type={f}")
        };
        parts.push(format!(
            "⚠️ DO NOT STOP. {remaining} questions remain (~{batches_left} batches). Call step=2{filter_hint} immediately. You have cleared {resolved_so_far} — the DB tracks your progress, nothing is lost. Only stop if your runtime forces you to."
        ));
    } else if remaining > 100 {
        let filter_hint = if type_filter.is_empty() {
            String::new()
        } else {
            let f = type_filter
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",");
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

/// Initialize git in the KB directory if not already a git repo.
/// Writes `.gitignore`, then runs `git init && git add -A && git commit` if needed.
/// Skips gracefully if git is not installed.
/// Returns a JSON value describing what was done.
pub(super) fn apply_git_init(path: &str) -> Value {
    let repo_path = std::path::Path::new(path);
    if !repo_path.exists() {
        return serde_json::json!({"status": "skipped", "message": "path does not exist"});
    }

    // Write .gitignore
    let gitignore_updated = match crate::models::ensure_kb_gitignore(repo_path) {
        Ok(added) => added,
        Err(e) => {
            tracing::warn!("Failed to write .gitignore: {e}");
            false
        }
    };

    // Check if already a git repo
    let is_git_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(repo_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    const TIP: &str = "Your KB is git-tracked. Commit after each session to checkpoint your work:\n  `git commit -am 'maintain: YYYY-MM-DD'`\nIf you lose the database, run `factbase scan` to rebuild it from your files.";

    if is_git_repo {
        return serde_json::json!({
            "status": "already_tracked",
            "message": "KB is already git-tracked ✓",
            "gitignore_updated": gitignore_updated,
            "tip": TIP
        });
    }

    // Try git init
    let init_ok = std::process::Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !init_ok {
        return serde_json::json!({
            "status": "skipped",
            "message": "git not available or init failed — skipping git setup",
            "gitignore_updated": gitignore_updated
        });
    }

    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_path)
        .output();

    let commit_ok = std::process::Command::new("git")
        .args(["commit", "-m", "init: factbase KB bootstrap"])
        .current_dir(repo_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    serde_json::json!({
        "status": "initialized",
        "message": "KB git repository initialized ✓",
        "initial_commit": commit_ok,
        "gitignore_updated": gitignore_updated,
        "tip": TIP
    })
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
        .filter(|doc| !is_archived_path(&doc.file_path))
        .map(|doc| {
            let empty = (Vec::new(), Vec::new());
            let (outgoing, incoming) = links_map.get(&doc.id).unwrap_or(&empty);
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
            if let Some(suggested) = detect_weak_identification(&doc.title, &doc.content, &contexts)
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

/// Returns true if the file path is under an archive directory.
/// Mirrors the `is_archived` logic in `question_generator/check.rs`.
pub(super) fn is_archived_path(file_path: &str) -> bool {
    file_path.contains("/archive/") || file_path.starts_with("archive/")
}

const REFRESH_PAGE_SIZE: usize = 20;

/// Paged variant of bulk_quality for refresh step 3.
/// Returns (items, has_more, next_offset).
pub(super) fn bulk_quality_paged(
    db: &Database,
    doc_type: Option<&str>,
    repo: Option<&str>,
    offset: usize,
) -> (Vec<Value>, bool) {
    // Fetch one extra to detect if more pages remain
    let fetch = REFRESH_PAGE_SIZE + 1;
    let docs = match db.list_documents_paged(doc_type, repo, None, fetch, offset) {
        Ok(d) => d,
        Err(_) => return (Vec::new(), false),
    };
    let has_more = docs.len() == fetch;
    let docs = if has_more {
        &docs[..REFRESH_PAGE_SIZE]
    } else {
        &docs[..]
    };

    if docs.is_empty() {
        return (Vec::new(), false);
    }
    let doc_ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let links_map = db.get_links_for_documents(&doc_ids).unwrap_or_default();

    let mut items: Vec<Value> = docs
        .iter()
        .filter(|doc| !crate::patterns::is_reference_doc(&doc.content))
        .filter(|doc| !is_archived_path(&doc.file_path))
        .map(|doc| {
            let empty = (Vec::new(), Vec::new());
            let (outgoing, incoming) = links_map.get(&doc.id).unwrap_or(&empty);
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
            if let Some(suggested) = detect_weak_identification(&doc.title, &doc.content, &contexts)
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
    (items, has_more)
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
pub(super) const IMPROVE_STEPS: &[&str] = &["cleanup", "resolve", "enrich", "scan", "check"];

/// Compute the effective step sequence, skipping any steps in `skip`.
pub(super) fn effective_steps(skip: &[String]) -> Vec<(usize, &'static str)> {
    IMPROVE_STEPS
        .iter()
        .enumerate()
        .filter(|(_, name)| !skip.contains(&name.to_string()))
        .map(|(i, name)| (i + 1, *name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- normalize_question_text ---

    #[test]
    fn test_normalize_replaces_footnotes() {
        let input = r#"Source [^3] is weak — consider replacing with a specific reference"#;
        let result = normalize_question_text(input);
        assert!(
            result.contains("[^_]"),
            "Footnote should be normalized: {result}"
        );
        assert!(!result.contains("[^3]"));
    }

    #[test]
    fn test_normalize_replaces_quoted_text() {
        let input = r#""VP at BigCo" has no temporal tag"#;
        let result = normalize_question_text(input);
        assert!(
            result.contains("\"_\""),
            "Quoted text should be normalized: {result}"
        );
        assert!(!result.contains("VP at BigCo"));
    }

    #[test]
    fn test_normalize_replaces_dates() {
        let input = "Last verified 2024-06-15, stale since 2023-01";
        let result = normalize_question_text(input);
        assert!(
            !result.contains("2024"),
            "Dates should be normalized: {result}"
        );
        assert!(result.contains("_DATE_"));
    }

    #[test]
    fn test_normalize_replaces_temporal_tags() {
        let input = "Fact @t[2020..2023] overlaps with @t[2022..]";
        let result = normalize_question_text(input);
        assert!(
            result.contains("@t[_]"),
            "Temporal tags should be normalized: {result}"
        );
        assert!(!result.contains("2020..2023"));
    }

    #[test]
    fn test_normalize_replaces_line_refs() {
        let input = "Some issue (line 42)";
        let result = normalize_question_text(input);
        assert!(
            result.contains("(line _)"),
            "Line refs should be normalized: {result}"
        );
        assert!(!result.contains("42"));
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize_question_text(""), "");
    }

    // --- detect_question_patterns ---

    #[test]
    fn test_detect_patterns_below_threshold() {
        let questions: Vec<Value> = (0..3)
            .map(
                |i| json!({"type": "temporal", "description": format!("\"Fact {i}\" has no date")}),
            )
            .collect();
        let patterns = detect_question_patterns(&questions, &questions);
        assert!(
            patterns.is_empty(),
            "3 questions should be below threshold of {PATTERN_MIN_COUNT}"
        );
    }

    #[test]
    fn test_detect_patterns_above_threshold() {
        let questions: Vec<Value> = (0..5)
            .map(|i| json!({"type": "temporal", "description": format!("\"Fact {i}\" has no temporal tag")}))
            .collect();
        let patterns = detect_question_patterns(&questions, &questions);
        assert!(
            !patterns.is_empty(),
            "5 similar questions should form a pattern"
        );
        assert_eq!(patterns[0]["count_total"].as_u64().unwrap(), 5);
    }

    #[test]
    fn test_detect_patterns_empty() {
        let patterns = detect_question_patterns(&[], &[]);
        assert!(patterns.is_empty());
    }

    // --- question_type_priority ---

    #[test]
    fn test_priority_ordering() {
        assert!(
            question_type_priority(&QuestionType::Temporal)
                < question_type_priority(&QuestionType::WeakSource)
        );
        assert!(
            question_type_priority(&QuestionType::Missing)
                < question_type_priority(&QuestionType::Duplicate)
        );
        assert!(
            question_type_priority(&QuestionType::Stale)
                < question_type_priority(&QuestionType::Corruption)
        );
    }

    // --- type_evidence_guidance ---

    #[test]
    fn test_evidence_guidance_returns_nonempty() {
        for qt in [
            QuestionType::Stale,
            QuestionType::Temporal,
            QuestionType::Ambiguous,
            QuestionType::Conflict,
            QuestionType::Precision,
            QuestionType::Missing,
            QuestionType::Duplicate,
            QuestionType::Corruption,
            QuestionType::WeakSource,
        ] {
            let guidance = type_evidence_guidance(&qt);
            assert!(
                !guidance.is_empty(),
                "Guidance for {qt:?} should not be empty"
            );
        }
    }

    // --- rebrand_step ---

    #[test]
    fn test_rebrand_step_replaces_workflow() {
        let val = json!({"workflow": "old", "step": 1, "when_done": "call workflow='old' step=2"});
        let result = rebrand_step(val, "old", "new");
        assert_eq!(result["workflow"], "new");
        assert!(result["when_done"]
            .as_str()
            .unwrap()
            .contains("workflow='new'"));
    }

    #[test]
    fn test_rebrand_step_no_when_done() {
        let val = json!({"workflow": "old", "step": 1});
        let result = rebrand_step(val, "old", "new");
        assert_eq!(result["workflow"], "new");
    }

    // --- perspective_context ---

    #[test]
    fn test_perspective_context_none() {
        assert_eq!(perspective_context(&None), "");
    }

    #[test]
    fn test_perspective_context_with_org() {
        let p = Perspective {
            organization: Some("Acme Corp".into()),
            ..Default::default()
        };
        let result = perspective_context(&Some(p));
        assert!(result.contains("Acme Corp"));
    }

    #[test]
    fn test_perspective_context_with_both() {
        let p = Perspective {
            organization: Some("Acme".into()),
            focus: Some("Sales".into()),
            ..Default::default()
        };
        let result = perspective_context(&Some(p));
        assert!(result.contains("Acme"));
        assert!(result.contains("Sales"));
    }

    // --- stale_days ---

    #[test]
    fn test_stale_days_default() {
        assert_eq!(stale_days(&None), 365);
    }

    #[test]
    fn test_stale_days_from_perspective() {
        let p = Perspective {
            review: Some(crate::models::ReviewPerspective {
                stale_days: Some(90),
                stale_days_by_type: None,
                source_types: None,
                ignore_patterns: None,
                required_fields: None,
                glossary_types: None,
                suppress_question_types: vec![],
                suppress_question_types_by_type: None,
            }),
            ..Default::default()
        };
        assert_eq!(stale_days(&Some(p)), 90);
    }

    // --- required_fields_hint ---

    #[test]
    fn test_required_fields_hint_none() {
        assert_eq!(required_fields_hint(&None), "");
    }

    #[test]
    fn test_required_fields_hint_with_fields() {
        let mut fields = HashMap::new();
        fields.insert(
            "person".to_string(),
            vec!["name".to_string(), "role".to_string()],
        );
        let p = Perspective {
            review: Some(crate::models::ReviewPerspective {
                stale_days: None,
                stale_days_by_type: None,
                source_types: None,
                ignore_patterns: None,
                required_fields: Some(fields),
                glossary_types: None,
                suppress_question_types: vec![],
                suppress_question_types_by_type: None,
            }),
            ..Default::default()
        };
        let result = required_fields_hint(&Some(p));
        assert!(result.contains("person"));
        assert!(result.contains("name"));
    }

    // --- subagent_fanout_hint ---

    #[test]
    fn test_fanout_hint_small_queue() {
        let dist = vec![("temporal".into(), 10), ("stale".into(), 5)];
        let result = subagent_fanout_hint(15, &dist);
        assert!(result.contains("PARALLEL DISPATCH"));
        assert!(!result.contains("MANDATORY"));
    }

    #[test]
    fn test_fanout_hint_large_queue() {
        let dist = vec![("temporal".into(), 150), ("stale".into(), 100)];
        let result = subagent_fanout_hint(250, &dist);
        assert!(result.contains("MANDATORY"));
    }

    #[test]
    fn test_fanout_hint_skips_zero_count() {
        let dist = vec![("temporal".into(), 10), ("stale".into(), 0)];
        let result = subagent_fanout_hint(10, &dist);
        assert!(result.contains("temporal"));
        assert!(!result.contains("question_type='stale'"));
    }

    // --- parse_skip_steps ---

    #[test]
    fn test_parse_skip_steps_none() {
        let args = json!({});
        assert!(parse_skip_steps(&args).is_empty());
    }

    #[test]
    fn test_parse_skip_steps_string() {
        let args = json!({"skip": "cleanup,resolve"});
        let result = parse_skip_steps(&args);
        assert_eq!(result, vec!["cleanup", "resolve"]);
    }

    #[test]
    fn test_parse_skip_steps_array() {
        let args = json!({"skip": ["Cleanup", "Check"]});
        let result = parse_skip_steps(&args);
        assert_eq!(result, vec!["cleanup", "check"]);
    }

    // --- effective_steps ---

    #[test]
    fn test_effective_steps_no_skip() {
        let steps = effective_steps(&[]);
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], (1, "cleanup"));
        assert_eq!(steps[3], (4, "scan"));
        assert_eq!(steps[4], (5, "check"));
    }

    #[test]
    fn test_effective_steps_skip_some() {
        let skip = vec!["resolve".to_string(), "check".to_string()];
        let steps = effective_steps(&skip);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].1, "cleanup");
        assert_eq!(steps[1].1, "enrich");
        assert_eq!(steps[2].1, "scan");
    }

    // --- apply_git_init ---

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_apply_git_init_new_repo() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        // Write a file so there's something to commit
        std::fs::write(tmp.path().join("README.md"), "# Test KB\n").unwrap();

        let result = apply_git_init(&path);
        assert_eq!(result["status"], "initialized");
        assert!(tmp.path().join(".git").exists(), ".git should be created");
        assert!(
            tmp.path().join(".gitignore").exists(),
            ".gitignore should be created"
        );
        let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".factbase/factbase.db"));
        assert_eq!(result["gitignore_updated"], true);
        assert!(result["tip"].as_str().unwrap().contains("factbase scan"));
    }

    #[test]
    fn test_apply_git_init_existing_repo() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        // Initialize git first
        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let result = apply_git_init(&path);
        assert_eq!(result["status"], "already_tracked");
        assert!(result["message"]
            .as_str()
            .unwrap()
            .contains("already git-tracked"));
        // .gitignore should still be written
        assert!(
            tmp.path().join(".gitignore").exists(),
            ".gitignore should be created even for existing repos"
        );
    }

    #[test]
    fn test_apply_git_init_existing_repo_gitignore_already_present() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        // Pre-populate .gitignore with the key entry
        std::fs::write(tmp.path().join(".gitignore"), ".factbase/factbase.db\n").unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let result = apply_git_init(&path);
        assert_eq!(result["status"], "already_tracked");
        assert_eq!(
            result["gitignore_updated"], false,
            "should not modify existing gitignore"
        );
    }

    #[test]
    fn test_apply_git_init_nonexistent_path() {
        let result = apply_git_init("/nonexistent/path/that/does/not/exist");
        assert_eq!(result["status"], "skipped");
    }

    // --- recommended_resolve_order ---

    #[test]
    fn test_recommended_order_fewest_first() {
        let dist = vec![
            (QuestionType::Stale, 50),
            (QuestionType::Temporal, 10),
            (QuestionType::Missing, 30),
        ];
        let order = recommended_resolve_order(&dist);
        assert_eq!(order[0], "temporal");
        assert_eq!(order[1], "missing");
        assert_eq!(order[2], "stale");
    }

    #[test]
    fn test_recommended_order_skips_zero() {
        let dist = vec![(QuestionType::Stale, 0), (QuestionType::Temporal, 5)];
        let order = recommended_resolve_order(&dist);
        assert_eq!(order.len(), 1);
        assert_eq!(order[0], "temporal");
    }

    // --- build_continuation_guidance ---

    #[test]
    fn test_continuation_guidance_large_queue() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 300);
        let result = build_continuation_guidance(600, 100, 20, &dist, &[]);
        assert!(result.is_some());
        assert!(result.unwrap().contains("DO NOT STOP"));
    }

    #[test]
    fn test_continuation_guidance_empty_queue() {
        let dist = HashMap::new();
        let result = build_continuation_guidance(0, 50, 20, &dist, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_continuation_guidance_context_management() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Stale, 100);
        let result = build_continuation_guidance(100, 200, 20, &dist, &[]);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(
            text.contains("CONTEXT MANAGEMENT"),
            "Should include context hint: {text}"
        );
    }

    #[test]
    fn test_continuation_guidance_type_cleared() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 0);
        dist.insert(QuestionType::Stale, 20);
        let filter = vec![QuestionType::Temporal];
        let result = build_continuation_guidance(20, 10, 20, &dist, &filter);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("stale"), "Should suggest next type: {text}");
    }
}
