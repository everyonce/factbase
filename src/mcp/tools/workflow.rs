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
use crate::llm::LlmProvider;
use crate::models::{Perspective, QuestionType};
use crate::processor::parse_review_queue;
use serde_json::Value;

use crate::config::workflows::{resolve_workflow_text, WorkflowsConfig};
use super::helpers::{build_quality_stats, detect_weak_identification, load_perspective};
use super::review::format_question_json;
use super::{get_str_arg, get_str_arg_required, get_u64_arg};

/// Compact format rules inlined into workflow steps so weaker models don't need a separate get_authoring_guide call.
const FORMAT_RULES: &str = "\n\n**⚠️ FORMAT RULES — read carefully:**\n\n**Temporal tags** — ONLY dates/years go inside @t[...]. NEVER put names, descriptions, statuses, or any other text inside.\n- ✅ CORRECT: `@t[=2024]` `@t[~2024]` `@t[2020..2023]` `@t[2024..]` `@t[?]` `@t[=331 BCE]` `@t[=-0490]`\n- ❌ WRONG (entity names): `@t[Wolfgang Amadeus Mozart]` `@t[Mount Vesuvius]`\n- ❌ WRONG (descriptions): `@t[Complex counterpoint and fugal writing]` `@t[bright red when young]`\n- ❌ WRONG (statuses): `@t[Active Production Status: Ongoing]` `@t[No significant seismic activity]`\n- ❌ WRONG (statistics): `@t[Total Produced: 650+]` `@t[Population: 12000]`\n- ❌ WRONG (vague time words): `@t[seasonal]` `@t[since ancient times]` `@t[traditional..modern]`\n- Syntax: `@t[=YYYY]` exact date, `@t[~YYYY]` approximate, `@t[YYYY..YYYY]` range, `@t[YYYY..]` ongoing, `@t[?]` unknown\n- BCE dates: `@t[=331 BCE]` or `@t[=-331]` or `@t[=-0331]` — all equivalent\n- Place the tag AFTER the fact text: `- Cap color: red to orange @t[~2024] [^1]`\n- If you don't know the date, use `@t[?]` — NEVER put text descriptions inside the brackets\n\n**Source footnotes** on every fact: `[^1]` inline, then `---\\n[^1]: Author, Title, Date` at bottom\n\nCall get_authoring_guide for the full format reference";

// ---------------------------------------------------------------------------
// Default instruction templates for each workflow step.
// Use {placeholder} syntax for dynamic parts; resolved via config overrides.
// ---------------------------------------------------------------------------

// --- Setup workflow ---
pub(crate) const DEFAULT_SETUP_INIT_INSTRUCTION: &str = "Initialize a new factbase repository at '{path}'. Call init_repository with path='{path}'.\n\nAfter initialization, the directory will contain a perspective.yaml file that needs to be configured in the next step.\n\nTip: If you're unsure what document types and folder structure to use for this domain, call workflow='bootstrap' with a domain description first — it will generate tailored suggestions.\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=2)";

pub(crate) const DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION: &str = "Configure the repository's perspective. Write the file `{path}/perspective.yaml` with YAML content like this:\n\n```yaml\nfocus: \"What this knowledge base is about\"\norganization: \"Who maintains it (optional)\"\nallowed_types:\n  - type1\n  - type2\n  - type3\nreview:\n  stale_days: 180\n  required_fields:\n    type1: [field1, field2]\n    type2: [field1, field2]\n```\n\nIf you ran bootstrap first, use the perspective values it suggested. Otherwise choose values appropriate for the domain.\n\n⚠️ This MUST be valid YAML written to `perspective.yaml` (not .md, not .json). The file goes in the repository root, not in .factbase/.\n\nAlso plan the folder structure — each allowed_type becomes a top-level folder. Documents are placed in type folders (e.g., `species/amanita-muscaria.md`).\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=3)";

pub(crate) const DEFAULT_SETUP_VALIDATE_OK_INSTRUCTION: &str = "✅ perspective.yaml parsed successfully:\n  {detail}\n\nIf this looks correct, proceed to the next step.\n\n⚠️ NEXT: Call workflow(workflow='setup', step=4)";

pub(crate) const DEFAULT_SETUP_VALIDATE_ERROR_INSTRUCTION: &str = "❌ {detail}\n\n⚠️ NEXT: Fix perspective.yaml, then call workflow(workflow='setup', step=3) again to re-validate.";

pub(crate) const DEFAULT_SETUP_CREATE_INSTRUCTION: &str = "Create 2-3 example documents using create_document.\n\nIMPORTANT: First call get_authoring_guide to learn the required document format (temporal tags, footnotes, structure).\n\nTips for first documents:\n- Place each in the appropriate type folder (e.g., 'species/amanita-muscaria.md')\n- Start with a clear # Title\n- Use exact entity names that match other document titles for automatic cross-linking\n- A definitions/ document for domain terminology is a good first document{format_rules}\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=5)";

pub(crate) const DEFAULT_SETUP_SCAN_INSTRUCTION: &str = "Index and verify the new repository. First call scan_repository to generate embeddings and detect links. If the response includes `continue: true`, call it again until complete. Then call check_repository to see initial quality. If the response includes `continue: true`, call it again with the `checked_pair_ids` array from the response to resume where it left off.\n\nReport what the scan found: how many documents were indexed, how many links were detected, and any quality issues from the check.\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=6)";

pub(crate) const DEFAULT_SETUP_COMPLETE_INSTRUCTION: &str = "The repository is set up! Summarize what was created and suggest next steps:\n\n- **Add more content**: Use workflow='ingest' with a topic to research and add documents\n- **Fill gaps**: Use workflow='enrich' to find and fill missing information\n- **Quality check**: Use workflow='update' periodically to scan, check quality, and detect reorganization opportunities\n- **Fix issues**: Use workflow='resolve' to address any review questions\n- **Improve a document**: Use workflow='improve' with a doc_id to improve a specific document end-to-end\n\nThe knowledge base is ready for use. Any markdown editor can modify files directly — just run scan_repository afterward to re-index.";

// --- Update workflow ---
pub(crate) const DEFAULT_UPDATE_SCAN_INSTRUCTION: &str = "Re-index the factbase to pick up file changes and detect cross-entity links.\n\n1. Call scan_repository. If the response includes `continue: true`, call it again until complete.\n2. Record: documents_total, links_detected, temporal_coverage_pct, source_coverage_pct\n3. Save links_detected as LINKS_BEFORE — you'll compare after entity creation\n\nHow links work: scan_repository finds entity title mentions in document text. Each doc should link to at least 1 other. Low link density means entities are isolated — they discuss related topics but don't reference each other by name.{ctx}";

pub(crate) const DEFAULT_UPDATE_CHECK_INSTRUCTION: &str = "Run a deep quality check to find issues across documents and discover missing entities.\n\n1. Call check_repository with deep_check=true. If the response includes `continue: true`, call it again with the `checked_pair_ids` array from the response to resume where it left off.\n2. Record: questions_total, breakdown by type (stale, conflict, temporal, missing)\n   - Mostly stale → KB is aging, needs fresh sources\n   - Mostly temporal → facts lack dates, timeline is murky\n   - Mostly conflict → documents disagree, contradictions to resolve\n   - Mostly missing → claims lack evidence\n3. Look at suggested_entities — these are important actors that appear across documents but don't have their own page yet\n\nIF suggested_entities count > 0:\n  4. For EACH entity with high or medium confidence:\n     - Call create_document with the suggested name, type, and a skeleton body\n     - If the entity is external to your domain (e.g. a well-known product, standard, or organization you reference but don't track in depth), add `<!-- factbase:reference -->` after the factbase ID header. Reference entities are available for linking and search but skipped by quality checks.\n     - Body: \"# {name}\\n\\nType: {type}\\n\\nReferenced in: {doc IDs that mention it}\"\n  5. After creating ALL entities, call scan_repository again. If the response includes `continue: true`, call it again until complete. Then re-run check_repository (without checked_pair_ids, since new entities changed the graph).\n     - New entities = new titles for the link detector to find\n  6. Record links_detected as LINKS_AFTER\n  7. Calculate: LINKS_GAINED = LINKS_AFTER - LINKS_BEFORE\nELSE:\n  4. LINKS_AFTER = LINKS_BEFORE, LINKS_GAINED = 0";

pub(crate) const DEFAULT_UPDATE_ORGANIZE_INSTRUCTION: &str = "Analyze the knowledge base structure for improvement opportunities.\n\n1. Call organize_analyze\n2. Record candidates:\n   - Merge: documents that overlap significantly — telling the same story twice\n   - Split: documents covering multiple distinct topics\n   - Misplaced: documents whose type doesn't match their content\n   - Duplicates: repeated facts across documents\n3. Do NOT execute changes — just record what you find";

pub(crate) const DEFAULT_UPDATE_SUMMARY_INSTRUCTION: &str = "Write a diagnostic report combining metrics and assessment.\n\n## Scan & Links\n- Documents: X | Links before: X | Links after: X | Gained: +X\n- Temporal coverage: X% | Source coverage: X%\n- Link health: [healthy / needs work / poor] — each doc should average 1+ link\n\n## Quality Issues\n- Total questions: X (stale: X, conflict: X, temporal: X, missing: X)\n- Dominant issue type tells you the KB's biggest weakness\n\n## Entities Created\n- List each: name, type, why it matters to the KB's connectivity\n\n## Organization\n- Merge/split/misplaced/duplicate candidates found\n\n## Health Assessment\nOne paragraph: overall KB health, biggest strength, biggest gap, and top 3 priorities ordered by impact.";

// --- Resolve workflow ---
pub(crate) const DEFAULT_RESOLVE_QUEUE_INSTRUCTION: &str = "Get the review queue to see what needs fixing. Call get_review_queue with include_context=true. If there are many questions, filter by type (stale, conflict, missing, temporal, ambiguous, duplicate) to work in batches.{ctx}{deferred_note}";

pub(crate) const DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION: &str = "You are resolving review questions in batches. The system feeds you 15 questions at a time. You will receive multiple batches. Answer each batch and call step 2 again. The system will tell you when all questions are resolved.\n\nANSWER FORMAT BY TYPE:\n\nTEMPORAL — fact line has no @t[...] tag.\n→ MUST include the tag: \"@t[YYYY] per [source] ([URL]); verified YYYY-MM-DD\"\n→ Ranges: @t[YYYY..YYYY], ongoing: @t[YYYY..], BCE: @t[=-480] or @t[=480 BCE]\n→ Unknown date: @t[?] (only when truly unfindable)\n→ WRONG: \"well-known\", \"static\", \"doesn't change\" — rejected, no audit trail\n\nMISSING — fact has no source citation.\n→ \"Source: [name] ([URL]), [date]\"\n\nSTALE — source older than {stale} days.\n→ Search \"{entity} {fact} {current year}\"\n→ Still true: \"Still accurate per [source] ([URL]), verified [date]\"\n→ Changed: \"Updated: [new info] per [source] ([URL])\"\n\nCONFLICT — two facts disagree. Read the [pattern:...] tag.\n→ Both valid (parallel, different entities): \"Not a conflict: [reason]\"\n→ One supersedes: \"Transition: adjust end date to [date] per [source]\"\n→ One wrong: \"[correct fact] per [source], remove [wrong fact]\"\n→ Cross-doc: call get_entity on referenced doc for context\n\nAMBIGUOUS → clarify or create definitions/ file\nDUPLICATE → \"Duplicate of [doc_id], remove from here\"\n\nCan't resolve? → \"defer: searched [what], found [nothing/insufficient]\"\nAlways include your source.{ctx}";

pub(crate) const DEFAULT_RESOLVE_ANSWER_INSTRUCTION: &str = "Answer the questions in this batch. Call answer_questions with doc_id, question_index, and your answer for each.\n\nResearch when needed — web search for stale/temporal, get_entity for cross-doc conflicts.\n\nAfter answering, call workflow with workflow='resolve', step=2 for the next batch. Do NOT skip ahead to step 3 — the system will tell you when all questions are resolved.{ctx}";

pub(crate) const DEFAULT_RESOLVE_APPLY_INSTRUCTION: &str = "Apply your answered questions to the actual document content. Call apply_review_answers to rewrite documents based on your answers. If the response includes `continue: true`, call it again until complete. Use dry_run=true first to preview, then without dry_run to apply.";

pub(crate) const DEFAULT_RESOLVE_VERIFY_INSTRUCTION: &str = "Verify your work. For each document you modified, call generate_questions with dry_run=true to check if your answers introduced new issues. If new questions appear, resolve them now.";

// --- Ingest workflow ---
pub(crate) const DEFAULT_INGEST_SEARCH_INSTRUCTION: &str = "Search factbase to see what already exists about '{topic}'. Call search_knowledge with a relevant query. Also try list_entities to browse by type.{ctx}";

pub(crate) const DEFAULT_INGEST_RESEARCH_INSTRUCTION: &str = "Research '{topic}' using your available tools. Strategies:\n- **Web search**: Search for recent, authoritative information. Try specific queries like '{entity name} {fact type} {year}' rather than broad searches.\n- **Multiple sources**: Cross-reference findings across at least 2 sources before adding facts.\n- **Gather specifics**: Collect dates, numbers, names, and citations — not just summaries.\n- **Note your sources**: Track the URL, author, publication, and date for every fact you find — you'll need these for footnotes.\n\nOrganize what you find by entity and section before proceeding to document creation.{ctx}";

pub(crate) const DEFAULT_INGEST_CREATE_INSTRUCTION: &str = "Create or update factbase documents with your findings. Use create_document for new entities, update_document for existing ones.\n\nDocument rules:\n- Place in typed folders: people/, companies/, projects/, definitions/, etc.\n- First # Heading = document title\n- Use exact entity names matching other document titles for cross-linking\n- For acronyms or domain terms, create/update a definitions/ file\n- Never use 'Author knowledge' as a source — that's reserved for human-authored author-knowledge/ files\n- Never modify <!-- factbase:XXXXXX --> headers\n- If existing files are in the wrong folder or poorly named, feel free to rename/move them — just run scan_repository afterward\n- Entity discovery: while researching, if you discover an entity that fits the KB's allowed types (check the perspective) and is mentioned across multiple existing documents or is significant enough to warrant its own entry, create a new document for it using create_document\n- For entities external to your domain (well-known products, standards, organizations you reference but don't track in depth), add `<!-- factbase:reference -->` after the factbase ID header. These are available for linking but won't be quality-checked.{fields}{format_rules}";

pub(crate) const DEFAULT_INGEST_VERIFY_INSTRUCTION: &str = "Verify your work. Call generate_questions with dry_run=true on each document you created or modified. Review any questions that come up — they indicate quality issues you can fix now.\n\nAlso note any frequently-mentioned names that don't have their own documents — these are candidates for new entities.";

// --- Enrich workflow ---
pub(crate) const DEFAULT_ENRICH_REVIEW_INSTRUCTION: &str = "Review the entity_quality list (sorted by attention_score, highest first). Reference entities are excluded — they exist for linking, not enrichment.\nPick the top 3-5 entities that need work. You will enrich them ONE AT A TIME — fully completing each before moving to the next.\n\nCall get_entity on the first entity to begin.{ctx}";

pub(crate) const DEFAULT_ENRICH_GAPS_INSTRUCTION: &str = "Score this document before researching:\n\n1. Temporal: X of Y fact lines have @t tags = Z%\n2. Sources: X of Y fact lines have [^N] citations = Z%\n3. Missing fields: [list any required by perspective]{fields}\n4. Review questions: X unanswered\n5. Gaps: list ALL areas where you could add substantive facts — sections that are thin, topics mentioned but not developed, missing context or history\n\nResearch the LOWEST coverage area first, then continue through all gaps.";

pub(crate) const DEFAULT_ENRICH_RESEARCH_INSTRUCTION: &str = "Research and update THIS document. Work through ALL gaps you identified — don't stop at 3-5.\n\nFor each gap:\n1. Search specifically: \"{entity name} {fact}\" — targeted beats broad\n2. Read the full page, not just snippets\n3. Every new fact MUST have BOTH @t[YYYY] AND [^N] citation — no exceptions\n4. Mention related KB entities by exact title in prose (enables link detection)\n\nKeep going until you've exhausted your research for this document. More well-sourced facts = better.\n\nCall update_document with the enriched content. Then verify: call generate_questions with dry_run=true.\n\nRecord: facts added, sources added, @t tags added, issues from verify.\n\n⚠️ REPEAT for the next document:\n1. Call get_entity on the next entity from your list\n2. Score it (same as step 2)\n3. Research + update + verify (same as this step)\nContinue until all documents are done.\n\nRules:\n- Preserve ALL existing content — add, don't replace\n- Resolve review questions when your research provides answers\n- Create documents for significant missing entities{ctx}{format_rules}";

pub(crate) const DEFAULT_ENRICH_VERIFY_INSTRUCTION: &str = "Report totals across all documents enriched:\n\n| Document | +Facts | +Sources | +@t | Issues |\n|----------|--------|----------|-----|--------|\n| ... | ... | ... | ... | ... |\n\nTotals: X facts, X sources, X @t tags added. X questions resolved. X new issues.\nAssessment: biggest improvement, remaining gaps, recommended next step.";

// --- Improve workflow ---
pub(crate) const DEFAULT_IMPROVE_CLEANUP_INSTRUCTION: &str = "Read the document and fix any issues{doc_hint}. Call get_entity with the doc_id to read its full content.\n\nCheck for and fix:\n- Corruption artifacts (malformed review queue sections, broken markdown)\n- Duplicate entries (same fact stated multiple times)\n- Formatting inconsistencies (inconsistent heading levels, missing blank lines)\n- Orphaned footnote references or definitions\n\nIf issues are found, call update_document to fix them. If the document looks clean, move to the next step.{ctx}";

pub(crate) const DEFAULT_IMPROVE_RESOLVE_INSTRUCTION: &str = "Resolve outstanding review questions{doc_hint}. Call get_review_queue with doc_id to see pending questions.\n\nFor each unanswered question:\n- stale: Source is older than {stale} days. Search for current info\n- missing: Find a source citation\n- conflict: Check the [pattern:...] tag — parallel_overlap, same_entity_transition, date_imprecision are often not real conflicts\n- temporal: The fact line is MISSING an @t[...] temporal tag — that is what this question means. Your answer MUST include the @t[...] tag to add, plus a source. Answer: '@t[YYYY] per [source] ([URL]); verified [YYYY-MM-DD]' or '@t[YYYY..YYYY] per [source]' for ranges. Just citing a source in prose does NOT resolve it — the @t[...] tag must appear in your answer. Use @t[?] only when no date is findable. Every datable fact gets a tag regardless of domain or era. Never answer with bare dismissals like 'static fact', 'well-known', or 'historical constant' — these provide no audit trail\n- ambiguous: Clarify the term or create a definitions document\n- duplicate: Identify the canonical entry\n\nCall answer_questions with your answers. If you can't resolve a question, defer it with a note about what you tried.\n\nAfter answering, call apply_review_answers with doc_id to apply changes to the document. If the response includes `continue: true`, call it again until complete.{ctx}";

pub(crate) const DEFAULT_IMPROVE_ENRICH_INSTRUCTION: &str = "Enrich the document with new information{doc_hint}. Call get_entity to read the current content (it may have changed from earlier steps).\n\nIdentify gaps:\n- Dynamic facts missing temporal tags\n- Facts without source citations\n- Sparse sections that could be expanded\n- Missing standard fields for the document type\n- Weak identification: if the title is an alias, abbreviation, or partial label and a fuller canonical name exists, update the title\n- Poor file organization: if the file is in the wrong folder or has an unclear name, rename/move it with file tools{fields}\n\nResearch the gaps using your available tools:\n- Web search for current data on each gap — use specific queries per entity/fact\n- Read full pages when snippets look relevant\n- Cross-reference important facts across multiple sources\n\nThen call update_document to add findings.\n\nRules:\n- Preserve all existing content — add to it, don't replace\n- Always add temporal tags and source footnotes on new facts\n- Don't add speculative information — only add what you can source\n- Use @t[?] for facts you found but can't date precisely\n- If you rename or move any files, run scan_repository afterward to re-index{ctx}";

pub(crate) const DEFAULT_IMPROVE_CHECK_INSTRUCTION: &str = "Verify the document quality{doc_hint}. Call generate_questions with doc_id and dry_run=true to check for any remaining or newly introduced issues.\n\nReport what you find:\n- How many questions remain vs. how many were resolved\n- Any new issues introduced during enrichment\n- Overall document health assessment{compare_note}";

/// Start a guided workflow.
pub fn workflow(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let workflow = get_str_arg_required(args, "workflow")?;
    let step = get_u64_arg(args, "step", 1) as usize;
    let perspective = load_perspective(db, get_str_arg(args, "repo"));
    let wf_config = crate::Config::load(None)
        .unwrap_or_default()
        .workflows;

    match workflow.as_str() {
        "update" => Ok(update_step(step, args, &perspective, &wf_config)),
        "resolve" => {
            let deferred = db
                .count_deferred_questions(get_str_arg(args, "repo"))
                .unwrap_or(0);
            Ok(resolve_step(step, args, &perspective, deferred, db, &wf_config))
        }
        "ingest" => Ok(ingest_step(step, args, &perspective, &wf_config)),
        "enrich" => Ok(enrich_step(step, args, &perspective, db, &wf_config)),
        "improve" => {
            let doc_id = get_str_arg(args, "doc_id");
            let skip = parse_skip_steps(args);
            Ok(improve_step(step, doc_id, &perspective, &skip, db, &wf_config))
        }
        "setup" => Ok(setup_step(step, args, &wf_config)),
        "bootstrap" => Ok(serde_json::json!({
            "error": "The bootstrap workflow requires an LLM provider. Make sure your factbase instance has an LLM configured.",
            "hint": "Bootstrap is handled as an async workflow. If you're seeing this, the routing in mod.rs didn't intercept it."
        })),
        "list" => Ok(serde_json::json!({
            "workflows": [
                {"name": "bootstrap", "description": "Design a domain-specific knowledge base structure using LLM. Provide domain='mycology' (or any domain) and get suggested document types, folder structure, templates, temporal patterns, and example documents. Use this BEFORE setup when starting a new KB in an unfamiliar domain."},
                {"name": "setup", "description": "Set up a new factbase repository from scratch: initialize, configure perspective, create first documents, scan, and verify"},
                {"name": "update", "description": "Scan, check quality, analyze organization (merge/split/misplaced/duplicates), and report what needs attention"},
                {"name": "resolve", "description": "Fix quality issues by resolving review queue questions using external sources"},
                {"name": "ingest", "description": "Research a topic and create/update factbase documents"},
                {"name": "enrich", "description": "Find and fill gaps in existing documents"},
                {"name": "improve", "description": "Improve a single document end-to-end: cleanup, resolve questions, enrich, then quality check. Requires doc_id."}
            ]
        })),
        _ => Ok(serde_json::json!({
            "error": format!("Unknown workflow '{}'. Call workflow with workflow='list' to see available workflows.", workflow)
        })),
    }
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

/// Resolve a workflow instruction with config override support.
fn resolve(wf: &WorkflowsConfig, key: &str, default: &str, vars: &[(&str, &str)]) -> String {
    resolve_workflow_text(wf, key, default, vars)
}

fn setup_step(step: usize, args: &Value, wf: &WorkflowsConfig) -> Value {
    let path = get_str_arg(args, "path").unwrap_or("the target directory");
    let total = 6;
    match step {
        1 => serde_json::json!({
            "workflow": "setup",
            "step": 1, "total_steps": total,
            "title": "Step 1 of 6: Initialize Repository",
            "instruction": resolve(wf, "setup.init", DEFAULT_SETUP_INIT_INSTRUCTION, &[("path", path)]),
            "next_tool": "init_repository",
            "suggested_args": {"path": path},
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=2) to continue to Step 2 of 6"
        }),
        2 => serde_json::json!({
            "workflow": "setup",
            "step": 2, "total_steps": total,
            "title": "Step 2 of 6: Configure Perspective",
            "instruction": resolve(wf, "setup.perspective", DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION, &[("path", path)]),
            "note": "Write perspective.yaml as YAML to the repository root directory. Do NOT create perspective.md — factbase only reads perspective.yaml.",
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=3) to continue to Step 3 of 6"
        }),
        3 => {
            // Validate perspective.yaml was written correctly
            let perspective = crate::models::load_perspective_from_file(std::path::Path::new(path));
            let (status, detail) = match &perspective {
                Some(p) => {
                    let mut fields = Vec::new();
                    if let Some(f) = &p.focus { fields.push(format!("focus: {f}")); }
                    if let Some(o) = &p.organization { fields.push(format!("organization: {o}")); }
                    if let Some(t) = &p.allowed_types { fields.push(format!("allowed_types: {}", t.join(", "))); }
                    if p.review.is_some() { fields.push("review: configured".into()); }
                    ("ok".to_string(), fields.join("\n  "))
                }
                None => ("error".to_string(), "perspective.yaml is missing, empty, or has invalid YAML. Go back to step 2 and fix it.".into()),
            };
            let instruction = if status == "ok" {
                resolve(wf, "setup.validate_ok", DEFAULT_SETUP_VALIDATE_OK_INSTRUCTION, &[("detail", &detail)])
            } else {
                resolve(wf, "setup.validate_error", DEFAULT_SETUP_VALIDATE_ERROR_INSTRUCTION, &[("detail", &detail)])
            };
            serde_json::json!({
                "workflow": "setup",
                "step": 3, "total_steps": total,
                "title": "Step 3 of 6: Validate Perspective",
                "perspective_status": status,
                "perspective_parsed": detail,
                "instruction": instruction,
                "when_done": if status == "ok" {
                    "⚠️ REQUIRED: Call workflow(workflow='setup', step=4) to continue to Step 4 of 6"
                } else {
                    "⚠️ REQUIRED: Fix perspective.yaml, then call workflow(workflow='setup', step=3) again"
                }
            })
        },
        4 => serde_json::json!({
            "workflow": "setup",
            "step": 4, "total_steps": total,
            "title": "Step 4 of 6: Create Documents",
            "instruction": resolve(wf, "setup.create", DEFAULT_SETUP_CREATE_INSTRUCTION, &[("format_rules", FORMAT_RULES)]),
            "next_tool": "get_authoring_guide",
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=5) to continue to Step 5 of 6"
        }),
        5 => serde_json::json!({
            "workflow": "setup",
            "step": 5, "total_steps": total,
            "title": "Step 5 of 6: Scan & Verify",
            "instruction": resolve(wf, "setup.scan", DEFAULT_SETUP_SCAN_INSTRUCTION, &[]),
            "next_tool": "scan_repository",
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=6) to continue to Step 6 of 6"
        }),
        6 => serde_json::json!({
            "workflow": "setup",
            "step": 6, "total_steps": total,
            "title": "Step 6 of 6: Complete",
            "instruction": resolve(wf, "setup.complete", DEFAULT_SETUP_COMPLETE_INSTRUCTION, &[]),
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "setup",
            "complete": true,
            "instruction": "Workflow complete."
        }),
    }
}

/// Default template for the bootstrap prompt.
const DEFAULT_BOOTSTRAP_PROMPT: &str = r##"Design a knowledge base structure for: "{domain}"{entity_types}

Return a JSON object with exactly these 4 fields:

1. "document_types": array of {"name": "lowercase-hyphenated", "description": "one line"}
   — 3-5 types that reflect how practitioners organize knowledge in this domain.

2. "folder_structure": array of folder paths (e.g. ["airlines/", "airports/", "definitions/"])

3. "templates": object mapping each type name to a markdown template. Each template:
   - Starts with # placeholder title
   - Has 2-3 section headings suited to the type
   - Shows example bullets with @t[YYYY] or @t[YYYY..YYYY] temporal tags on time-sensitive facts
   - CRITICAL: @t[...] tags contain ONLY dates — NEVER names, descriptions, or non-date content
     ✅ @t[=2024], @t[~2024-03], @t[2020..2023], @t[2024..], @t[?], @t[=331 BCE]
     ❌ @t[Wolfgang Amadeus Mozart], @t[Complex counterpoint], @t[Active Production Status: Ongoing]
   - Includes [^1] footnote references and a --- section with source definitions
   - Is realistic for the domain

4. "perspective": {"focus": "one-line mission", "allowed_types": ["your type names"]}

Return ONLY valid JSON, no markdown fences or explanation."##;

/// Build the LLM prompt for domain-aware KB structure generation.
fn build_bootstrap_prompt(
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

/// Parse the LLM response, extracting JSON from the text.
fn parse_bootstrap_response(response: &str) -> Option<Value> {
    serde_json::from_str(response).ok().or_else(|| {
        response.find('{').and_then(|start| {
            response
                .rfind('}')
                .and_then(|end| serde_json::from_str(&response[start..=end]).ok())
        })
    })
}

/// Run the bootstrap workflow: use LLM to generate domain-specific KB structure.
pub async fn bootstrap(
    llm: &dyn LlmProvider,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let domain = get_str_arg_required(args, "domain")?;
    let entity_types = get_str_arg(args, "entity_types");
    let _path = get_str_arg(args, "path");

    let prompts = crate::Config::load(None)
        .unwrap_or_default()
        .prompts;
    let prompt = build_bootstrap_prompt(&domain, entity_types, &prompts);
    let response = llm.complete(&prompt).await?;

    let suggestions = parse_bootstrap_response(&response).unwrap_or_else(|| {
        serde_json::json!({
            "error": "Could not parse LLM response as JSON. Raw response included.",
            "raw_response": response
        })
    });

    let has_error = suggestions.get("error").is_some();

    Ok(serde_json::json!({
        "workflow": "bootstrap",
        "domain": domain,
        "suggestions": suggestions,
        "next_steps": if has_error {
            serde_json::json!([
                "The LLM response could not be parsed. Try calling workflow='bootstrap' again with a more specific domain description.",
            ])
        } else {
            serde_json::json!([
                "Use the suggestions above as reference when configuring perspective.yaml (YAML format, not markdown) and creating documents.",
                "⚠️ REQUIRED NEXT: Call workflow(workflow='setup', step=1) to begin the guided setup process. The setup workflow will walk you through each step (init → configure → create docs → scan → verify).",
                "Do NOT skip the setup workflow — it provides step-by-step guidance including format rules for temporal tags and source footnotes."
            ])
        },
        "note": "These are suggestions — adapt them to your needs. The templates and folder structure can be modified at any time.",
        "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=1) to begin guided setup"
    }))
}

fn update_step(step: usize, _args: &Value, perspective: &Option<Perspective>, wf: &WorkflowsConfig) -> Value {
    let ctx = perspective_context(perspective);
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "update",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "update.scan", DEFAULT_UPDATE_SCAN_INSTRUCTION, &[("ctx", &ctx)]),
            "next_tool": "scan_repository",
            "when_done": "Call workflow with workflow='update', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "update",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "update.check", DEFAULT_UPDATE_CHECK_INSTRUCTION, &[]),
            "next_tool": "check_repository",
            "suggested_args": {"dry_run": false},
            "when_done": "Call workflow with workflow='update', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "update",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "update.organize", DEFAULT_UPDATE_ORGANIZE_INSTRUCTION, &[]),
            "next_tool": "organize_analyze",
            "when_done": "Call workflow with workflow='update', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "update",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "update.summary", DEFAULT_UPDATE_SUMMARY_INSTRUCTION, &[]),
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "update",
            "complete": true,
            "instruction": "Workflow complete."
        }),
    }
}

/// Batch size for resolve step 2 question batching.
const RESOLVE_BATCH_SIZE: usize = 15;

/// Priority ordering for question types within a batch.
/// Lower number = higher priority (processed first).
fn question_type_priority(qt: &QuestionType) -> u8 {
    match qt {
        QuestionType::Temporal => 0,
        QuestionType::Missing => 1,
        QuestionType::Stale => 2,
        QuestionType::Conflict => 3,
        QuestionType::Ambiguous => 4,
        QuestionType::Duplicate => 5,
        QuestionType::Corruption => 6,
    }
}

fn resolve_step(
    step: usize,
    _args: &Value,
    perspective: &Option<Perspective>,
    deferred: usize,
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
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
            "instruction": resolve(wf, "resolve.queue", DEFAULT_RESOLVE_QUEUE_INSTRUCTION, &[("ctx", &ctx), ("deferred_note", &deferred_note)]),
            "next_tool": "get_review_queue",
            "suggested_args": {"include_context": true},
            "policy": {"stale_days": stale},
            "deferred_count": deferred,
            "when_done": "Call workflow with workflow='resolve', step=2"
        }),
        2 => resolve_step2_batch(perspective, db, wf),
        3 => serde_json::json!({
            "workflow": "resolve",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "resolve.apply", DEFAULT_RESOLVE_APPLY_INSTRUCTION, &[]),
            "next_tool": "apply_review_answers",
            "suggested_args": {"dry_run": false},
            "when_done": "Call workflow with workflow='resolve', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "resolve",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "resolve.verify", DEFAULT_RESOLVE_VERIFY_INSTRUCTION, &[]),
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

/// Build the step 2 response with inline question batching.
///
/// Reads the actual review queue from the DB, collects unanswered questions,
/// sorts them (grouped by document, then by type priority), and returns the
/// next batch. The agent just answers what it sees and calls step 2 again.
fn resolve_step2_batch(
    perspective: &Option<Perspective>,
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
    let ctx = perspective_context(perspective);
    let stale = stale_days(perspective);
    let total_steps = 4;

    // Collect all questions from the review queue
    let docs = db.get_documents_with_review_queue(None).unwrap_or_else(|_| Vec::new());
    let mut unanswered: Vec<Value> = Vec::new();
    let mut resolved_so_far: usize = 0;

    for doc in &docs {
        if let Some(questions) = parse_review_queue(&doc.content) {
            for (idx, q) in questions.iter().enumerate() {
                if q.answered || q.is_deferred() {
                    resolved_so_far += 1;
                } else {
                    let mut qjson = format_question_json(q, Some((&doc.id, &doc.title)));
                    if let Some(obj) = qjson.as_object_mut() {
                        obj.insert("question_index".to_string(), serde_json::json!(idx));
                        // Stash sort keys (removed before sending)
                        obj.insert("_doc_id".to_string(), Value::String(doc.id.clone()));
                        obj.insert(
                            "_type_priority".to_string(),
                            serde_json::json!(question_type_priority(&q.question_type)),
                        );
                    }
                    unanswered.push(qjson);
                }
            }
        }
    }

    // Sort: group by document, then by type priority within each doc
    unanswered.sort_by(|a, b| {
        let doc_a = a["_doc_id"].as_str().unwrap_or("");
        let doc_b = b["_doc_id"].as_str().unwrap_or("");
        doc_a.cmp(doc_b).then_with(|| {
            let pa = a["_type_priority"].as_u64().unwrap_or(99);
            let pb = b["_type_priority"].as_u64().unwrap_or(99);
            pa.cmp(&pb)
        })
    });

    // Remove sort keys before sending
    for q in &mut unanswered {
        if let Some(obj) = q.as_object_mut() {
            obj.remove("_doc_id");
            obj.remove("_type_priority");
        }
    }

    let remaining = unanswered.len();

    // If no unanswered questions remain, advance to step 3
    if remaining == 0 {
        return serde_json::json!({
            "workflow": "resolve",
            "step": 2, "total_steps": total_steps,
            "instruction": "All review questions have been resolved. Proceeding to apply answers.",
            "batch": {
                "questions": [],
                "batch_number": 0,
                "total_batches_estimate": 0,
                "resolved_so_far": resolved_so_far,
                "remaining": 0
            },
            "when_done": "Call workflow with workflow='resolve', step=3"
        });
    }

    let batch_size = RESOLVE_BATCH_SIZE;
    let total_questions = resolved_so_far + remaining;
    let batch_number = (resolved_so_far / batch_size) + 1;
    let total_batches_estimate = (total_questions + batch_size - 1) / batch_size;
    let batch: Vec<Value> = unanswered.into_iter().take(batch_size).collect();

    let instruction = resolve(
        wf,
        "resolve.answer",
        DEFAULT_RESOLVE_ANSWER_INSTRUCTION,
        &[("stale", &stale.to_string()), ("ctx", &ctx)],
    );

    let is_first_batch = resolved_so_far == 0;

    let mut result = serde_json::json!({
        "workflow": "resolve",
        "step": 2, "total_steps": total_steps,
        "instruction": instruction,
        "next_tool": "answer_questions",
        "conflict_patterns": {
            "parallel_overlap": "Two overlapping facts about different entities that may legitimately coexist. Answer: 'Not a conflict: parallel overlap'.",
            "same_entity_transition": "Two overlapping facts about the same entity where one likely supersedes the other. Adjust the earlier entry's end date.",
            "date_imprecision": "Small overlap relative to date ranges — likely data-source imprecision. Adjust the boundary date.",
            "unknown": "No recognized pattern — investigate which fact is current."
        },
        "batch": {
            "questions": batch,
            "batch_number": batch_number,
            "total_batches_estimate": total_batches_estimate,
            "resolved_so_far": resolved_so_far,
            "remaining": remaining
        },
        "progress": format!("Batch {batch_number}: {resolved_so_far} answered, {remaining} remaining"),
        "completion_gate": format!("⚠️ {remaining} questions remain. Do NOT proceed to step 3 until remaining is 0. Call workflow with workflow='resolve', step=2 for the next batch."),
        "when_done": "Call workflow with workflow='resolve', step=2"
    });

    if is_first_batch {
        let intro = resolve(
            wf,
            "resolve.answer_intro",
            DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION,
            &[("stale", &stale.to_string()), ("ctx", &ctx)],
        );
        result
            .as_object_mut()
            .unwrap()
            .insert("intro".to_string(), Value::String(intro));
    }

    result
}

fn ingest_step(step: usize, args: &Value, perspective: &Option<Perspective>, wf: &WorkflowsConfig) -> Value {
    let topic = get_str_arg(args, "topic").unwrap_or("the requested topic");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "ingest",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "ingest.search", DEFAULT_INGEST_SEARCH_INSTRUCTION, &[("topic", topic), ("ctx", &ctx)]),
            "next_tool": "search_knowledge",
            "when_done": "Call workflow with workflow='ingest', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "ingest",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "ingest.research", DEFAULT_INGEST_RESEARCH_INSTRUCTION, &[("topic", topic), ("ctx", &ctx)]),
            "note": "This step uses your non-factbase tools. When you have enough information, proceed to step 3.",
            "when_done": "Call workflow with workflow='ingest', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "ingest",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "ingest.create", DEFAULT_INGEST_CREATE_INSTRUCTION, &[("fields", &fields), ("format_rules", FORMAT_RULES)]),
            "next_tool": "create_document",
            "when_done": "Call workflow with workflow='ingest', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "ingest",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "ingest.verify", DEFAULT_INGEST_VERIFY_INSTRUCTION, &[]),
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

/// Build quality stats JSON for a single entity by doc_id.
fn entity_quality(db: &Database, doc_id: &str) -> Option<Value> {
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
fn bulk_quality(db: &Database, doc_type: Option<&str>, repo: Option<&str>) -> Value {
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

fn enrich_step(step: usize, args: &Value, perspective: &Option<Perspective>, db: &Database, wf: &WorkflowsConfig) -> Value {
    let doc_type = get_str_arg(args, "doc_type").unwrap_or("all types");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 4;
    match step {
        1 => {
            let type_filter = if doc_type != "all types" { Some(doc_type) } else { None };
            let repo_filter = get_str_arg(args, "repo");
            let quality = bulk_quality(db, type_filter, repo_filter);
            serde_json::json!({
                "workflow": "enrich",
                "step": 1, "total_steps": total,
                "instruction": resolve(wf, "enrich.review", DEFAULT_ENRICH_REVIEW_INSTRUCTION, &[("ctx", &ctx)]),
                "entity_quality": quality,
                "next_tool": "get_entity",
                "when_done": "Call workflow with workflow='enrich', step=2"
            })
        },
        2 => serde_json::json!({
            "workflow": "enrich",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "enrich.gaps", DEFAULT_ENRICH_GAPS_INSTRUCTION, &[("fields", &fields)]),
            "next_tool": "get_entity",
            "when_done": "Call workflow with workflow='enrich', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "enrich",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "enrich.research", DEFAULT_ENRICH_RESEARCH_INSTRUCTION, &[("ctx", &ctx), ("format_rules", FORMAT_RULES)]),
            "next_tool": "update_document",
            "when_done": "Call workflow with workflow='enrich', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "enrich",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "enrich.verify", DEFAULT_ENRICH_VERIFY_INSTRUCTION, &[]),
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

/// Parse the `skip` parameter into a list of step names to skip.
fn parse_skip_steps(args: &Value) -> Vec<String> {
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
const IMPROVE_STEPS: &[&str] = &["cleanup", "resolve", "enrich", "check"];

/// Compute the effective step sequence, skipping any steps in `skip`.
fn effective_steps(skip: &[String]) -> Vec<(usize, &'static str)> {
    IMPROVE_STEPS
        .iter()
        .enumerate()
        .filter(|(_, name)| !skip.contains(&name.to_string()))
        .map(|(i, name)| (i + 1, *name))
        .collect()
}

fn improve_step(
    step: usize,
    doc_id: Option<&str>,
    perspective: &Option<Perspective>,
    skip: &[String],
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
    let steps = effective_steps(skip);
    let total = steps.len();

    if total == 0 {
        return serde_json::json!({
            "workflow": "improve",
            "error": "All steps were skipped. Nothing to do."
        });
    }

    // Map user-facing step number to the logical step name
    let Some(&(_, step_name)) = steps.get(step - 1) else {
        return serde_json::json!({
            "workflow": "improve",
            "complete": true,
            "instruction": "Workflow complete. Document improvement finished."
        });
    };

    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let stale = stale_days(perspective);
    let doc_hint = doc_id.map(|id| format!(" for document '{id}'")).unwrap_or_default();
    let doc_arg = doc_id.map(|id| serde_json::json!(id));
    let skipped: Vec<&str> = skip.iter().map(|s| s.as_str()).collect();
    let next_step_hint = if step < total {
        format!("Call workflow with workflow='improve', step={}{}", step + 1,
            doc_id.map(|id| format!(", doc_id='{id}'")).unwrap_or_default())
    } else {
        String::new()
    };

    // Include quality stats on step 1 so the agent has immediate context
    let quality = if step == 1 {
        doc_id.and_then(|id| entity_quality(db, id))
    } else {
        None
    };

    let mut result = match step_name {
        "cleanup" => serde_json::json!({
            "workflow": "improve",
            "step": step, "total_steps": total,
            "step_name": "cleanup",
            "doc_id": doc_arg,
            "skipped_steps": skipped,
            "instruction": resolve(wf, "improve.cleanup", DEFAULT_IMPROVE_CLEANUP_INSTRUCTION, &[("doc_hint", &doc_hint), ("ctx", &ctx)]),
            "next_tool": "get_entity",
            "suggested_args": {"id": doc_arg},
            "when_done": next_step_hint
        }),
        "resolve" => serde_json::json!({
            "workflow": "improve",
            "step": step, "total_steps": total,
            "step_name": "resolve",
            "doc_id": doc_arg,
            "skipped_steps": skipped,
            "instruction": resolve(wf, "improve.resolve", DEFAULT_IMPROVE_RESOLVE_INSTRUCTION, &[("doc_hint", &doc_hint), ("stale", &stale.to_string()), ("ctx", &ctx)]),
            "next_tool": "get_review_queue",
            "suggested_args": {"doc_id": doc_arg, "include_context": true},
            "policy": {"stale_days": stale},
            "when_done": next_step_hint
        }),
        "enrich" => serde_json::json!({
            "workflow": "improve",
            "step": step, "total_steps": total,
            "step_name": "enrich",
            "doc_id": doc_arg,
            "skipped_steps": skipped,
            "instruction": resolve(wf, "improve.enrich", DEFAULT_IMPROVE_ENRICH_INSTRUCTION, &[("doc_hint", &doc_hint), ("fields", &fields), ("ctx", &ctx)]),
            "next_tool": "get_entity",
            "suggested_args": {"id": doc_arg},
            "when_done": next_step_hint
        }),
        "check" => {
            let compare_note = if !skip.is_empty() {
                ""
            } else {
                "\n\nCompare the question count to what existed before cleanup — report the net change as a measure of improvement."
            };
            serde_json::json!({
                "workflow": "improve",
                "step": step, "total_steps": total,
                "step_name": "check",
                "doc_id": doc_arg,
                "skipped_steps": skipped,
                "instruction": resolve(wf, "improve.check", DEFAULT_IMPROVE_CHECK_INSTRUCTION, &[("doc_hint", &doc_hint), ("compare_note", compare_note)]),
                "next_tool": "generate_questions",
                "suggested_args": {"doc_id": doc_arg, "dry_run": true},
                "complete": true
            })
        }
        _ => serde_json::json!({
            "workflow": "improve",
            "complete": true,
            "instruction": "Workflow complete."
        }),
    };

    if let Some(q) = quality {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("entity_quality".into(), q);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::models::{Perspective, ReviewPerspective};
    use std::collections::HashMap;

    fn wf() -> WorkflowsConfig {
        WorkflowsConfig::default()
    }

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
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &p, 0, &db, &wf());
        assert!(step["instruction"].as_str().unwrap().contains("Acme Corp"));
        assert_eq!(step["policy"]["stale_days"], 180);
    }

    #[test]
    fn test_resolve_without_perspective() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(!step["instruction"]
            .as_str()
            .unwrap()
            .contains("Knowledge base context"));
        assert_eq!(step["policy"]["stale_days"], 365);
    }

    #[test]
    fn test_ingest_includes_required_fields() {
        let p = mock_perspective();
        let step = ingest_step(3, &serde_json::json!({}), &p, &wf());
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
    }

    #[test]
    fn test_enrich_includes_required_fields() {
        let p = mock_perspective();
        let (db, _tmp) = test_db();
        let step = enrich_step(2, &serde_json::json!({}), &p, &db, &wf());
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
    }

    #[test]
    fn test_enrich_step2_mentions_scoring() {
        let (db, _tmp) = test_db();
        let step = enrich_step(2, &serde_json::json!({}), &None, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("Score this document"));
        assert!(instruction.contains("Temporal"));
    }

    #[test]
    fn test_resolve_stale_days_in_instructions() {
        let p = mock_perspective();
        let (db, _tmp) = test_db();
        // Insert a doc with a review question so step 2 returns instruction
        let content = "<!-- factbase:stl001 -->\n# Stale Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source (line 4)\n";
        insert_test_doc(&db, "stl001", content);
        let step = resolve_step(2, &serde_json::json!({}), &p, 0, &db, &wf());
        // Stale days now appear in the intro (first batch), not the per-batch instruction
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("180 days"));
    }

    #[test]
    fn test_past_last_step_returns_complete() {
        let (db, _tmp) = test_db();
        let step = resolve_step(99, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_resolve_step1_includes_deferred_note() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 5, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("5 deferred item(s)"));
        assert_eq!(step["deferred_count"], 5);
    }

    #[test]
    fn test_resolve_step1_no_deferred_note_when_zero() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(!instruction.contains("deferred"));
        assert_eq!(step["deferred_count"], 0);
    }

    #[test]
    fn test_resolve_step2_includes_conflict_patterns() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:cfp001 -->\n# Conflict Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[conflict]` Two facts overlap (line 4)\n";
        insert_test_doc(&db, "cfp001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // Conflict pattern names now in intro (first batch), not per-batch instruction
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("CONFLICT"), "intro should cover conflict type");
        assert!(intro.contains("[pattern:"), "intro should mention pattern tags");
        // Structured conflict_patterns field should also be present
        let patterns = &step["conflict_patterns"];
        assert!(patterns["parallel_overlap"].is_string());
        assert!(patterns["same_entity_transition"].is_string());
        assert!(patterns["date_imprecision"].is_string());
        assert!(patterns["unknown"].is_string());
    }

    #[test]
    fn test_resolve_step2_temporal_requires_tag_in_answer() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:tmp001 -->\n# Temporal Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Missing date (line 4)\n";
        insert_test_doc(&db, "tmp001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // Temporal guidance now in intro (first batch)
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("TEMPORAL"), "intro should cover temporal type");
        assert!(intro.contains("@t[YYYY]"), "intro must show tag format");
        assert!(intro.contains("verified"), "intro must require verification date");
        assert!(intro.contains("WRONG"), "intro must flag rejected answers");
        assert!(intro.contains("well-known"), "intro must explicitly name 'well-known' as rejected");
        assert!(intro.contains("no audit trail"), "intro must explain why dismissals are rejected");
    }

    #[test]
    fn test_improve_resolve_temporal_requires_tag_in_answer() {
        let (db, _tmp) = test_db();
        let step = improve_step(2, Some("abc123"), &None, &[], &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("MISSING an @t[...]"), "improve/resolve temporal guidance must explain the fact line is missing a tag");
        assert!(instruction.contains("tag must appear in your answer"), "must require the @t tag in the answer");
        assert!(instruction.contains("static fact"), "improve/resolve must reject 'static fact' dismissals");
        assert!(instruction.contains("no audit trail"), "improve/resolve must explain why dismissals are rejected");
    }

    // --- improve workflow tests ---

    #[test]
    fn test_improve_step1_cleanup() {
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["workflow"], "improve");
        assert_eq!(step["step"], 1);
        assert_eq!(step["total_steps"], 4);
        assert_eq!(step["step_name"], "cleanup");
        assert_eq!(step["doc_id"], "abc123");
        assert!(step["instruction"].as_str().unwrap().contains("abc123"));
        assert_eq!(step["next_tool"], "get_entity");
    }

    #[test]
    fn test_improve_step2_resolve() {
        let (db, _tmp) = test_db();
        let step = improve_step(2, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "resolve");
        assert_eq!(step["next_tool"], "get_review_queue");
        assert_eq!(step["policy"]["stale_days"], 365);
    }

    #[test]
    fn test_improve_step3_enrich() {
        let (db, _tmp) = test_db();
        let step = improve_step(3, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "enrich");
        assert_eq!(step["next_tool"], "get_entity");
    }

    #[test]
    fn test_improve_step4_check() {
        let (db, _tmp) = test_db();
        let step = improve_step(4, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "check");
        assert_eq!(step["next_tool"], "generate_questions");
        assert!(step["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_improve_past_last_step() {
        let (db, _tmp) = test_db();
        let step = improve_step(5, Some("abc123"), &None, &[], &db, &wf());
        assert!(step["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_improve_with_perspective() {
        let p = mock_perspective();
        let (db, _tmp) = test_db();
        let step = improve_step(2, Some("abc123"), &p, &[], &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("Acme Corp"));
        assert_eq!(step["policy"]["stale_days"], 180);
    }

    #[test]
    fn test_improve_skip_cleanup() {
        let skip = vec!["cleanup".to_string()];
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step["step_name"], "resolve");
        assert_eq!(step["total_steps"], 3);
    }

    #[test]
    fn test_improve_skip_multiple() {
        let skip = vec!["cleanup".to_string(), "enrich".to_string()];
        let (db, _tmp) = test_db();
        let step1 = improve_step(1, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step1["step_name"], "resolve");
        assert_eq!(step1["total_steps"], 2);
        let step2 = improve_step(2, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step2["step_name"], "check");
        assert!(step2["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_improve_skip_all_returns_error() {
        let skip: Vec<String> = IMPROVE_STEPS.iter().map(|s| s.to_string()).collect();
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &skip, &db, &wf());
        assert!(step["error"].is_string());
    }

    #[test]
    fn test_improve_no_doc_id() {
        let (db, _tmp) = test_db();
        let step = improve_step(1, None, &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "cleanup");
        // doc_id should be null
        assert!(step["doc_id"].is_null());
    }

    #[test]
    fn test_parse_skip_steps_string() {
        let args = serde_json::json!({"skip": "cleanup, enrich"});
        let skip = parse_skip_steps(&args);
        assert_eq!(skip, vec!["cleanup", "enrich"]);
    }

    #[test]
    fn test_parse_skip_steps_array() {
        let args = serde_json::json!({"skip": ["resolve", "check"]});
        let skip = parse_skip_steps(&args);
        assert_eq!(skip, vec!["resolve", "check"]);
    }

    #[test]
    fn test_parse_skip_steps_empty() {
        let args = serde_json::json!({});
        let skip = parse_skip_steps(&args);
        assert!(skip.is_empty());
    }

    #[test]
    fn test_improve_skipped_steps_reported() {
        let skip = vec!["cleanup".to_string()];
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &skip, &db, &wf());
        let skipped = step["skipped_steps"].as_array().unwrap();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0], "cleanup");
    }

    #[test]
    fn test_improve_enrich_includes_required_fields() {
        let p = mock_perspective();
        let (db, _tmp) = test_db();
        let step = improve_step(3, Some("abc123"), &p, &[], &db, &wf());
        assert!(step["instruction"].as_str().unwrap().contains("current_role"));
    }

    // --- quality stats tests ---

    fn insert_test_doc(db: &Database, id: &str, content: &str) {
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;
        test_repo_in_db(db, "test-repo", std::path::Path::new("/tmp/test"));
        db.upsert_document(&Document {
            id: id.to_string(),
            content: content.to_string(),
            title: format!("Doc {id}"),
            file_path: format!("{id}.md"),
            ..Document::test_default()
        })
        .unwrap();
    }

    #[test]
    fn test_improve_step1_includes_entity_quality_when_doc_exists() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:doc001 -->\n# Test\n\n- Fact one @t[2024-01] [^1]\n- Fact two\n- Fact three @t[2024-02]\n\n---\n[^1]: Source A";
        insert_test_doc(&db, "doc001", content);
        let step = improve_step(1, Some("doc001"), &None, &[], &db, &wf());
        let q = &step["entity_quality"];
        assert!(q.is_object(), "entity_quality should be present");
        assert!(q["total_facts"].as_u64().unwrap() > 0);
        assert!(q["attention_score"].is_number());
        assert!(q["pending_questions"].is_number());
        assert!(q["links"].is_object());
    }

    #[test]
    fn test_improve_step1_no_entity_quality_when_doc_missing() {
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("nonexistent"), &None, &[], &db, &wf());
        assert!(step.get("entity_quality").is_none());
    }

    #[test]
    fn test_improve_step2_no_entity_quality() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:doc002 -->\n# Test\n\n- Fact one";
        insert_test_doc(&db, "doc002", content);
        let step = improve_step(2, Some("doc002"), &None, &[], &db, &wf());
        assert!(step.get("entity_quality").is_none());
    }

    #[test]
    fn test_enrich_step1_includes_entity_quality_bulk() {
        let (db, _tmp) = test_db();
        let content_a = "<!-- factbase:aaa001 -->\n# Alpha\n\n- Fact one\n- Fact two";
        let content_b = "<!-- factbase:bbb001 -->\n# Beta\n\n- Fact one @t[2024-01] [^1]\n\n---\n[^1]: Source";
        insert_test_doc(&db, "aaa001", content_a);
        insert_test_doc(&db, "bbb001", content_b);
        let step = enrich_step(1, &serde_json::json!({}), &None, &db, &wf());
        let quality = step["entity_quality"].as_array().unwrap();
        assert_eq!(quality.len(), 2);
        // First item should have higher attention_score (aaa001 has no tags/sources)
        let first_score = quality[0]["attention_score"].as_u64().unwrap();
        let second_score = quality[1]["attention_score"].as_u64().unwrap();
        assert!(first_score >= second_score, "should be sorted by attention_score desc");
    }

    #[test]
    fn test_enrich_step1_empty_repo() {
        let (db, _tmp) = test_db();
        let step = enrich_step(1, &serde_json::json!({}), &None, &db, &wf());
        let quality = step["entity_quality"].as_array().unwrap();
        assert!(quality.is_empty());
    }

    #[test]
    fn test_build_quality_stats_all_covered() {
        use super::super::helpers::build_quality_stats;
        let content = "# Test\n\n- Fact one @t[2024-01] [^1]\n- Fact two @t[2024-02] [^2]\n\n---\n[^1]: Source A\n[^2]: Source B";
        let stats = build_quality_stats(content, 3, 2);
        assert_eq!(stats["links"]["outgoing"], 3);
        assert_eq!(stats["links"]["incoming"], 2);
        assert_eq!(stats["pending_questions"], 0);
        assert_eq!(stats["attention_score"], 0);
    }

    #[test]
    fn test_build_quality_stats_no_coverage() {
        use super::super::helpers::build_quality_stats;
        let content = "# Test\n\n- Fact one\n- Fact two\n- Fact three";
        let stats = build_quality_stats(content, 0, 0);
        assert_eq!(stats["total_facts"], 3);
        assert_eq!(stats["facts_with_dates"], 0);
        assert_eq!(stats["facts_with_sources"], 0);
        // attention_score = 0*2 + 3 + 3 = 6
        assert_eq!(stats["attention_score"], 6);
    }

    #[test]
    fn test_update_step1_diagnostic_narrative() {
        let step = update_step(1, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("LINKS_BEFORE"), "update step 1 should mention LINKS_BEFORE");
        assert!(instruction.contains("link density"), "should mention link density");
        assert!(instruction.contains("scan_repository"), "should call scan_repository");
    }

    #[test]
    fn test_enrich_step3_mentions_link_detection() {
        let (db, _tmp) = test_db();
        let step = enrich_step(3, &serde_json::json!({}), &None, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("link detection"), "enrich step 3 should mention link detection");
        assert!(instruction.contains("exact title"), "should emphasize exact titles");
        assert!(instruction.contains("Preserve ALL existing content"), "should warn about preserving content");
    }

    // --- setup workflow tests ---

    #[test]
    fn test_setup_step1_initialize() {
        let step = setup_step(1, &serde_json::json!({"path": "/tmp/mushrooms"}), &wf());
        assert_eq!(step["workflow"], "setup");
        assert_eq!(step["step"], 1);
        assert_eq!(step["total_steps"], 6);
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("/tmp/mushrooms"));
        assert_eq!(step["next_tool"], "init_repository");
    }

    #[test]
    fn test_setup_step1_default_path() {
        let step = setup_step(1, &serde_json::json!({}), &wf());
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("the target directory"));
    }

    #[test]
    fn test_setup_step2_perspective() {
        let step = setup_step(2, &serde_json::json!({"path": "/tmp/mushrooms"}), &wf());
        assert_eq!(step["step"], 2);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("perspective.yaml"));
        assert!(instruction.contains("focus"));
        assert!(instruction.contains("allowed_types"));
    }

    #[test]
    fn test_setup_step3_validates_perspective() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        // No perspective.yaml → error
        let step = setup_step(3, &serde_json::json!({"path": path}), &wf());
        assert_eq!(step["perspective_status"], "error");

        // Write valid perspective
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Mycology\nallowed_types:\n  - species\n",
        )
        .unwrap();
        let step = setup_step(3, &serde_json::json!({"path": path}), &wf());
        assert_eq!(step["perspective_status"], "ok");
        let parsed = step["perspective_parsed"].as_str().unwrap();
        assert!(parsed.contains("Mycology"));
        assert!(parsed.contains("species"));
    }

    #[test]
    fn test_setup_step4_create_documents() {
        let step = setup_step(4, &serde_json::json!({}), &wf());
        assert_eq!(step["step"], 4);
        assert_eq!(step["next_tool"], "get_authoring_guide");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("create_document"));
        assert!(instruction.contains("@t[=2024]"));
        assert!(instruction.contains("[^1]"));
        assert!(instruction.contains("get_authoring_guide"));
    }

    #[test]
    fn test_setup_step5_scan_and_verify() {
        let step = setup_step(5, &serde_json::json!({}), &wf());
        assert_eq!(step["step"], 5);
        assert_eq!(step["next_tool"], "scan_repository");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("scan_repository"));
        assert!(instruction.contains("check_repository"));
    }

    #[test]
    fn test_format_rules_inlined_in_document_creation_steps() {
        // Setup step 4, ingest step 3, and enrich step 3 should all inline format rules
        // so weaker models don't need a separate get_authoring_guide call.
        let setup = setup_step(4, &serde_json::json!({}), &wf());
        let ingest = ingest_step(3, &serde_json::json!({}), &None, &wf());
        let (db, _tmp) = test_db();
        let enrich = enrich_step(3, &serde_json::json!({}), &None, &db, &wf());

        for (name, step) in [("setup", setup), ("ingest", ingest), ("enrich", enrich)] {
            let instruction = step["instruction"].as_str().unwrap();
            assert!(
                instruction.contains("@t[=2024]"),
                "{name} step missing temporal tag examples"
            );
            assert!(
                instruction.contains("[^1]"),
                "{name} step missing source footnote examples"
            );
            assert!(
                instruction.contains("get_authoring_guide"),
                "{name} step should still mention get_authoring_guide"
            );
        }
    }

    #[test]
    fn test_setup_step6_next_steps() {
        let step = setup_step(6, &serde_json::json!({}), &wf());
        assert_eq!(step["step"], 6);
        assert!(step["complete"].as_bool().unwrap());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("ingest"));
        assert!(instruction.contains("enrich"));
        assert!(instruction.contains("update"));
    }

    #[test]
    fn test_setup_past_last_step() {
        let step = setup_step(99, &serde_json::json!({}), &wf());
        assert!(step["complete"].as_bool().unwrap());
    }

    // --- bootstrap workflow tests ---

    #[test]
    fn test_build_bootstrap_prompt_basic() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("mycology", None, &prompts);
        assert!(prompt.contains("mycology"));
        assert!(prompt.contains("document_types"));
        assert!(prompt.contains("folder_structure"));
        assert!(prompt.contains("templates"));
        assert!(prompt.contains("perspective"));
        assert!(!prompt.contains("suggested these entity types"));
    }

    #[test]
    fn test_build_bootstrap_prompt_with_entity_types() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("mycology", Some("species, habitats, researchers"), &prompts);
        assert!(prompt.contains("mycology"));
        assert!(prompt.contains("species, habitats, researchers"));
        assert!(prompt.contains("suggested these entity types"));
    }

    #[test]
    fn test_parse_bootstrap_response_valid_json() {
        let json = r#"{"document_types": [{"name": "species"}], "folder_structure": ["species/"]}"#;
        let result = parse_bootstrap_response(json);
        assert!(result.is_some());
        let v = result.unwrap();
        assert!(v["document_types"].is_array());
    }

    #[test]
    fn test_parse_bootstrap_response_json_in_text() {
        let text = "Here is the structure:\n{\"document_types\": [{\"name\": \"species\"}]}\nDone.";
        let result = parse_bootstrap_response(text);
        assert!(result.is_some());
        assert!(result.unwrap()["document_types"].is_array());
    }

    #[test]
    fn test_parse_bootstrap_response_invalid() {
        let result = parse_bootstrap_response("This is not JSON at all");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_bootstrap_with_mock_llm() {
        use crate::llm::test_helpers::MockLlm;

        let mock_response = r##"{"document_types":[{"name":"species","description":"Mushroom species"}],"folder_structure":["species/","habitats/"],"templates":{"species":"# Species Name"},"perspective":{"focus":"Mycology research","allowed_types":["species","habitats"]}}"##;
        let llm = MockLlm::new(mock_response);
        let args = serde_json::json!({"domain": "mycology", "path": "/tmp/mushrooms"});
        let result = bootstrap(&llm, &args).await.unwrap();

        assert_eq!(result["workflow"], "bootstrap");
        assert_eq!(result["domain"], "mycology");
        assert!(result["suggestions"]["document_types"].is_array());
        assert!(result["suggestions"]["folder_structure"].is_array());
        assert!(result["suggestions"]["templates"].is_object());
        assert!(result["suggestions"]["perspective"].is_object());
        assert!(result["next_steps"].is_array());
        let steps = result["next_steps"].as_array().unwrap();
        // next_steps should route into setup workflow, not list raw tool calls
        let all_steps = steps.iter().map(|s| s.as_str().unwrap_or("")).collect::<Vec<_>>().join(" ");
        assert!(all_steps.contains("workflow"), "next_steps should mention workflow");
        assert!(all_steps.contains("setup"), "next_steps should route to setup workflow");
    }

    #[tokio::test]
    async fn test_bootstrap_unparseable_response() {
        use crate::llm::test_helpers::MockLlm;

        let llm = MockLlm::new("I don't know how to generate JSON");
        let args = serde_json::json!({"domain": "mycology"});
        let result = bootstrap(&llm, &args).await.unwrap();

        assert_eq!(result["workflow"], "bootstrap");
        assert!(result["suggestions"]["error"].is_string());
        assert!(result["suggestions"]["raw_response"].is_string());
    }

    #[tokio::test]
    async fn test_bootstrap_requires_domain() {
        use crate::llm::test_helpers::MockLlm;

        let llm = MockLlm::new("{}");
        let args = serde_json::json!({});
        let result = bootstrap(&llm, &args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_list_includes_bootstrap() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let names: Vec<&str> = workflows
            .iter()
            .filter_map(|w| w["name"].as_str())
            .collect();
        assert!(names.contains(&"bootstrap"));
    }

    #[test]
    fn test_workflow_bootstrap_without_llm_returns_error() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "bootstrap"});
        let result = workflow(&db, &args).unwrap();
        assert!(result["error"].is_string());
    }

    #[test]
    fn test_setup_step1_mentions_bootstrap() {
        let step = setup_step(1, &serde_json::json!({"path": "/tmp/test"}), &wf());
        assert!(step["instruction"].as_str().unwrap().contains("bootstrap"));
    }

    #[test]
    fn test_format_rules_has_negative_examples_for_all_categories() {
        // Entity names
        assert!(FORMAT_RULES.contains("Wolfgang Amadeus Mozart"), "missing entity name");
        // Descriptions
        assert!(FORMAT_RULES.contains("Complex counterpoint"), "missing description");
        // Statuses
        assert!(FORMAT_RULES.contains("Active Production Status"), "missing status");
        // Statistics
        assert!(FORMAT_RULES.contains("Total Produced: 650+"), "missing statistic");
        // Vague time words
        assert!(FORMAT_RULES.contains("seasonal"), "missing vague time word");
    }

    #[test]
    fn test_bootstrap_prompt_has_temporal_tag_negative_examples() {
        assert!(DEFAULT_BOOTSTRAP_PROMPT.contains("NEVER names, descriptions"), "missing negative guidance in bootstrap prompt");
        assert!(DEFAULT_BOOTSTRAP_PROMPT.contains("Wolfgang Amadeus Mozart"), "missing entity name example in bootstrap prompt");
    }

    // --- workflow config override tests ---

    #[test]
    fn test_workflow_config_override_in_step() {
        let mut wfc = WorkflowsConfig::default();
        wfc.templates.insert("update.scan".into(), "Custom scan: {ctx}".into());
        let p = mock_perspective();
        let step = update_step(1, &serde_json::json!({}), &p, &wfc);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.starts_with("Custom scan:"));
        assert!(instruction.contains("Acme Corp"));
    }

    #[test]
    fn test_workflow_config_override_improve() {
        let mut wfc = WorkflowsConfig::default();
        wfc.templates.insert("improve.cleanup".into(), "My cleanup for {doc_hint}".into());
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &[], &db, &wfc);
        assert!(step["instruction"].as_str().unwrap().starts_with("My cleanup for"));
        assert!(step["instruction"].as_str().unwrap().contains("abc123"));
    }

    #[test]
    fn test_workflow_config_fallback_to_default() {
        // Empty config should produce the same output as default
        let step_default = update_step(2, &serde_json::json!({}), &None, &wf());
        let step_empty = update_step(2, &serde_json::json!({}), &None, &WorkflowsConfig::default());
        assert_eq!(step_default["instruction"], step_empty["instruction"]);
    }

    // --- resolve step 2 batch tests ---

    /// Helper: create a doc with N unanswered review questions of given types.
    fn insert_doc_with_questions(db: &Database, id: &str, types: &[&str]) {
        let questions: String = types
            .iter()
            .enumerate()
            .map(|(i, t)| format!("- [ ] `@q[{t}]` Question {} (line {})\n", i + 1, i + 4))
            .collect();
        let content = format!(
            "<!-- factbase:{id} -->\n# Doc {id}\n\n- Fact\n\n<!-- factbase:review -->\n{questions}"
        );
        insert_test_doc(db, id, &content);
    }

    #[test]
    fn test_resolve_step2_empty_queue_advances_to_step3() {
        let (db, _tmp) = test_db();
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["step"], 2);
        let batch = &step["batch"];
        assert_eq!(batch["remaining"], 0);
        assert_eq!(batch["resolved_so_far"], 0);
        assert!(batch["questions"].as_array().unwrap().is_empty());
        assert!(step["when_done"].as_str().unwrap().contains("step=3"));
    }

    #[test]
    fn test_resolve_step2_returns_batch_of_questions() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "bat001", &["temporal", "missing", "stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(batch["questions"].as_array().unwrap().len(), 3);
        assert_eq!(batch["remaining"], 3);
        assert_eq!(batch["resolved_so_far"], 0);
        assert_eq!(batch["batch_number"], 1);
        // Should loop back to step 2
        assert!(step["when_done"].as_str().unwrap().contains("step=2"));
    }

    #[test]
    fn test_resolve_step2_first_batch_includes_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "int001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // First batch (resolved_so_far=0) should have intro
        assert!(step["intro"].is_string(), "first batch should include intro");
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("TEMPORAL"), "intro should describe question types");
        assert!(intro.contains("STALE"), "intro should describe question types");
    }

    #[test]
    fn test_resolve_step2_subsequent_batch_no_intro() {
        let (db, _tmp) = test_db();
        // Insert a doc with an answered question (resolved) and an unanswered one
        let content = "<!-- factbase:sub001 -->\n# Sub Test\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[missing]` Unanswered (line 5)\n";
        insert_test_doc(&db, "sub001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // resolved_so_far > 0, so no intro
        assert!(step.get("intro").is_none(), "subsequent batch should not include intro");
    }

    #[test]
    fn test_resolve_step2_batch_size_limits_questions() {
        let (db, _tmp) = test_db();
        // Insert 20 questions across two docs (more than RESOLVE_BATCH_SIZE=15)
        let types_10: Vec<&str> = vec!["temporal"; 10];
        insert_doc_with_questions(&db, "big001", &types_10);
        insert_doc_with_questions(&db, "big002", &types_10);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(batch["questions"].as_array().unwrap().len(), RESOLVE_BATCH_SIZE);
        assert_eq!(batch["remaining"], 20);
        assert_eq!(batch["total_batches_estimate"], 2);
        assert!(step["when_done"].as_str().unwrap().contains("step=2"));
    }

    #[test]
    fn test_resolve_step2_questions_ordered_by_doc_then_type() {
        let (db, _tmp) = test_db();
        // Doc aaa: conflict, temporal → should sort as temporal, conflict
        // Doc bbb: stale → comes after aaa
        insert_doc_with_questions(&db, "aaa001", &["conflict", "temporal"]);
        insert_doc_with_questions(&db, "bbb001", &["stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 3);
        // First two from aaa001, sorted by type priority (temporal < conflict)
        assert_eq!(questions[0]["doc_id"], "aaa001");
        assert_eq!(questions[0]["type"], "temporal");
        assert_eq!(questions[1]["doc_id"], "aaa001");
        assert_eq!(questions[1]["type"], "conflict");
        // Third from bbb001
        assert_eq!(questions[2]["doc_id"], "bbb001");
        assert_eq!(questions[2]["type"], "stale");
    }

    #[test]
    fn test_resolve_step2_questions_include_doc_context() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "ctx001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let q = &step["batch"]["questions"].as_array().unwrap()[0];
        assert!(q["doc_id"].is_string());
        assert!(q["doc_title"].is_string());
        assert!(q["question_index"].is_number());
        assert!(q["type"].is_string());
        assert!(q["description"].is_string());
    }

    #[test]
    fn test_resolve_step2_config_override_answer_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "cfg001", &["temporal"]);
        let mut wfc = WorkflowsConfig::default();
        wfc.templates.insert("resolve.answer_intro".into(), "Custom intro {ctx}".into());
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wfc);
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.starts_with("Custom intro"));
    }

    #[test]
    fn test_resolve_step2_has_completion_gate_when_remaining() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "gate01", &["temporal", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let gate = step["completion_gate"].as_str().unwrap();
        assert!(gate.contains("2 questions remain"));
        assert!(gate.contains("Do NOT proceed to step 3"));
        assert!(gate.contains("step=2"));
    }

    #[test]
    fn test_resolve_step2_has_progress_field() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "prg001", &["temporal", "stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let progress = step["progress"].as_str().unwrap();
        assert!(progress.contains("Batch 1"));
        assert!(progress.contains("0 answered"));
        assert!(progress.contains("2 remaining"));
    }

    #[test]
    fn test_resolve_step2_empty_queue_has_no_completion_gate() {
        let (db, _tmp) = test_db();
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("completion_gate").is_none());
        assert!(step.get("progress").is_none());
    }

    #[test]
    fn test_resolve_step2_answer_instruction_warns_no_skip() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "skip01", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("Do NOT skip ahead to step 3"));
    }

    #[test]
    fn test_resolve_step2_intro_mentions_multiple_batches() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "mul001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("multiple batches"));
        assert!(intro.contains("system will tell you when all questions are resolved"));
    }

    #[test]
    fn test_check_repository_workflow_texts_mention_checked_pair_ids_cursor() {
        // Every workflow instruction that tells agents to loop on check_repository
        // must mention passing back checked_pair_ids to avoid restarting from scratch.
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let update = update_step(2, &serde_json::json!({}), &None, &wf());

        for (name, step) in [("setup.scan", setup), ("update.check", update)] {
            let instruction = step["instruction"].as_str().unwrap();
            assert!(
                instruction.contains("checked_pair_ids"),
                "{name} workflow mentions check_repository continue:true but not the checked_pair_ids cursor"
            );
        }
    }

    #[test]
    fn test_check_repository_schema_mentions_cursor() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let check = tools_arr.iter().find(|t| t["name"] == "check_repository").unwrap();
        let desc = check["description"].as_str().unwrap();
        assert!(
            desc.contains("checked_pair_ids"),
            "check_repository schema description should mention checked_pair_ids cursor"
        );
    }

    #[test]
    fn test_time_budget_progress_message_warns_incomplete() {
        let mut resp = serde_json::json!({"ok": true});
        crate::mcp::tools::helpers::apply_time_budget_progress(&mut resp, 3, 10, "check_repository", true);
        let msg = resp["message"].as_str().unwrap();
        assert!(msg.contains("7 remaining"));
        assert!(msg.contains("same arguments"));
        assert!(msg.contains("Do NOT report partial results"));
    }
}
