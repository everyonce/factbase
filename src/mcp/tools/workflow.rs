//! Guided workflow tools for AI agents.
//!
//! Provides step-by-step instructions that agents follow, calling other
//! factbase MCP tools along the way. The workflow tools don't do work
//! themselves — they just tell the agent what to do next.
//!
//! Workflows read the repository perspective to tailor instructions
//! to the knowledge base's purpose and policies.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::Perspective;
use serde_json::Value;

use super::{get_str_arg, get_str_arg_required, get_u64_arg};

/// Start a guided workflow.
pub fn workflow(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let workflow = get_str_arg_required(args, "workflow")?;
    let step = get_u64_arg(args, "step", 1) as usize;
    let perspective = load_perspective(db, get_str_arg(args, "repo"));

    match workflow.as_str() {
        "update" => Ok(update_step(step, args, &perspective)),
        "resolve" => {
            let deferred = count_deferred(db, get_str_arg(args, "repo"));
            Ok(resolve_step(step, args, &perspective, deferred))
        }
        "ingest" => Ok(ingest_step(step, args, &perspective)),
        "enrich" => Ok(enrich_step(step, args, &perspective)),
        "list" => Ok(serde_json::json!({
            "workflows": [
                {"name": "update", "description": "Scan, check quality, find duplicates, and report what needs attention"},
                {"name": "resolve", "description": "Fix quality issues by resolving review queue questions using external sources"},
                {"name": "ingest", "description": "Research a topic and create/update factbase documents"},
                {"name": "enrich", "description": "Find and fill gaps in existing documents"}
            ]
        })),
        _ => Ok(serde_json::json!({
            "error": format!("Unknown workflow '{}'. Call workflow with workflow='list' to see available workflows.", workflow)
        })),
    }
}

/// Load perspective from the first (or specified) repository.
fn load_perspective(db: &Database, repo_id: Option<&str>) -> Option<Perspective> {
    let repos = db.list_repositories().ok()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.and_then(|r| r.perspective)
}

/// Count deferred review items across documents.
fn count_deferred(db: &Database, repo_id: Option<&str>) -> usize {
    let Ok(docs) = db.get_documents_with_review_queue(repo_id) else {
        return 0;
    };
    docs.iter()
        .filter_map(|d| crate::processor::parse_review_queue(&d.content))
        .flatten()
        .filter(|q| !q.answered && q.answer.is_some())
        .count()
}

/// Build a context string from perspective for embedding in instructions.
fn perspective_context(p: &Option<Perspective>) -> String {
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

fn stale_days(p: &Option<Perspective>) -> i64 {
    p.as_ref()
        .and_then(|p| p.review.as_ref())
        .and_then(|r| r.stale_days)
        .map_or(365, |d| d as i64)
}

fn required_fields_hint(p: &Option<Perspective>) -> String {
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

fn update_step(step: usize, _args: &Value, perspective: &Option<Perspective>) -> Value {
    let ctx = perspective_context(perspective);
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "update",
            "step": 1, "total_steps": total,
            "instruction": format!("Re-index the factbase to pick up any file changes. Call scan_repository. Tell the user this may take a minute for large repositories.{ctx}"),
            "next_tool": "scan_repository",
            "when_done": "Call workflow with workflow='update', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "update",
            "step": 2, "total_steps": total,
            "instruction": "Run a deep quality check across all documents. Call lint_repository — this performs local checks (temporal tags, sources, stale facts) AND cross-document validation using the LLM, so it may take several minutes on large knowledge bases. Tell the user this step takes a while before calling it.",
            "next_tool": "lint_repository",
            "suggested_args": {"dry_run": false},
            "when_done": "Call workflow with workflow='update', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "update",
            "step": 3, "total_steps": total,
            "instruction": "Check for duplicate entity entries across documents. Call get_duplicate_entries to find entities mentioned in multiple places that may be stale or redundant.",
            "next_tool": "get_duplicate_entries",
            "when_done": "Call workflow with workflow='update', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "update",
            "step": 4, "total_steps": total,
            "instruction": "Summarize what you found: how many documents were scanned, how many quality issues were identified, how many duplicates were detected. If there are issues to fix, suggest the user run the 'resolve' workflow next.",
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "update",
            "complete": true,
            "instruction": "Workflow complete."
        }),
    }
}

fn resolve_step(step: usize, _args: &Value, perspective: &Option<Perspective>, deferred: usize) -> Value {
    let ctx = perspective_context(perspective);
    let stale = stale_days(perspective);
    let total = 4;
    let deferred_note = if deferred > 0 {
        format!("\n\nYou have {deferred} deferred item(s) that need human attention. Call get_deferred_items first to review them before proceeding with new questions.")
    } else {
        String::new()
    };
    match step {
        1 => serde_json::json!({
            "workflow": "resolve",
            "step": 1, "total_steps": total,
            "instruction": format!("Get the review queue to see what needs fixing. Call get_review_queue with include_context=true. If there are many questions, filter by type (stale, conflict, missing, temporal, ambiguous, duplicate) to work in batches.{ctx}{deferred_note}"),
            "next_tool": "get_review_queue",
            "suggested_args": {"include_context": true},
            "policy": {"stale_days": stale},
            "deferred_count": deferred,
            "when_done": "Call workflow with workflow='resolve', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "resolve",
            "step": 2, "total_steps": total,
            "instruction": format!("For each unanswered question, resolve it:\n\n1. Read the question description and context lines. Check if a previous attempt left a note — avoid repeating the same search.\n2. Use your other tools (web search, APIs, file access) to research the answer\n3. Call answer_question with doc_id, question_index, and your answer\n\nHow to answer each type:\n- stale: Source is older than {stale} days. Search for current info. Answer: 'Still accurate per [source], verified [date]' or 'Updated: [new info] per [source]'\n- missing: Find a source. Answer: 'Source: [type], [date]'\n- conflict: Determine which is current. This includes cross-document conflicts where a fact in one document contradicts another document. Check both documents and resolve. Answer: '[fact A] is current, [fact B] ended [date] per [source]'\n- temporal: Research the date. Answer: 'Started [date] per [source]'\n- ambiguous: Clarify. Answer: 'This refers to [clarification] per [source]'\n- duplicate: Identify canonical. Answer: 'Duplicate of [doc_id], remove from here'\n\nCross-document conflict questions include the source document ID in their description — use get_entity to read that document for context.\n\nIf you cannot find sufficient data to resolve a question, defer it instead of guessing: answer with 'defer: <what you searched and why it was insufficient>'. This leaves the question in the queue with your note for future reviewers.\n\nAlways include your source.{ctx}"),
            "next_tool": "answer_question",
            "when_done": "After resolving questions, call workflow with workflow='resolve', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "resolve",
            "step": 3, "total_steps": total,
            "instruction": "Apply your answered questions to the actual document content. Call apply_review_answers to rewrite documents based on your answers. Use dry_run=true first to preview, then without dry_run to apply.",
            "next_tool": "apply_review_answers",
            "suggested_args": {"dry_run": false},
            "when_done": "Call workflow with workflow='resolve', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "resolve",
            "step": 4, "total_steps": total,
            "instruction": "Verify your work. For each document you modified, call generate_questions with dry_run=true to check if your answers introduced new issues. If new questions appear, resolve them now.",
            "next_tool": "generate_questions",
            "suggested_args": {"dry_run": true},
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "resolve",
            "complete": true,
            "instruction": "Workflow complete. All review questions have been processed."
        }),
    }
}

fn ingest_step(step: usize, args: &Value, perspective: &Option<Perspective>) -> Value {
    let topic = get_str_arg(args, "topic").unwrap_or("the requested topic");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "ingest",
            "step": 1, "total_steps": total,
            "instruction": format!("Search factbase to see what already exists about '{topic}'. Call search_knowledge with a relevant query. Also try list_entities to browse by type.{ctx}"),
            "next_tool": "search_knowledge",
            "when_done": "Call workflow with workflow='ingest', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "ingest",
            "step": 2, "total_steps": total,
            "instruction": format!("Research '{topic}' using your other tools (web search, APIs, files, etc.). Gather facts, dates, sources, and relationships.{ctx}"),
            "note": "This step uses your non-factbase tools. When you have enough information, proceed to step 3.",
            "when_done": "Call workflow with workflow='ingest', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "ingest",
            "step": 3, "total_steps": total,
            "instruction": format!("Create or update factbase documents with your findings. Use create_document for new entities, update_document for existing ones.\n\nDocument rules:\n- Place in typed folders: people/, companies/, projects/, etc.\n- First # Heading = document title\n- Every dynamic fact needs a temporal tag:\n  @t[2020..2023] = date range, @t[2024..] = ongoing, @t[=2024-03] = point in time, @t[~2025-02] = last verified, @t[?] = unverified\n- Add source footnotes: [^1] on the fact, [^1]: Source type, date at the bottom\n- Use exact entity names matching other document titles for cross-linking\n- Never modify <!-- factbase:XXXXXX --> headers{fields}"),
            "next_tool": "create_document",
            "when_done": "Call workflow with workflow='ingest', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "ingest",
            "step": 4, "total_steps": total,
            "instruction": "Verify your work. Call generate_questions with dry_run=true on each document you created or modified. Review any questions that come up — they indicate quality issues you can fix now.",
            "next_tool": "generate_questions",
            "suggested_args": {"dry_run": true},
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "ingest",
            "complete": true,
            "instruction": "Workflow complete. Documents have been created/updated."
        }),
    }
}

fn enrich_step(step: usize, args: &Value, perspective: &Option<Perspective>) -> Value {
    let doc_type = get_str_arg(args, "doc_type").unwrap_or("all types");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "enrich",
            "step": 1, "total_steps": total,
            "instruction": format!("List documents to review. Call list_entities to browse documents{}. Then call get_document_stats on each to find which ones need attention (low temporal coverage, missing sources, few links).{ctx}",
                if doc_type != "all types" { format!(" filtered by type '{doc_type}'") } else { String::new() }),
            "next_tool": "list_entities",
            "when_done": "Call workflow with workflow='enrich', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "enrich",
            "step": 2, "total_steps": total,
            "instruction": format!("For each document that needs enrichment, call get_entity to read its full content. Identify gaps:\n- Dynamic facts missing temporal tags\n- Facts without source citations\n- Sparse sections that could be expanded\n- Missing standard fields for the document type{fields}"),
            "next_tool": "get_entity",
            "when_done": "Call workflow with workflow='enrich', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "enrich",
            "step": 3, "total_steps": total,
            "instruction": format!("Research the gaps using your other tools, then call update_document to add findings.\n\nRules:\n- Preserve all existing content — add to it, don't replace\n- Always add temporal tags and source footnotes on new facts\n- Don't add speculative information — only add what you can source\n- Use @t[?] for facts you found but can't date precisely{ctx}"),
            "next_tool": "update_document",
            "when_done": "Call workflow with workflow='enrich', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "enrich",
            "step": 4, "total_steps": total,
            "instruction": "Verify your work. Call generate_questions with dry_run=true on each document you modified to check for new issues.",
            "next_tool": "generate_questions",
            "suggested_args": {"dry_run": true},
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "enrich",
            "complete": true,
            "instruction": "Workflow complete. Documents have been enriched."
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Perspective, ReviewPerspective};
    use std::collections::HashMap;

    fn mock_perspective() -> Option<Perspective> {
        Some(Perspective {
            type_name: String::new(),
            organization: Some("Acme Corp".into()),
            focus: Some("Customer relationship tracking".into()),
            allowed_types: None,
            review: Some(ReviewPerspective {
                stale_days: Some(180),
                required_fields: Some(HashMap::from([(
                    "person".into(),
                    vec!["current_role".into(), "location".into()],
                )])),
                ignore_patterns: None,
            }),
        })
    }

    #[test]
    fn test_perspective_context() {
        let p = mock_perspective();
        let ctx = perspective_context(&p);
        assert!(ctx.contains("Acme Corp"));
        assert!(ctx.contains("Customer relationship tracking"));
    }

    #[test]
    fn test_perspective_context_none() {
        assert_eq!(perspective_context(&None), "");
    }

    #[test]
    fn test_stale_days_from_perspective() {
        assert_eq!(stale_days(&mock_perspective()), 180);
        assert_eq!(stale_days(&None), 365);
    }

    #[test]
    fn test_required_fields_hint() {
        let hint = required_fields_hint(&mock_perspective());
        assert!(hint.contains("person"));
        assert!(hint.contains("current_role"));
    }

    #[test]
    fn test_resolve_includes_perspective() {
        let p = mock_perspective();
        let step = resolve_step(1, &serde_json::json!({}), &p, 0);
        assert!(step["instruction"].as_str().unwrap().contains("Acme Corp"));
        assert_eq!(step["policy"]["stale_days"], 180);
    }

    #[test]
    fn test_resolve_without_perspective() {
        let step = resolve_step(1, &serde_json::json!({}), &None, 0);
        assert!(!step["instruction"]
            .as_str()
            .unwrap()
            .contains("Knowledge base context"));
        assert_eq!(step["policy"]["stale_days"], 365);
    }

    #[test]
    fn test_ingest_includes_required_fields() {
        let p = mock_perspective();
        let step = ingest_step(3, &serde_json::json!({}), &p);
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
    }

    #[test]
    fn test_enrich_includes_required_fields() {
        let p = mock_perspective();
        let step = enrich_step(2, &serde_json::json!({}), &p);
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
    }

    #[test]
    fn test_resolve_stale_days_in_instructions() {
        let p = mock_perspective();
        let step = resolve_step(2, &serde_json::json!({}), &p, 0);
        assert!(step["instruction"].as_str().unwrap().contains("180 days"));
    }

    #[test]
    fn test_past_last_step_returns_complete() {
        let step = resolve_step(99, &serde_json::json!({}), &None, 0);
        assert!(step["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_resolve_step1_includes_deferred_note() {
        let step = resolve_step(1, &serde_json::json!({}), &None, 5);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("5 deferred item(s)"));
        assert_eq!(step["deferred_count"], 5);
    }

    #[test]
    fn test_resolve_step1_no_deferred_note_when_zero() {
        let step = resolve_step(1, &serde_json::json!({}), &None, 0);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(!instruction.contains("deferred"));
        assert_eq!(step["deferred_count"], 0);
    }
}
