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
use crate::models::{Document, Perspective, QuestionType};
use crate::processor::parse_review_queue;
use crate::question_generator::extract_acronym_from_question;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::config::workflows::{resolve_workflow_text, WorkflowsConfig};
use super::helpers::{build_quality_stats, detect_weak_identification, load_perspective, resolve_repo_filter};
use super::review::format_question_json;
use super::{get_str_arg, get_str_arg_required, get_u64_arg};

/// Resolve the filesystem path for a repository (first repo if none specified).
fn resolve_repo_path(db: &Database, repo_id: Option<&str>) -> Option<std::path::PathBuf> {
    let repos = db.list_repositories().ok()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.map(|r| r.path)
}

/// Compact format rules inlined into workflow steps so weaker models don't need a separate get_authoring_guide call.
const FORMAT_RULES: &str = "\n\n**⚠️ FORMAT RULES — read carefully:**\n\n**Temporal tags** — ONLY dates/years go inside @t[...]. NEVER put names, descriptions, statuses, or any other text inside.\n- ✅ CORRECT: `@t[=2024]` `@t[~2024]` `@t[2020..2023]` `@t[2024..]` `@t[?]` `@t[=331 BCE]` `@t[=-0490]`\n- ❌ WRONG (entity names): `@t[Wolfgang Amadeus Mozart]` `@t[Mount Vesuvius]`\n- ❌ WRONG (descriptions): `@t[Complex counterpoint and fugal writing]` `@t[bright red when young]`\n- ❌ WRONG (statuses): `@t[Active Production Status: Ongoing]` `@t[No significant seismic activity]`\n- ❌ WRONG (statistics): `@t[Total Produced: 650+]` `@t[Population: 12000]`\n- ❌ WRONG (vague time words): `@t[seasonal]` `@t[since ancient times]` `@t[traditional..modern]`\n- Syntax: `@t[=YYYY]` exact date, `@t[~YYYY]` approximate, `@t[YYYY..YYYY]` range, `@t[YYYY..]` ongoing, `@t[?]` unknown\n- BCE dates: `@t[=331 BCE]` or `@t[=-331]` or `@t[=-0331]` — all equivalent\n- Place the tag AFTER the fact text: `- Cap color: red to orange @t[~2024] [^1]`\n- If you don't know the date, use `@t[?]` — NEVER put text descriptions inside the brackets\n\n**Source footnotes** on every fact: `[^1]` inline, then `---\\n[^1]: Author, Title, Date` at bottom\n\nCall get_authoring_guide for the full format reference";

// ---------------------------------------------------------------------------
// Default instruction templates for each workflow step.
// Use {placeholder} syntax for dynamic parts; resolved via config overrides.
// ---------------------------------------------------------------------------

/// Build the sub-agent fanout hint. For large queues (>200) the language is
/// directive ("DO IT NOW"); for smaller queues it remains a strong suggestion.
fn subagent_fanout_hint(total_questions: usize, type_dist: &[(String, usize)]) -> String {
    let workers: String = type_dist
        .iter()
        .filter(|(_, c)| *c > 0)
        .enumerate()
        .map(|(i, (qt, _))| format!("- Worker {}: workflow(workflow='resolve', step=2, question_type='{qt}')", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    if total_questions > 200 {
        format!(
            "\n\n🚀 PARALLEL DISPATCH (MANDATORY for {total_questions} questions): If you can spawn sub-agents, DO IT NOW. \
Spawn one worker per question type. This is the fastest path to clearing the queue.\n\
{workers}\n\
Each worker processes its type independently. Do NOT work serially — fan out immediately."
        )
    } else {
        format!(
            "\n\n🚀 PARALLEL DISPATCH: If you can spawn sub-agents, spawn one worker per question type — \
this is significantly faster than serial processing.\n\
{workers}\n\
Each worker processes its type independently."
        )
    }
}

// --- Setup workflow ---
pub(crate) const DEFAULT_SETUP_INIT_INSTRUCTION: &str = "Initialize a new factbase repository at '{path}'. Call init_repository with path='{path}'.\n\nAfter initialization, the directory will contain a perspective.yaml file that needs to be configured in the next step.\n\nTip: If you're unsure what document types and folder structure to use for this domain, call workflow='bootstrap' with a domain description first — it will generate tailored suggestions.\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=2)";

pub(crate) const DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION: &str = "Configure the repository's perspective. Write the file `{path}/perspective.yaml` with YAML content like this:\n\n```yaml\nfocus: \"What this knowledge base is about\"\norganization: \"Who maintains it (optional)\"\nallowed_types:\n  - type1\n  - type2\n  - type3\nreview:\n  stale_days: 180\n  required_fields:\n    type1: [field1, field2]\n    type2: [field1, field2]\n```\n\nIf you ran bootstrap first, use the perspective values it suggested. Otherwise choose values appropriate for the domain.\n\n⚠️ This MUST be valid YAML written to `perspective.yaml` (not .md, not .json). The file goes in the repository root, not in .factbase/.\n\nAlso plan the folder structure — each allowed_type becomes a top-level folder. Documents are placed in type folders (e.g., `species/amanita-muscaria.md`).\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=3)";

pub(crate) const DEFAULT_SETUP_VALIDATE_OK_INSTRUCTION: &str = "✅ perspective.yaml parsed successfully:\n  {detail}\n\nIf this looks correct, proceed to the next step.\n\n⚠️ NEXT: Call workflow(workflow='setup', step=4)";

pub(crate) const DEFAULT_SETUP_VALIDATE_ERROR_INSTRUCTION: &str = "❌ {detail}\n\n⚠️ NEXT: Fix perspective.yaml, then call workflow(workflow='setup', step=3) again to re-validate.";

pub(crate) const DEFAULT_SETUP_CREATE_INSTRUCTION: &str = "Create 2-3 example documents using create_document.\n\nIMPORTANT: First call get_authoring_guide to learn the required document format (temporal tags, footnotes, structure).\n\nTips for first documents:\n- Place each in the appropriate type folder (e.g., 'species/amanita-muscaria.md')\n- Start with a clear # Title\n- Use exact entity names that match other document titles for automatic cross-linking\n- A definitions/ document for domain terminology is a good first document{format_rules}\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=5)";

pub(crate) const DEFAULT_SETUP_SCAN_INSTRUCTION: &str = "Index and verify the new repository.\n\n1. Call scan_repository with time_budget_secs=120 to generate document embeddings and detect links.\n   ⚠️ PAGING: This tool is time-boxed. It WILL return `continue: true` with a `resume` token for any non-trivial repository.\n   When it does, you MUST call it again passing the resume token until `continue` is no longer in the response. Do NOT stop early.\n2. Call check_repository to run quality checks and see initial issues.\n3. Report what the scan found: how many documents were indexed, how many links were detected, and any quality issues from the check.\n\n⚠️ NEXT: When done, you MUST call: workflow(workflow='setup', step=6)";

pub(crate) const DEFAULT_SETUP_COMPLETE_INSTRUCTION: &str = "The repository is set up! Summarize what was created and suggest next steps:\n\n- **Add more content**: Use workflow='ingest' with a topic to research and add documents\n- **Fill gaps**: Use workflow='enrich' to find and fill missing information\n- **Quality check**: Use workflow='update' periodically to scan, check quality, and detect reorganization opportunities\n- **Fix issues**: Use workflow='resolve' to address any review questions\n- **Improve a document**: Use workflow='improve' with a doc_id to improve a specific document end-to-end\n\nThe knowledge base is ready for use. Any markdown editor can modify files directly — just run scan_repository afterward to re-index.";

// --- Update workflow ---
pub(crate) const DEFAULT_UPDATE_SCAN_INSTRUCTION: &str = "Re-index the factbase to pick up file changes and detect cross-entity links.\n\n1. Call scan_repository with time_budget_secs=120.\n   ⚠️ PAGING: This tool is time-boxed. It WILL return `continue: true` with a `resume` token for any non-trivial repository.\n   When it does, you MUST call it again passing the resume token until `continue` is no longer in the response.\n   This may take many iterations — that is normal. Do NOT stop early, skip ahead, or report partial results.\n2. Record: documents_total, links_detected, temporal_coverage_pct, source_coverage_pct\n3. Save links_detected as LINKS_BEFORE — you'll compare after entity creation{ctx}";

pub(crate) const DEFAULT_UPDATE_CHECK_INSTRUCTION: &str = "Run quality checks to find stale facts, missing sources, temporal gaps, and other issues.\n\n1. Call check_repository (one call — no paging needed).\n2. Record: questions_total, breakdown by type (stale, conflict, temporal, missing)\n   - Mostly stale → KB is aging, needs fresh sources\n   - Mostly temporal → facts lack dates, timeline is murky\n   - Mostly missing → claims lack evidence";

pub(crate) const DEFAULT_UPDATE_CROSS_VALIDATE_INSTRUCTION: &str = "Review cross-document fact pairs to find contradictions between documents.\n\n1. Call get_fact_pairs to retrieve embedding-similar fact pairs across documents.\n   - Each pair contains two facts from different documents with their text, line numbers, and similarity score.\n   - Pairs where a cross-check question already exists are excluded.\n\n2. For each pair, classify the relationship:\n   - CONSISTENT: Facts are compatible or about different aspects\n   - CONTRADICTS: Facts give different answers to the same question about the same entity\n   - SUPERSEDES: One fact provides newer information that replaces the other\n\n3. For CONTRADICTS or SUPERSEDES pairs, create a review question:\n   - Call answer_questions with the target doc_id, the fact's line number as question_index context,\n     and a description like: \"Cross-check with {other_doc_title}: {fact_text} — {reason}\"\n   - Use @q[conflict] for contradictions, @q[stale] for superseded facts\n\n4. Record: pairs_reviewed, conflicts_found";

pub(crate) const DEFAULT_UPDATE_LINKS_INSTRUCTION: &str = "Review link suggestions to improve cross-document connectivity.\n\n1. Call get_link_suggestions TWICE for better coverage:\n   a. Cross-type discovery: use exclude_types matching the most common doc type (e.g., exclude_types=[\"person\"] if reviewing people docs) with min_similarity=0.5. This finds connections between different entity types.\n   b. Same-type discovery: use include_types matching a specific type with min_similarity=0.7. This finds related entities of the same kind.\n2. Review each suggestion: does the candidate document genuinely relate to the source?\n3. For confirmed links, call store_links with the source_id and target_id pairs.\n   - store_links writes `References:` to source files and `Referenced by:` to target files, and updates the database.\n4. Record: links_added, documents_modified";

pub(crate) const DEFAULT_UPDATE_ORGANIZE_INSTRUCTION: &str = "Analyze the knowledge base structure for improvement opportunities.\n\n1. Call organize_analyze (one call — no paging needed).\n2. Record candidates:\n   - Merge: documents that overlap significantly — telling the same story twice\n   - Split: documents covering multiple distinct topics\n   - Misplaced: documents whose type doesn't match their content\n   - Duplicates: repeated facts across documents\n3. Do NOT execute changes — just record what you find";

pub(crate) const DEFAULT_UPDATE_SUMMARY_INSTRUCTION: &str = "Write a diagnostic report combining metrics and assessment.\n\n## Scan & Links\n- Documents: X | Links: X\n- Temporal coverage: X% | Source coverage: X%\n- Link health: [healthy / needs work / poor] — each doc should average 1+ link\n\n## Quality Issues\n- Total questions: X (stale: X, conflict: X, temporal: X, missing: X)\n- Dominant issue type tells you the KB's biggest weakness\n\n## Organization\n- Merge/split/misplaced/duplicate candidates found\n\n## Health Assessment\nOne paragraph: overall KB health, biggest strength, biggest gap, and top 3 priorities ordered by impact.";

// --- Resolve workflow ---
pub(crate) const DEFAULT_RESOLVE_QUEUE_INSTRUCTION: &str = "Process types in recommended_order (fewest questions first = quick wins). Start with: workflow resolve step=2 question_type=<first_type>. Clear each type completely before moving to the next. Skip types with 0 questions.{ctx}{deferred_note}";

pub(crate) const DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION: &str = "You are resolving review questions in batches. The system feeds you 15 questions at a time. You will receive multiple batches. Answer each batch and call step 2 again. The system will tell you when all questions are resolved.\n\n⚠️ EVIDENCE REQUIREMENT: Every answer MUST cite an external source (URL, document ID, book, API result, or other verifiable reference). 'Well-known historical fact', 'still accurate', or 'training data' are NOT acceptable evidence. If you cannot find an external source confirming the claim, DEFER — that is the correct action.\n\nCONFIDENCE LEVELS:\n- **verified**: You consulted an external source and found confirmation. Include the source reference (URL, document ID, API response, etc.). Use confidence='verified' (or omit — it's the default). These answers WILL be applied.\n- **believed**: You are confident from training data but did NOT find external confirmation. Use confidence='believed'. These answers stay in the queue for human review and are NOT applied.\n- **defer**: You researched and could not confirm. Prefix with 'defer:' and explain what you tried. A good defer with reasoning is better than a confident guess without evidence. Deferring means you did your job — you researched and couldn't confirm.\n\nANSWER FORMAT BY TYPE:\n\nTEMPORAL — fact line has no @t[...] tag.\n→ MUST include the tag: \"@t[YYYY] per [source] ([reference]); verified YYYY-MM-DD\"\n→ Ranges: @t[YYYY..YYYY], ongoing: @t[YYYY..], BCE: @t[=-480] or @t[=480 BCE]\n→ Unknown date: @t[?] (only when truly unfindable)\n→ WRONG: \"well-known\", \"static\", \"doesn't change\" — rejected, no audit trail\n\nMISSING — fact has no source citation.\n→ \"Source: [name] ([reference]), [date]\"\n\nSTALE — source older than {stale} days.\n→ Research \"{entity} {fact} {current year}\"\n→ Still true: \"Still accurate per [source] ([reference]), verified [date]\"\n→ Changed: \"Updated: [new info] per [source] ([reference])\"\n\nCONFLICT — two facts disagree. Read the [pattern:...] tag.\n→ Both valid (parallel, different entities): \"Not a conflict: [reason]\"\n→ One supersedes: \"Transition: adjust end date to [date] per [source]\"\n→ One wrong: \"[correct fact] per [source], remove [wrong fact]\"\n→ Cross-doc: call get_entity on referenced doc for context\n\nAMBIGUOUS → clarify or create definitions/ file\nDUPLICATE → \"Duplicate of [doc_id], remove from here\"\nPRECISION → replace vague term with specific value: \"heavy losses\" → \"~500 casualties\" per [source]\nWEAK-SOURCE → use your tools to find the actual source. Update the footnote with a specific reference (URL, path, page, ISBN, RFC, channel/thread+date). If you cannot find it, change to '[^N]: UNVERIFIED — original claim: <text>'. Do not invent specific-looking citations.\n\nEXAMPLES:\n\n✅ GOOD verified answer: \"@t[2019..2023] per Wikipedia (https://en.wikipedia.org/wiki/Example); verified 2026-02-28\"\n✅ GOOD verified answer: \"@t[=2024-03] per internal doc fb:3a2c1e; verified 2026-02-28\"\n✅ GOOD defer: \"defer: Researched 'entity role 2026' using available tools — no results confirming current status\"\n❌ BAD answer: \"Still accurate, well-documented historical fact\" (no source, no evidence)\n❌ BAD answer: \"This is common knowledge\" (not verifiable)\n\nCan't find a source? → defer: researched [what], found [nothing/insufficient]. This is the RIGHT answer when evidence is lacking.{ctx}";

pub(crate) const DEFAULT_RESOLVE_ANSWER_INSTRUCTION: &str = "Your goal is to ANSWER questions, not analyze them. Answer from knowledge first. Only research if you genuinely cannot answer. Every tool call that is not answer_questions is reducing your throughput. Minimize research calls.\n\nAnswer the questions in this batch. For each question: call answer_questions with doc_id, question_index, answer, and confidence.\n- Found a source? → confidence='verified' (or omit), include the source reference\n- Confident but no source found? → confidence='believed' (stays in queue for human review)\n- Researched and found nothing? → 'defer: researched [what], found [nothing]' — this is the correct action\n\nDo not spend context on statistics, breakdowns, or pattern analysis — spend it on answer_questions calls. Report progress by questions answered, not by patterns observed.\n\n⚠️ SCOPE: The resolve workflow is ONLY for answering existing questions. Do NOT call scan_repository or check_repository — those belong to the update workflow. Stay focused on the current batch.\n\n⚠️ CONTINUATION: After answering this batch, call workflow(workflow='resolve', step=2) to get the next batch. The workflow returns continue=true when more batches remain.

Process as many batches as you can. If you must stop (context limits, timeout), report progress honestly: answered/remaining/deferred counts. Commit your work (git add/commit) so the next session picks up where you left off — answered questions are tracked in the DB and won't reappear. Resume with workflow(workflow='resolve') to continue.{ctx}";

// --- Variant A: Type-specific evidence standards ---
pub(crate) const VARIANT_TYPE_EVIDENCE_INTRO: &str = "You are resolving review questions in batches. The system feeds you 15 questions at a time. You will receive multiple batches. Answer each batch and call step 2 again. The system will tell you when all questions are resolved.\n\n⚠️ EVIDENCE REQUIREMENT — varies by question type:\n\nSTALE: Search for the claim + current year. Cite a URL confirming or updating it. Wikipedia is acceptable for well-established facts.\n→ Still true: \"Still accurate per [source] ([URL]), verified [date]\"\n→ Changed: \"Updated: [new info] per [source] ([URL])\"\n\nTEMPORAL: Search for the specific event date. Cite a URL with the date.\n→ Format: \"@t[YYYY-MM-DD] per [source] ([URL]); verified YYYY-MM-DD\"\n→ Ranges: @t[YYYY..YYYY], ongoing: @t[YYYY..], BCE: @t[=-480] or @t[=480 BCE]\n→ Unknown date: @t[?] (only when truly unfindable)\n\nAMBIGUOUS: Check the KB first (get_entity, read other docs). If the term is defined elsewhere in the KB, cite that doc. Only search externally if KB has no answer.\n→ KB has answer: \"Clarified per [doc_id]: [definition]\"\n→ External: \"Clarified per [source] ([URL]): [definition]\"\n\nCONFLICT: Read BOTH referenced documents (get_entity). Search for the specific claim in each. Compare sources by recency and authority. If genuinely unresolvable, defer with analysis of both sides.\n→ Both valid: \"Not a conflict: [reason] per [source]\"\n→ One supersedes: \"[correct fact] per [source], supersedes [old fact]\"\n→ Unresolvable: \"defer: [analysis of both sides with sources]\"\n\nPRECISION: Search for a quantitative replacement. If no specific number exists in sources, defer — do not guess.\n→ Found: \"[specific value] per [source] ([URL])\"\n→ Not found: \"defer: searched for specific value, no authoritative source found\"\n\nMISSING: Find a source citation for the unsourced fact.\n→ \"Source: [name] ([URL]), [date]\"\n\nDUPLICATE: Identify the canonical entry.\n→ \"Duplicate of [doc_id], remove from here\"\n\nCONFIDENCE LEVELS:\n- **verified**: External source found and cited. These answers WILL be applied.\n- **believed**: Confident but no external source. Stays in queue for human review.\n- **defer**: Researched and could not confirm. A good defer is better than a guess.\n\nEXAMPLES:\n✅ GOOD: \"@t[1942-06-04..1942-06-07] per Wikipedia (https://en.wikipedia.org/wiki/Battle_of_Midway); verified 2026-03-01\"\n✅ GOOD defer: \"defer: Searched 'entity role 2026' — no results confirming current status\"\n❌ BAD: \"Well-known historical fact\" (no source)\n\nCan't find a source? → defer. This is the correct action.{ctx}";

pub(crate) const VARIANT_TYPE_EVIDENCE_ANSWER: &str = "Answer the questions in this batch. Each question has an `evidence_guidance` field with type-specific research instructions — follow them.\n\nFor each question: follow the evidence_guidance, research, then call answer_questions with doc_id, question_index, answer, and confidence.\n- Found a source? → confidence='verified' (or omit), include the source reference\n- Confident but no source found? → confidence='believed'\n- Researched and found nothing? → 'defer: researched [what], found [nothing]'\n\n⚠️ SCOPE: The resolve workflow is ONLY for answering existing questions. Do NOT call scan_repository or check_repository.\n\n⚠️ CONTINUATION: After answering this batch, call workflow(workflow='resolve', step=2) to get the next batch. The workflow returns continue=true when more batches remain.

Process as many batches as you can. If you must stop, report progress honestly: answered/remaining/deferred counts. Commit your work (git add/commit) and resume with workflow(workflow='resolve') — answered questions are tracked in the DB and won't reappear.{ctx}";

// --- Variant B: Research-then-batch ---
pub(crate) const VARIANT_RESEARCH_BATCH_INTRO: &str = "You are resolving review questions using a research-first approach. Questions are grouped by document. For each document group:\n\nPHASE 1 — RESEARCH: Call get_entity to read the full document. Then do ONE comprehensive search covering all its questions. Gather all evidence before answering anything.\n\nPHASE 2 — ANSWER: Answer ALL questions for that document in one answer_questions call, citing the research from Phase 1.\n\nThis reduces redundant searches — multiple questions about the same document/topic share research.\n\n⚠️ EVIDENCE REQUIREMENT: Every answer MUST cite an external source. If you cannot find evidence, DEFER.\n\nCONFIDENCE LEVELS:\n- **verified**: External source found and cited. These answers WILL be applied.\n- **believed**: Confident but no external source. Stays in queue for human review.\n- **defer**: Researched and could not confirm. A good defer is better than a guess.\n\nANSWER FORMAT BY TYPE:\nTEMPORAL → \"@t[YYYY] per [source] ([reference]); verified YYYY-MM-DD\"\nSTALE → \"Still accurate per [source] ([reference]), verified [date]\" or \"Updated: [new info] per [source]\"\nCONFLICT → Read [pattern:...] tag. Both valid: \"Not a conflict: [reason]\". One wrong: cite source.\nMISSING → \"Source: [name] ([reference]), [date]\"\nAMBIGUOUS → clarify with KB context or external source\nPRECISION → replace vague term with specific value per source\nDUPLICATE → \"Duplicate of [doc_id], remove from here\"\n\nCan't find a source? → defer. This is the correct action.{ctx}";

pub(crate) const VARIANT_RESEARCH_BATCH_ANSWER: &str = "Process this batch using the research-first approach:\n\n1. For each document group below, call get_entity with the doc_id to read the full document\n2. Do ONE comprehensive search covering all questions for that document\n3. Answer ALL questions for that document in one answer_questions call\n4. Move to the next document group\n\nDo not answer from memory alone. Research each document thoroughly before answering any of its questions.\n\n⚠️ SCOPE: Do NOT call scan_repository or check_repository.\n\n⚠️ CONTINUATION: After answering all groups, call workflow(workflow='resolve', step=2) to get the next batch. The workflow returns continue=true when more batches remain.

Process as many batches as you can. If you must stop, report progress honestly: answered/remaining/deferred counts. Commit your work (git add/commit) and resume with workflow(workflow='resolve') — answered questions are tracked in the DB and won't reappear.{ctx}";

pub(crate) const DEFAULT_RESOLVE_APPLY_INSTRUCTION: &str = "Apply your answers by rewriting the documents directly.\n\nFor each document you answered questions about:\n1. Call get_entity to read the current content\n2. Apply your answers: insert @t[...] tags, add source footnotes, resolve conflicts, etc.\n3. Call update_document with the modified content\n\nThis gives you full control over the edits — no LLM intermediary.";

pub(crate) const DEFAULT_RESOLVE_VERIFY_INSTRUCTION: &str = "Verify your work. For each document you modified, call check_repository with doc_id and dry_run=true to check if your answers introduced new issues. If new questions appear, resolve them now.";

// --- Ingest workflow ---
pub(crate) const DEFAULT_INGEST_SEARCH_INSTRUCTION: &str = "Search factbase to see what already exists about '{topic}'. Call search_knowledge with a relevant query. Also try list_entities to browse by type.{ctx}";

pub(crate) const DEFAULT_INGEST_RESEARCH_INSTRUCTION: &str = "Research '{topic}' using your available tools. Strategies:\n- **Search**: Use available research tools for recent, authoritative information. Try specific queries like '{entity name} {fact type} {year}' rather than broad searches.\n- **Multiple sources**: Cross-reference findings across at least 2 sources before adding facts.\n- **Gather specifics**: Collect dates, numbers, names, and citations — not just summaries.\n- **Note your sources**: Track the reference (URL, document ID, publication, etc.), author, and date for every fact you find — you'll need these for footnotes.\n\nOrganize what you find by entity and section before proceeding to document creation.{ctx}";

pub(crate) const DEFAULT_INGEST_CREATE_INSTRUCTION: &str = "Create or update factbase documents with your findings. Use bulk_create_documents for multiple new entities (up to 100 at a time), create_document for a single entity, or update_document for existing ones.\n\nDocument rules:\n- Place in typed folders: people/, companies/, projects/, definitions/, etc.\n- First # Heading = document title\n- Use exact entity names matching other document titles for cross-linking\n- For acronyms or domain terms, create/update a definitions/ file\n- Never use 'Author knowledge' as a source — that's reserved for human-authored author-knowledge/ files\n- Never modify <!-- factbase:XXXXXX --> headers\n- If existing files are in the wrong folder or poorly named, feel free to rename/move them — just run scan_repository afterward\n- Entity discovery: while researching, if you discover an entity that fits the KB's allowed types (check the perspective) and is mentioned across multiple existing documents or is significant enough to warrant its own entry, create a new document for it\n- For entities external to your domain (well-known products, standards, organizations you reference but don't track in depth), add `<!-- factbase:reference -->` after the factbase ID header. These are available for linking but won't be quality-checked.\n\n⚠️ SOURCE REQUIREMENT: Every fact must have a footnote with a specific, independently verifiable source. Before writing a fact, verify it with one of your available tools and cite the specific result.\n- If you found it via web search: cite the URL\n- If you found it in a file: cite the file path\n- If you found it in email/Slack: cite the channel, thread, date, and participants\n- If you cannot verify a fact with your tools: do NOT add it. Unverified claims degrade the knowledge base.\n- NEVER cite vague sources like 'AWS documentation', 'internal docs', 'Wikipedia' (without article URL), or 'author knowledge' without specifics.\n- GOOD citations: URL, file path, book+page, email with subject+date, Slack channel+thread+date, RFC number, ISBN\n- BAD citations: 'AWS documentation', 'internal docs', 'author knowledge', 'Wikipedia' without article name/URL\n\nKeep track of all document IDs you create — you'll need them for the verify step.{fields}{format_rules}\n\n⚠️ NEXT: When done creating documents, you MUST call: workflow(workflow='ingest', step=4)";

pub(crate) const DEFAULT_INGEST_VERIFY_INSTRUCTION: &str = "Verify the quality of your new documents.\n\n1. Call check_repository with doc_ids=[list of all document IDs you created] to run quality checks on them.\n2. Review the results — questions indicate quality issues:\n   - Missing @t[] tags: add temporal context (when was this verified? what year?)\n   - Missing sources: add footnotes for any unsourced claims\n   - Ambiguous terms: clarify or create definitions/ entries\n3. Fix what you can NOW using update_document. The rest stays in the review queue for resolve.\n4. Run scan_repository to re-index after any fixes.\n\nAlso note any frequently-mentioned names that don't have their own documents — these are candidates for new entities.\n\n⚠️ NEXT: When done verifying, you MUST call: workflow(workflow='ingest', step=5)";

pub(crate) const DEFAULT_INGEST_LINKS_INSTRUCTION: &str = "Discover cross-references for your new documents.\n\n1. Call get_link_suggestions with exclude_types matching the new document types (cross-type discovery, min_similarity=0.5).\n2. Review suggestions: does the candidate genuinely relate to the source?\n3. For confirmed links, call store_links with the source_id and target_id pairs. store_links writes References: to source files AND Referenced by: to target files automatically.\n4. Bidirectional discovery: for each existing document your new docs link TO, check if that existing document's content mentions any of your new document titles. If so, store the reverse link too (existing → new) via store_links.\n5. Record: links_added, documents_modified";

// --- Enrich workflow ---
pub(crate) const DEFAULT_ENRICH_REVIEW_INSTRUCTION: &str = "Review the entity_quality list (sorted by attention_score, highest first). Reference entities are excluded — they exist for linking, not enrichment.\nPick the top 3-5 entities that need work. You will enrich them ONE AT A TIME — fully completing each before moving to the next.\n\nCall get_entity on the first entity to begin.{ctx}";

pub(crate) const DEFAULT_ENRICH_GAPS_INSTRUCTION: &str = "Score this document before researching:\n\n1. Temporal: X of Y fact lines have @t tags = Z%\n2. Sources: X of Y fact lines have [^N] citations = Z%\n3. Missing fields: [list any required by perspective]{fields}\n4. Review questions: X unanswered\n5. Gaps: list ALL areas where you could add substantive facts — sections that are thin, topics mentioned but not developed, missing context or history\n\nResearch the LOWEST coverage area first, then continue through all gaps.";

pub(crate) const DEFAULT_ENRICH_RESEARCH_INSTRUCTION: &str = "Research and update THIS document. Work through ALL gaps you identified — don't stop at 3-5.\n\nFor each gap:\n1. Search specifically: \"{entity name} {fact}\" — targeted beats broad\n2. Read the full page, not just snippets\n3. Every new fact MUST have BOTH @t[YYYY] AND [^N] citation — no exceptions\n4. Mention related KB entities by exact title in prose (enables link detection)\n\n⚠️ SOURCE REQUIREMENT: Every footnote must be specific enough that someone else could independently find the same source.\n- Before writing a fact, verify it with one of your available tools and cite the specific result (URL, file path, etc.)\n- If you cannot verify a fact with your tools: do NOT add it.\n- When enriching, check existing citations. If a citation is vague (e.g. 'AWS documentation' without a URL, 'Wikipedia' without article name), look up the actual source with your tools and update the footnote with the specific URL or reference.\n- GOOD: URL, file path, book+page, email with subject+date, Slack channel+thread+date, RFC number, ISBN\n- BAD: 'AWS documentation', 'internal docs', 'author knowledge', 'Wikipedia' without article name/URL\n\nKeep going until you've exhausted your research for this document. More well-sourced facts = better.\n\nCall update_document with the enriched content. Then verify: call check_repository with doc_id and dry_run=true.\n\nRecord: facts added, sources added, @t tags added, issues from verify.\n\n⚠️ REPEAT for the next document:\n1. Call get_entity on the next entity from your list\n2. Score it (same as step 2)\n3. Research + update + verify (same as this step)\nContinue until all documents are done.\n\nRules:\n- Preserve ALL existing content — add, don't replace\n- Resolve review questions when your research provides answers\n- Create documents for significant missing entities{ctx}{format_rules}";

pub(crate) const DEFAULT_ENRICH_VERIFY_INSTRUCTION: &str = "Report totals across all documents enriched:\n\n| Document | +Facts | +Sources | +@t | Issues |\n|----------|--------|----------|-----|--------|\n| ... | ... | ... | ... | ... |\n\nTotals: X facts, X sources, X @t tags added. X questions resolved. X new issues.\nAssessment: biggest improvement, remaining gaps, recommended next step.";

// --- Improve workflow ---
pub(crate) const DEFAULT_IMPROVE_CLEANUP_INSTRUCTION: &str = "Read the document and fix any issues{doc_hint}. Call get_entity with the doc_id to read its full content.\n\nCheck for and fix:\n- Corruption artifacts (malformed review queue sections, broken markdown)\n- Duplicate entries (same fact stated multiple times)\n- Formatting inconsistencies (inconsistent heading levels, missing blank lines)\n- Orphaned footnote references or definitions\n\nIf issues are found, call update_document to fix them. If the document looks clean, move to the next step.{ctx}";

pub(crate) const DEFAULT_IMPROVE_RESOLVE_INSTRUCTION: &str = "Resolve outstanding review questions{doc_hint}. Call get_review_queue with doc_id to see pending questions.\n\nFor each unanswered question:\n- stale: Source is older than {stale} days. Search for current info\n- missing: Find a source citation\n- conflict: Check the [pattern:...] tag — parallel_overlap, same_entity_transition, date_imprecision are often not real conflicts\n- temporal: The fact line is MISSING an @t[...] temporal tag — that is what this question means. Your answer MUST include the @t[...] tag to add, plus a source. Answer: '@t[YYYY] per [source] ([URL]); verified [YYYY-MM-DD]' or '@t[YYYY..YYYY] per [source]' for ranges. Just citing a source in prose does NOT resolve it — the @t[...] tag must appear in your answer. Use @t[?] only when no date is findable. Every datable fact gets a tag regardless of domain or era. Never answer with bare dismissals like 'static fact', 'well-known', or 'historical constant' — these provide no audit trail\n- ambiguous: Clarify the term or create a definitions document\n- precision: Replace vague qualifier with a specific value or measurement, e.g. 'heavy losses' → '~500 casualties'\n- duplicate: Identify the canonical entry\n\nCall answer_questions with your answers. If you can't resolve a question, defer it with a note about what you tried.\n\nAfter answering, apply your changes directly: call get_entity to read the document, apply the edits (insert @t tags, add footnotes, etc.), then call update_document with the modified content.{ctx}";

pub(crate) const DEFAULT_IMPROVE_ENRICH_INSTRUCTION: &str = "Enrich the document with new information{doc_hint}. Call get_entity to read the current content (it may have changed from earlier steps).\n\nIdentify gaps:\n- Dynamic facts missing temporal tags\n- Facts without source citations\n- Sparse sections that could be expanded\n- Missing standard fields for the document type\n- Weak identification: if the title is an alias, abbreviation, or partial label and a fuller canonical name exists, update the title\n- Poor file organization: if the file is in the wrong folder or has an unclear name, rename/move it with file tools{fields}\n\nResearch the gaps using your available tools:\n- Use available research tools for current data on each gap — use specific queries per entity/fact\n- Read full pages when snippets look relevant\n- Cross-reference important facts across multiple sources\n\nThen call update_document to add findings.\n\nRules:\n- Preserve all existing content — add to it, don't replace\n- Always add temporal tags and source footnotes on new facts\n- Don't add speculative information — only add what you can source\n- Use @t[?] for facts you found but can't date precisely\n- If you rename or move any files, run scan_repository afterward to re-index{ctx}";

pub(crate) const DEFAULT_IMPROVE_CHECK_INSTRUCTION: &str = "Verify the document quality{doc_hint}. Call check_repository with doc_id and dry_run=true to check for any remaining or newly introduced issues.\n\nReport what you find:\n- How many questions remain vs. how many were resolved\n- Any new issues introduced during enrichment\n- Overall document health assessment{compare_note}";

/// Start a guided workflow.
pub fn workflow(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let workflow = get_str_arg_required(args, "workflow")?;
    let step = get_u64_arg(args, "step", 1) as usize;
    let repo_resolved = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let perspective = load_perspective(db, repo_resolved.as_deref());
    let mut wf_config = crate::Config::load(None)
        .unwrap_or_default()
        .workflows;

    // Merge per-repo .factbase/prompts.yaml overrides (highest priority)
    if let Some(repo_path) = resolve_repo_path(db, repo_resolved.as_deref()) {
        if let Some(repo_prompts) = WorkflowsConfig::load_repo_prompts(&repo_path) {
            wf_config.merge(&repo_prompts);
        }
    }

    match workflow.as_str() {
        "update" => Ok(update_step(step, args, &perspective, &wf_config)),
        "resolve" => {
            let deferred = db
                .count_deferred_questions(repo_resolved.as_deref())
                .unwrap_or(0);
            Ok(resolve_step(step, args, &perspective, deferred, db, &wf_config))
        }
        "ingest" => Ok(ingest_step(step, args, &perspective, &wf_config)),
        "enrich" => Ok(enrich_step(step, args, &perspective, db, repo_resolved.as_deref(), &wf_config)),
        "improve" => {
            let doc_id = get_str_arg(args, "doc_id");
            let skip = parse_skip_steps(args);
            Ok(improve_step(step, doc_id, &perspective, &skip, db, &wf_config))
        }
        "setup" => Ok(setup_step(step, args, &wf_config)),
        "bootstrap" => Ok(bootstrap(args)?),
        "list" => Ok(serde_json::json!({
            "workflows": [
                {"name": "bootstrap", "description": "Design a domain-specific knowledge base structure. Provide domain='mycology' (or any domain) and get instructions for generating suggested document types, folder structure, templates, and perspective. Use this BEFORE setup when starting a new KB in an unfamiliar domain."},
                {"name": "setup", "description": "Set up a new factbase repository from scratch: initialize, configure perspective, create first documents, scan, and verify"},
                {"name": "update", "description": "Scan, check quality, analyze organization (merge/split/misplaced/duplicates), and report what needs attention"},
                {"name": "resolve", "description": "Answer existing review queue questions using external sources. Does NOT scan or check — use 'update' for that. Optionally pass question_type to filter by type (e.g. 'stale' or 'temporal,ambiguous'). Response includes type_distribution and recommended_order (fewest questions first for quick wins). Process types in recommended_order for best results."},
                {"name": "ingest", "description": "Research a topic or process source data into factbase documents. Handles search, research, bulk creation, quality checks, and link discovery."},
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

/// Run the bootstrap workflow: return instructions for the agent to generate domain-specific KB structure.
pub fn bootstrap(args: &Value) -> Result<Value, FactbaseError> {
    let domain = get_str_arg_required(args, "domain")?;
    let entity_types = get_str_arg(args, "entity_types");

    let prompts = crate::Config::load(None)
        .unwrap_or_default()
        .prompts;
    let prompt = build_bootstrap_prompt(&domain, entity_types, &prompts);

    Ok(serde_json::json!({
        "workflow": "bootstrap",
        "domain": domain,
        "instruction": prompt,
        "expected_format": "Generate a JSON object with these 4 fields: document_types (array of {name, description}), folder_structure (array of paths), templates (object mapping type to markdown template), perspective ({focus, allowed_types}). Then proceed to setup.",
        "next_steps": [
            "Generate the JSON suggestions based on the instruction above.",
            "Use the suggestions as reference when configuring perspective.yaml (YAML format, not markdown) and creating documents.",
            "⚠️ REQUIRED NEXT: Call workflow(workflow='setup', step=1) to begin the guided setup process.",
            "Do NOT skip the setup workflow — it provides step-by-step guidance including format rules for temporal tags and source footnotes."
        ],
        "note": "These are suggestions — adapt them to your needs. The templates and folder structure can be modified at any time.",
        "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=1) to begin guided setup"
    }))
}

fn update_step(step: usize, args: &Value, perspective: &Option<Perspective>, wf: &WorkflowsConfig) -> Value {
    let ctx = perspective_context(perspective);
    let do_cv = args.get("cross_validate").and_then(Value::as_bool).unwrap_or(false);
    // Steps: 1=scan, 2=check, 3=links, 4=cross_validate (if enabled), 5=organize, 6=summary
    let total = if do_cv { 6 } else { 5 };
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
            "when_done": "Call workflow with workflow='update', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "update",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "update.links", DEFAULT_UPDATE_LINKS_INSTRUCTION, &[]),
            "next_tool": "get_link_suggestions",
            "when_done": "Call workflow with workflow='update', step=4"
        }),
        4 => {
            if do_cv {
                serde_json::json!({
                    "workflow": "update",
                    "step": 4, "total_steps": total,
                    "instruction": resolve(wf, "update.cross_validate", DEFAULT_UPDATE_CROSS_VALIDATE_INSTRUCTION, &[]),
                    "next_tool": "get_fact_pairs",
                    "when_done": "Call workflow with workflow='update', step=5"
                })
            } else {
                // Skip cross-validation, advance to organize
                serde_json::json!({
                    "workflow": "update",
                    "step": 4, "total_steps": total,
                    "instruction": resolve(wf, "update.organize", DEFAULT_UPDATE_ORGANIZE_INSTRUCTION, &[]),
                    "next_tool": "organize_analyze",
                    "when_done": format!("Call workflow with workflow='update', step={}", total)
                })
            }
        }
        5 => {
            if do_cv {
                serde_json::json!({
                    "workflow": "update",
                    "step": 5, "total_steps": total,
                    "instruction": resolve(wf, "update.organize", DEFAULT_UPDATE_ORGANIZE_INSTRUCTION, &[]),
                    "next_tool": "organize_analyze",
                    "when_done": "Call workflow with workflow='update', step=6"
                })
            } else {
                serde_json::json!({
                    "workflow": "update",
                    "step": 5, "total_steps": total,
                    "instruction": resolve(wf, "update.summary", DEFAULT_UPDATE_SUMMARY_INSTRUCTION, &[]),
                    "complete": true
                })
            }
        }
        6 if do_cv => serde_json::json!({
            "workflow": "update",
            "step": 6, "total_steps": total,
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
const RESOLVE_BATCH_SIZE: usize = 50;

/// Minimum group size to surface as a repetitive pattern.
const PATTERN_MIN_COUNT: usize = 4;

/// Normalize a question description by replacing variable parts with placeholders.
/// This groups questions that follow the same template (e.g., same weak-source wording
/// across hundreds of documents differing only in footnote number and source text).
fn normalize_question_text(desc: &str) -> String {
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
fn detect_question_patterns(all_questions: &[Value], batch: &[Value]) -> Vec<Value> {
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
fn question_type_priority(qt: &QuestionType) -> u8 {
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
fn type_evidence_guidance(qt: &QuestionType) -> &'static str {
    match qt {
        QuestionType::Stale => "Search for the claim + current year. Cite a URL confirming or updating it. Wikipedia is acceptable for well-established facts.",
        QuestionType::Temporal => "Search for the specific event date. Cite a URL with the date. Format: @t[YYYY-MM-DD] per [source] ([URL]); verified YYYY-MM-DD",
        QuestionType::Ambiguous => "Check the KB first (get_entity, read other docs). If the term is defined elsewhere in the KB, cite that doc. Only search externally if KB has no answer.",
        QuestionType::Conflict => "Read BOTH referenced documents (get_entity). Search for the specific claim in each. Compare sources by recency and authority. If genuinely unresolvable, defer with analysis of both sides.",
        QuestionType::Precision => "Search for a quantitative replacement. If no specific number exists in sources, defer — do not guess.",
        QuestionType::Missing => "Find a source citation for this unsourced fact. Search for the specific claim and cite a URL.",
        QuestionType::Duplicate => "Identify the canonical entry by reading both documents. Cite which one should be kept.",
        QuestionType::Corruption => "Read the document to identify the corruption. Describe what needs to be fixed.",
        QuestionType::WeakSource => "Find the specific source using your available tools. Update the footnote with a specific, independently verifiable reference: URL, document path, page number, ISBN, RFC, channel/thread ID, etc. If you cannot find the source, change the footnote to '[^N]: UNVERIFIED — original claim: <original text>'. Do not invent specific-looking citations.",
    }
}

fn resolve_step(
    step: usize,
    args: &Value,
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
        1 => {
            let type_dist = compute_type_distribution(db);
            let total_unanswered: usize = type_dist.iter().map(|(_, c)| c).sum();
            let type_dist_json: Value = type_dist
                .iter()
                .map(|(qt, count)| serde_json::json!({"type": qt.to_string(), "count": count}))
                .collect::<Vec<_>>()
                .into();
            let rec_order = recommended_resolve_order(&type_dist);
            let suggested = if let Some(first) = rec_order.first() {
                serde_json::json!({"question_type": first})
            } else {
                serde_json::json!({})
            };
            serde_json::json!({
                "workflow": "resolve",
                "step": 1, "total_steps": total,
                "instruction": resolve(wf, "resolve.queue", DEFAULT_RESOLVE_QUEUE_INSTRUCTION, &[("ctx", &ctx), ("deferred_note", &deferred_note)]),
                "next_tool": "workflow",
                "suggested_args": suggested,
                "policy": {"stale_days": stale},
                "deferred_count": deferred,
                "total_unanswered": total_unanswered,
                "type_distribution": type_dist_json,
                "recommended_order": rec_order,
                "when_done": "Call workflow with workflow='resolve', step=2, question_type=<next_type>"
            })
        },
        2 => resolve_step2_batch(args, perspective, db, wf),
        3 => serde_json::json!({
            "workflow": "resolve",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "resolve.apply", DEFAULT_RESOLVE_APPLY_INSTRUCTION, &[]),
            "next_tool": "get_entity",
            "when_done": "Call workflow with workflow='resolve', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "resolve",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "resolve.verify", DEFAULT_RESOLVE_VERIFY_INSTRUCTION, &[]),
            "next_tool": "check_repository",
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

/// Load all documents with their disk content (preferred) or DB content (fallback),
/// filtered to only those that have a review queue section.
///
/// The `has_review_queue` DB flag can be stale when files are edited externally or
/// when check_repository writes questions to disk but the DB update is lost.
/// Reading from disk ensures the resolve workflow sees the filesystem truth.
fn load_review_docs_from_disk(db: &Database) -> Vec<Document> {
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
fn compute_type_distribution(db: &Database) -> Vec<(QuestionType, usize)> {
    let docs = load_review_docs_from_disk(db);
    let (counts, _) = crate::mcp::tools::review::count_question_types(&docs);
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted
}

/// Compute recommended type processing order: fewest questions first (quick wins),
/// with difficulty as tiebreaker for equal counts.
fn recommended_resolve_order(dist: &[(QuestionType, usize)]) -> Vec<String> {
    let mut with_questions: Vec<_> = dist.iter().filter(|(_, c)| *c > 0).collect();
    with_questions.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| question_type_priority(&a.0).cmp(&question_type_priority(&b.0)))
    });
    with_questions.iter().map(|(qt, _)| qt.to_string()).collect()
}

/// Load glossary terms from all repositories.
fn load_all_glossary_terms(db: &Database) -> HashSet<String> {
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
fn auto_dismiss_question(db: &Database, doc_id: &str, question_index: usize) -> Result<(), FactbaseError> {
    let doc = db.require_document(doc_id)?;
    let marker = "<!-- factbase:review -->";
    let Some(marker_pos) = doc.content.find(marker) else {
        return Ok(());
    };
    let (before, after) = doc.content.split_at(marker_pos);
    let queue_content = &after[marker.len()..];

    // Mark the question as answered with a glossary note
    let answer = "Defined in glossary — auto-resolved";
    let Some(modified) = super::review::answer::modify_question_in_queue(queue_content, question_index, answer, false) else {
        return Ok(());
    };
    let new_content = format!("{before}{marker}{modified}");
    let new_hash = crate::processor::content_hash(&new_content);
    db.update_document_content(doc_id, &new_content, &new_hash)?;

    // Also write to disk if possible
    if let Ok(file_path) = super::helpers::resolve_doc_path(db, &doc) {
        let _ = std::fs::write(&file_path, &new_content);
    }
    Ok(())
}

/// Build the step 2 response with inline question batching.
///
/// Reads the actual review queue from the DB, collects unanswered questions,
/// sorts them (grouped by document, then by type priority), and returns the
/// next batch. The agent just answers what it sees and calls step 2 again.
/// Build directive continuation guidance based on queue size and type distribution.
fn build_continuation_guidance(
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

    // Directive language scaled by remaining count
    if remaining > 500 {
        let batches_left = remaining.div_ceil(batch_size);
        let filter_hint = if type_filter.is_empty() {
            String::new()
        } else {
            let f = type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
            format!(" with question_type={f}")
        };
        parts.push(format!(
            "⚡ {remaining} questions remain. Keep calling step=2{filter_hint}. You have cleared {resolved_so_far} so far — maintain momentum. ~{batches_left} batches remaining at {batch_size}/batch. Process as many as your context allows."
        ));
    } else if remaining > 100 {
        let filter_hint = if type_filter.is_empty() {
            String::new()
        } else {
            let f = type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
            format!(" with question_type={f}")
        };
        parts.push(format!(
            "⚡ {remaining} questions remain. Keep calling step=2{filter_hint}. You have cleared {resolved_so_far} so far — maintain momentum."
        ));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn resolve_step2_batch(
    args: &Value,
    perspective: &Option<Perspective>,
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
    let ctx = perspective_context(perspective);
    let stale = stale_days(perspective);
    let total_steps = 4;
    let variant = get_str_arg(args, "variant")
        .unwrap_or_else(|| wf.resolve_variant.as_deref().unwrap_or("baseline"));

    // Track where the variant came from for A/B testing comparison
    let variant_source = if get_str_arg(args, "variant").is_some() {
        "arg"
    } else if wf.resolve_variant.is_some() {
        "config"
    } else {
        "default"
    };

    // Optional question_type filter (comma-separated)
    let type_filter: Vec<QuestionType> = get_str_arg(args, "question_type")
        .map(|s| {
            s.split(',')
                .filter_map(|t| t.trim().parse::<QuestionType>().ok())
                .collect()
        })
        .unwrap_or_default();

    // Collect all questions from the review queue (prefer disk content)
    let docs = load_review_docs_from_disk(db);
    let mut unanswered: Vec<Value> = Vec::new();
    let mut resolved_verified: usize = 0;
    let mut resolved_believed: usize = 0;
    let mut resolved_deferred: usize = 0;
    let mut type_distribution: HashMap<QuestionType, usize> = HashMap::new();

    // Load glossary terms to auto-dismiss acronym questions already covered
    let glossary_terms = load_all_glossary_terms(db);
    let mut glossary_auto_resolved: usize = 0;

    for doc in &docs {
        if let Some(questions) = parse_review_queue(&doc.content) {
            for (idx, q) in questions.iter().enumerate() {
                if q.answered {
                    resolved_verified += 1;
                } else if q.is_believed() {
                    resolved_believed += 1;
                } else if q.is_deferred() {
                    resolved_deferred += 1;
                } else {
                    // Auto-dismiss ambiguous acronym questions covered by glossary
                    if q.question_type == QuestionType::Ambiguous {
                        if let Some(acronym) = extract_acronym_from_question(&q.description) {
                            if glossary_terms.iter().any(|t| t.eq_ignore_ascii_case(&acronym)) {
                                glossary_auto_resolved += 1;
                                // Auto-answer in DB+file so the question doesn't reappear
                                let _ = auto_dismiss_question(db, &doc.id, idx);
                                continue;
                            }
                        }
                    }

                    // Count type distribution (before type filter)
                    *type_distribution.entry(q.question_type).or_insert(0) += 1;

                    // Apply type filter
                    if !type_filter.is_empty() && !type_filter.contains(&q.question_type) {
                        continue;
                    }
                    let mut qjson = format_question_json(q, Some((&doc.id, &doc.title)));
                    if let Some(obj) = qjson.as_object_mut() {
                        obj.insert("question_index".to_string(), serde_json::json!(idx));
                        // Stash sort keys (removed before sending)
                        obj.insert("_doc_id".to_string(), Value::String(doc.id.clone()));
                        obj.insert(
                            "_type_priority".to_string(),
                            serde_json::json!(question_type_priority(&q.question_type)),
                        );
                        // Variant A: add per-question evidence guidance
                        if variant == "type_evidence" {
                            obj.insert(
                                "evidence_guidance".to_string(),
                                Value::String(type_evidence_guidance(&q.question_type).to_string()),
                            );
                        }
                    }
                    unanswered.push(qjson);
                }
            }
        }
    }

    let resolved_so_far = resolved_verified + resolved_believed + resolved_deferred + glossary_auto_resolved;

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

    // Build type distribution summary (sorted by count descending)
    let mut type_dist_vec: Vec<_> = type_distribution.iter().collect();
    type_dist_vec.sort_by(|a, b| b.1.cmp(a.1));
    let type_dist_json: Value = type_dist_vec
        .iter()
        .map(|(qt, count)| serde_json::json!({"type": qt.to_string(), "count": count}))
        .collect::<Vec<_>>()
        .into();

    // Include active filter in response
    let active_filter: Value = if type_filter.is_empty() {
        Value::Null
    } else {
        type_filter.iter().map(|t| t.to_string()).collect::<Vec<_>>().into()
    };

    // If no unanswered questions remain, advance to step 3
    if remaining == 0 {
        let mut result = serde_json::json!({
            "workflow": "resolve",
            "step": 2, "total_steps": total_steps,
            "instruction": "✅ All review questions have been resolved. No more batches remain. You may now proceed to step 3 to apply your answers.",
            "all_resolved": true,
            "variant": variant,
            "variant_source": variant_source,
            "type_filter": active_filter,
            "type_distribution": type_dist_json,
            "continue": false,
            "batch": {
                "questions": [],
                "batch_number": 0,
                "total_batches_estimate": 0,
                "resolved_so_far": resolved_so_far,
                "resolved_verified": resolved_verified,
                "resolved_believed": resolved_believed,
                "resolved_deferred": resolved_deferred,
                "questions_remaining": 0
            },
            "when_done": "Call workflow with workflow='resolve', step=3"
        });
        if glossary_auto_resolved > 0 {
            result["batch"]["glossary_auto_resolved"] = serde_json::json!(glossary_auto_resolved);
        }
        return result;
    }

    let batch_size = RESOLVE_BATCH_SIZE;
    let total_questions = resolved_so_far + remaining;
    let batch_number = (resolved_so_far / batch_size) + 1;
    let total_batches_estimate = total_questions.div_ceil(batch_size);
    let batch: Vec<Value> = unanswered[..batch_size.min(unanswered.len())].to_vec();
    let patterns = detect_question_patterns(&unanswered, &batch);
    drop(unanswered);

    // Select instruction based on variant
    let (answer_default, intro_default) = match variant {
        "type_evidence" => (VARIANT_TYPE_EVIDENCE_ANSWER, VARIANT_TYPE_EVIDENCE_INTRO),
        "research_batch" => (VARIANT_RESEARCH_BATCH_ANSWER, VARIANT_RESEARCH_BATCH_INTRO),
        _ => (DEFAULT_RESOLVE_ANSWER_INSTRUCTION, DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION),
    };

    let instruction = resolve(
        wf,
        "resolve.answer",
        answer_default,
        &[("stale", &stale.to_string()), ("ctx", &ctx)],
    );

    let is_first_batch = resolved_so_far == 0;

    // Variant B: group questions by document for the agent
    let batch_value = if variant == "research_batch" {
        let mut doc_groups: Vec<Value> = Vec::new();
        let mut current_doc_id = String::new();
        let mut current_questions: Vec<Value> = Vec::new();
        let mut current_doc_title = String::new();

        for q in &batch {
            let did = q["doc_id"].as_str().unwrap_or("").to_string();
            if did != current_doc_id && !current_doc_id.is_empty() {
                doc_groups.push(serde_json::json!({
                    "doc_id": current_doc_id,
                    "doc_title": current_doc_title,
                    "questions": current_questions,
                }));
                current_questions = Vec::new();
            }
            current_doc_id = did;
            current_doc_title = q["doc_title"].as_str().unwrap_or("").to_string();
            current_questions.push(q.clone());
        }
        if !current_doc_id.is_empty() {
            doc_groups.push(serde_json::json!({
                "doc_id": current_doc_id,
                "doc_title": current_doc_title,
                "questions": current_questions,
            }));
        }

        serde_json::json!({
            "document_groups": doc_groups,
            "batch_number": batch_number,
            "total_batches_estimate": total_batches_estimate,
            "resolved_so_far": resolved_so_far,
            "resolved_verified": resolved_verified,
            "resolved_believed": resolved_believed,
            "resolved_deferred": resolved_deferred,
            "questions_remaining": remaining
        })
    } else {
        serde_json::json!({
            "questions": batch,
            "batch_number": batch_number,
            "total_batches_estimate": total_batches_estimate,
            "resolved_so_far": resolved_so_far,
            "resolved_verified": resolved_verified,
            "resolved_believed": resolved_believed,
            "resolved_deferred": resolved_deferred,
            "questions_remaining": remaining
        })
    };

    let pct = if total_questions > 0 { (resolved_so_far * 100) / total_questions } else { 0 };

    let mut result = serde_json::json!({
        "workflow": "resolve",
        "step": 2, "total_steps": total_steps,
        "instruction": instruction,
        "next_tool": "answer_questions",
        "variant": variant,
        "variant_source": variant_source,
        "type_filter": active_filter,
        "type_distribution": type_dist_json,
        "continue": true,
        "conflict_patterns": {
            "parallel_overlap": "Two overlapping facts about different entities that may legitimately coexist. Answer: 'Not a conflict: parallel overlap'.",
            "same_entity_transition": "Two overlapping facts about the same entity where one likely supersedes the other. Adjust the earlier entry's end date.",
            "date_imprecision": "Small overlap relative to date ranges — likely data-source imprecision. Adjust the boundary date.",
            "unknown": "No recognized pattern — investigate which fact is current."
        },
        "batch": batch_value,
        "progress": format!("Batch {batch_number}: {resolved_so_far} answered, {remaining} remaining"),
        "completion_gate": format!("Resolved {resolved_so_far} of {total_questions} ({pct}%). {remaining} questions remain. Call workflow with workflow='resolve', step=2 for the next batch. If you must stop, commit progress and resume with workflow(workflow='resolve') — the DB tracks answered questions."),
        "when_done": "Call workflow with workflow='resolve', step=2"
    });

    if is_first_batch {
        let mut intro = resolve(
            wf,
            "resolve.answer_intro",
            intro_default,
            &[("stale", &stale.to_string()), ("ctx", &ctx)],
        );
        let fanout_types: Vec<(String, usize)> = type_distribution
            .iter()
            .map(|(qt, c)| (qt.to_string(), *c))
            .collect();
        intro.push_str(&subagent_fanout_hint(total_questions, &fanout_types));
        result
            .as_object_mut()
            .unwrap()
            .insert("intro".to_string(), Value::String(intro));
    }

    if glossary_auto_resolved > 0 {
        result
            .as_object_mut()
            .unwrap()
            .insert("glossary_auto_resolved".to_string(), serde_json::json!(glossary_auto_resolved));
    }

    if let Some(guidance) = build_continuation_guidance(
        remaining,
        resolved_so_far,
        batch_size,
        &type_distribution,
        &type_filter,
    ) {
        result
            .as_object_mut()
            .unwrap()
            .insert("continuation_guidance".to_string(), Value::String(guidance));
    }

    if !patterns.is_empty() {
        result
            .as_object_mut()
            .unwrap()
            .insert("patterns_detected".to_string(), Value::Array(patterns));
    }

    result
}

fn ingest_step(step: usize, args: &Value, perspective: &Option<Perspective>, wf: &WorkflowsConfig) -> Value {
    let topic = get_str_arg(args, "topic").unwrap_or("the requested topic");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 5;
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
            "next_tool": "bulk_create_documents",
            "when_done": "Call workflow with workflow='ingest', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "ingest",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "ingest.verify", DEFAULT_INGEST_VERIFY_INSTRUCTION, &[]),
            "next_tool": "check_repository",
            "when_done": "Call workflow with workflow='ingest', step=5"
        }),
        5 => serde_json::json!({
            "workflow": "ingest",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "ingest.links", DEFAULT_INGEST_LINKS_INSTRUCTION, &[]),
            "next_tool": "get_link_suggestions",
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

fn enrich_step(step: usize, args: &Value, perspective: &Option<Perspective>, db: &Database, repo: Option<&str>, wf: &WorkflowsConfig) -> Value {
    let doc_type = get_str_arg(args, "doc_type").unwrap_or("all types");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 4;
    match step {
        1 => {
            let type_filter = if doc_type != "all types" { Some(doc_type) } else { None };
            let quality = bulk_quality(db, type_filter, repo);
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
            "next_tool": "check_repository",
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
                "next_tool": "check_repository",
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
                glossary_types: None,
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
    fn test_ingest_create_has_required_next() {
        let step = ingest_step(3, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("NEXT:"), "create step should have REQUIRED NEXT routing");
        assert!(instruction.contains("workflow(workflow='ingest', step=4)"), "should route to step 4");
    }

    #[test]
    fn test_ingest_create_recommends_bulk() {
        let step = ingest_step(3, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("bulk_create_documents"), "create step should recommend bulk_create_documents");
        assert_eq!(step["next_tool"].as_str().unwrap(), "bulk_create_documents", "next_tool should be bulk_create_documents");
    }

    #[test]
    fn test_ingest_verify_no_dry_run() {
        let step = ingest_step(4, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(!instruction.contains("dry_run"), "verify step should not mention dry_run");
        assert!(step.get("suggested_args").is_none(), "verify step should not have suggested_args with dry_run");
        assert!(instruction.contains("doc_ids"), "verify step should tell agent to use doc_ids");
    }

    #[test]
    fn test_ingest_verify_has_required_next() {
        let step = ingest_step(4, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("NEXT:"), "verify step should have REQUIRED NEXT routing");
        assert!(instruction.contains("workflow(workflow='ingest', step=5)"), "should route to step 5");
    }

    #[test]
    fn test_enrich_includes_required_fields() {
        let p = mock_perspective();
        let (db, _tmp) = test_db();
        let step = enrich_step(2, &serde_json::json!({}), &p, &db, None, &wf());
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
    }

    #[test]
    fn test_enrich_step2_mentions_scoring() {
        let (db, _tmp) = test_db();
        let step = enrich_step(2, &serde_json::json!({}), &None, &db, None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("Score this document"));
        assert!(instruction.contains("Temporal"));
    }

    #[test]
    fn test_ingest_create_has_source_requirement() {
        let step = ingest_step(3, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("SOURCE REQUIREMENT"), "ingest create must have source requirement");
        assert!(instruction.contains("independently verifiable"), "must mention independently verifiable");
        assert!(instruction.contains("GOOD citations"), "must list good citation examples");
        assert!(instruction.contains("BAD citations"), "must list bad citation examples");
    }

    #[test]
    fn test_enrich_research_has_source_requirement() {
        let (db, _tmp) = test_db();
        let step = enrich_step(3, &serde_json::json!({}), &None, &db, None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("SOURCE REQUIREMENT"), "enrich research must have source requirement");
        assert!(instruction.contains("vague"), "must mention checking existing vague citations");
    }

    #[test]
    fn test_resolve_intro_has_weak_source_guidance() {
        let intro = DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION;
        assert!(intro.contains("WEAK-SOURCE"), "resolve intro must have WEAK-SOURCE guidance");
        assert!(intro.contains("UNVERIFIED"), "must mention UNVERIFIED fallback");
    }

    #[test]
    fn test_type_evidence_weak_source_mentions_unverified() {
        let guidance = type_evidence_guidance(&QuestionType::WeakSource);
        assert!(guidance.contains("UNVERIFIED"), "weak-source guidance must mention UNVERIFIED fallback");
        assert!(guidance.contains("Do not invent"), "must warn against inventing citations");
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
    fn test_resolve_step1_includes_type_distribution() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:td001 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source (line 4)\n- [ ] `@q[temporal]` Missing date (line 4)\n- [ ] `@q[stale]` Another old source (line 4)\n";
        insert_test_doc(&db, "td001", content);
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let dist = step["type_distribution"].as_array().unwrap();
        assert!(!dist.is_empty(), "type_distribution should be populated");
        assert_eq!(step["total_unanswered"], 3);
        // stale should have count 2
        let stale_entry = dist.iter().find(|e| e["type"] == "stale").unwrap();
        assert_eq!(stale_entry["count"], 2);
        let temporal_entry = dist.iter().find(|e| e["type"] == "temporal").unwrap();
        assert_eq!(temporal_entry["count"], 1);
    }

    #[test]
    fn test_resolve_step1_empty_queue_type_distribution() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let dist = step["type_distribution"].as_array().unwrap();
        assert!(dist.is_empty());
        assert_eq!(step["total_unanswered"], 0);
    }

    #[test]
    fn test_resolve_step1_recommended_order_fewest_first() {
        let (db, _tmp) = test_db();
        // 2 stale + 1 temporal → temporal (1) should come before stale (2)
        let content = "<!-- factbase:ro001 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source (line 4)\n- [ ] `@q[temporal]` Missing date (line 4)\n- [ ] `@q[stale]` Another old source (line 4)\n";
        insert_test_doc(&db, "ro001", content);
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let order = step["recommended_order"].as_array().unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0], "temporal"); // 1 question
        assert_eq!(order[1], "stale");    // 2 questions
    }

    #[test]
    fn test_resolve_step1_recommended_order_empty_queue() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let order = step["recommended_order"].as_array().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_resolve_step1_recommended_order_difficulty_tiebreaker() {
        let (db, _tmp) = test_db();
        // 1 temporal + 1 ambiguous → same count, temporal has lower difficulty priority
        let content = "<!-- factbase:ro002 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Missing date (line 4)\n- [ ] `@q[ambiguous]` Unclear meaning (line 4)\n";
        insert_test_doc(&db, "ro002", content);
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let order = step["recommended_order"].as_array().unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0], "temporal");  // priority 0
        assert_eq!(order[1], "ambiguous"); // priority 4
    }

    #[test]
    fn test_resolve_step1_suggested_args_has_first_type() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:ro003 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old (line 4)\n- [ ] `@q[stale]` Old2 (line 4)\n- [ ] `@q[temporal]` Missing (line 4)\n";
        insert_test_doc(&db, "ro003", content);
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        let suggested = &step["suggested_args"];
        assert_eq!(suggested["question_type"], "temporal");
    }

    #[test]
    fn test_resolve_step1_next_tool_is_workflow() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["next_tool"], "workflow");
    }

    #[test]
    fn test_recommended_resolve_order_excludes_zero_counts() {
        // Unit test for the ordering function directly
        use crate::QuestionType;
        let dist = vec![
            (QuestionType::Stale, 5),
            (QuestionType::Temporal, 0),
            (QuestionType::Ambiguous, 3),
        ];
        let order = recommended_resolve_order(&dist);
        assert_eq!(order, vec!["ambiguous", "stale"]);
    }

    #[test]
    fn test_resolve_step2_intro_includes_fanout_hint() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:fan001 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source (line 4)\n";
        insert_test_doc(&db, "fan001", content);
        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf());
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("PARALLEL DISPATCH"), "intro should contain fan-out hint");
        assert!(intro.contains("question_type="), "hint should show question_type param");
        assert!(!intro.contains("optional"), "hint should not say optional");
    }

    #[test]
    fn test_subagent_fanout_hint_large_queue_mandatory() {
        let types = vec![("stale".to_string(), 150), ("temporal".to_string(), 80)];
        let hint = subagent_fanout_hint(230, &types);
        assert!(hint.contains("MANDATORY"), "large queue should use MANDATORY language");
        assert!(hint.contains("DO IT NOW"), "large queue should say DO IT NOW");
        assert!(hint.contains("question_type='stale'"), "should include actual types");
        assert!(hint.contains("question_type='temporal'"), "should include actual types");
    }

    #[test]
    fn test_subagent_fanout_hint_small_queue_not_mandatory() {
        let types = vec![("stale".to_string(), 5)];
        let hint = subagent_fanout_hint(5, &types);
        assert!(!hint.contains("MANDATORY"), "small queue should not use MANDATORY");
        assert!(hint.contains("PARALLEL DISPATCH"), "should still suggest parallelism");
    }

    #[test]
    fn test_resolve_answer_instruction_action_framing() {
        assert!(DEFAULT_RESOLVE_ANSWER_INSTRUCTION.contains("ANSWER questions, not analyze"));
        assert!(DEFAULT_RESOLVE_ANSWER_INSTRUCTION.contains("not answer_questions is reducing"));
        assert!(DEFAULT_RESOLVE_ANSWER_INSTRUCTION.contains("Minimize research calls"));
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
        assert_eq!(step["next_tool"], "check_repository");
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
        let step = enrich_step(1, &serde_json::json!({}), &None, &db, None, &wf());
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
        let step = enrich_step(1, &serde_json::json!({}), &None, &db, None, &wf());
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
        assert!(instruction.contains("scan_repository"), "should call scan_repository");
    }

    #[test]
    fn test_enrich_step3_mentions_link_detection() {
        let (db, _tmp) = test_db();
        let step = enrich_step(3, &serde_json::json!({}), &None, &db, None, &wf());
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
        let enrich = enrich_step(3, &serde_json::json!({}), &None, &db, None, &wf());

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
    fn test_bootstrap_returns_instruction() {
        let args = serde_json::json!({"domain": "mycology", "path": "/tmp/mushrooms"});
        let result = bootstrap(&args).unwrap();

        assert_eq!(result["workflow"], "bootstrap");
        assert_eq!(result["domain"], "mycology");
        assert!(result["instruction"].is_string());
        let instruction = result["instruction"].as_str().unwrap();
        assert!(instruction.contains("mycology"), "instruction should contain domain");
        assert!(instruction.contains("document_types"), "instruction should describe expected format");
        assert!(result["next_steps"].is_array());
        let steps = result["next_steps"].as_array().unwrap();
        let all_steps = steps.iter().map(|s| s.as_str().unwrap_or("")).collect::<Vec<_>>().join(" ");
        assert!(all_steps.contains("setup"), "next_steps should route to setup workflow");
    }

    #[test]
    fn test_bootstrap_requires_domain() {
        let args = serde_json::json!({});
        let result = bootstrap(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_bootstrap_with_entity_types() {
        let args = serde_json::json!({
            "domain": "aviation",
            "entity_types": "aircraft, airlines, airports"
        });
        let result = bootstrap(&args).unwrap();

        assert_eq!(result["workflow"], "bootstrap");
        assert_eq!(result["domain"], "aviation");
        let instruction = result["instruction"].as_str().unwrap();
        assert!(instruction.contains("aviation"));
        assert!(instruction.contains("aircraft, airlines, airports"));
    }

    #[test]
    fn test_build_bootstrap_prompt_includes_domain_and_entity_types() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt(
            "aviation",
            Some("aircraft, airlines, airports"),
            &prompts,
        );
        assert!(prompt.contains("aviation"));
        assert!(prompt.contains("aircraft, airlines, airports"));
        assert!(prompt.contains("document_types"));
        assert!(prompt.contains("templates"));
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
    fn test_workflow_bootstrap_returns_instruction() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "bootstrap", "domain": "mycology"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "bootstrap");
        assert!(result["instruction"].is_string());
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
        assert_eq!(batch["questions_remaining"], 0);
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
        assert_eq!(batch["questions_remaining"], 3);
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
        // Insert 70 questions across seven docs (more than RESOLVE_BATCH_SIZE=50)
        let types_10: Vec<&str> = vec!["temporal"; 10];
        for i in 0..7 {
            insert_doc_with_questions(&db, &format!("big{:03}", i), &types_10);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(batch["questions"].as_array().unwrap().len(), RESOLVE_BATCH_SIZE);
        assert_eq!(batch["questions_remaining"], 70);
        assert_eq!(batch["total_batches_estimate"], 2);
        assert!(step["when_done"].as_str().unwrap().contains("step=2"));
    }

    #[test]
    fn test_normalize_question_text_strips_footnotes() {
        assert_eq!(
            normalize_question_text(r#"Citation [^1] "source" is weak"#),
            normalize_question_text(r#"Citation [^42] "source" is weak"#),
        );
    }

    #[test]
    fn test_normalize_question_text_strips_quoted_strings() {
        let a = normalize_question_text(r#"Citation [^1] "Phonetool lookup" is not specific"#);
        let b = normalize_question_text(r#"Citation [^1] "LinkedIn profile" is not specific"#);
        assert_eq!(a, b);
    }

    #[test]
    fn test_normalize_question_text_strips_dates() {
        let a = normalize_question_text("source from 2023-06-15 may be outdated");
        let b = normalize_question_text("source from 2024-01-01 may be outdated");
        assert_eq!(a, b);
    }

    #[test]
    fn test_normalize_question_text_strips_temporal_tags() {
        let a = normalize_question_text("has @t[~2023-01] which may be outdated");
        let b = normalize_question_text("has @t[~2024-06] which may be outdated");
        assert_eq!(a, b);
    }

    #[test]
    fn test_normalize_question_text_strips_line_refs() {
        let a = normalize_question_text("Missing source (line 4)");
        let b = normalize_question_text("Missing source (line 99)");
        assert_eq!(a, b);
    }

    #[test]
    fn test_detect_question_patterns_surfaces_repetitive() {
        let questions: Vec<Value> = (0..10)
            .map(|i| serde_json::json!({
                "type": "weak-source",
                "description": format!(r#"Citation [^{i}] "source {i}" is not specific enough"#),
            }))
            .collect();
        let batch = questions[..5].to_vec();
        let patterns = detect_question_patterns(&questions, &batch);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0]["count_total"], 10);
        assert_eq!(patterns[0]["count_in_batch"], 5);
        assert_eq!(patterns[0]["question_type"], "weak-source");
    }

    #[test]
    fn test_detect_question_patterns_ignores_small_groups() {
        let questions: Vec<Value> = (0..3)
            .map(|i| serde_json::json!({
                "type": "temporal",
                "description": format!("Missing date (line {i})"),
            }))
            .collect();
        let patterns = detect_question_patterns(&questions, &questions);
        assert!(patterns.is_empty(), "groups of 3 or fewer should not be surfaced");
    }

    #[test]
    fn test_detect_question_patterns_multiple_groups() {
        let mut questions: Vec<Value> = (0..5)
            .map(|i| serde_json::json!({
                "type": "weak-source",
                "description": format!(r#"Citation [^{i}] "src" is not specific"#),
            }))
            .collect();
        questions.extend((0..6).map(|i| serde_json::json!({
            "type": "stale",
            "description": format!(r#""fact {i}" - source from 2020-01-01 may be outdated"#),
        })));
        let patterns = detect_question_patterns(&questions, &questions);
        assert_eq!(patterns.len(), 2);
        // Sorted by count descending
        assert_eq!(patterns[0]["count_total"], 6);
        assert_eq!(patterns[1]["count_total"], 5);
    }

    #[test]
    fn test_resolve_step2_includes_patterns_detected() {
        let (db, _tmp) = test_db();
        // Insert 5 docs each with the same weak-source question pattern
        for i in 0..5 {
            let id = format!("pat{:03}", i);
            let content = format!(
                "<!-- factbase:{id} -->\n# Doc {id}\n\n- Fact [^1]\n\n---\n[^1]: Phonetool lookup\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1] \"Phonetool lookup\" is not specific enough to verify (line 4)\n"
            );
            insert_test_doc(&db, &id, &content);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let patterns = step["patterns_detected"].as_array().unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0]["count_total"], 5);
        assert_eq!(patterns[0]["count_in_batch"], 5);
        assert_eq!(patterns[0]["question_type"], "weak-source");
        assert!(patterns[0]["suggestion"].as_str().unwrap().contains("5"));
    }

    #[test]
    fn test_resolve_step2_no_patterns_when_few_questions() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "few001", &["temporal", "stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("patterns_detected").is_none(), "should not include patterns_detected when no patterns");
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
        assert!(gate.contains("Resolved 0 of 2 (0%)"));
        assert!(gate.contains("resume"));
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
    fn test_resolve_step2_answer_instruction_allows_partial_completion() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "skip01", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("If you must stop"));
        assert!(instr.contains("report progress honestly"));
    }

    #[test]
    fn test_resolve_step2_continue_true_when_questions_remain() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "cnt001", &["temporal", "stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["continue"], true);
        assert!(step.get("all_resolved").is_none());
    }

    #[test]
    fn test_resolve_step2_continue_false_when_all_resolved() {
        let (db, _tmp) = test_db();
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["continue"], false);
        assert_eq!(step["all_resolved"], true);
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("All review questions have been resolved"));
    }

    #[test]
    fn test_resolve_step2_questions_remaining_in_batch() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "qr001", &["temporal", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["batch"]["questions_remaining"], 2);
    }

    #[test]
    fn test_resolve_step2_mandatory_continuation_in_all_variants() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var001", &["temporal"]);
        for variant in &["baseline", "type_evidence", "research_batch"] {
            let args = serde_json::json!({"variant": variant});
            let step = resolve_step(2, &args, &None, 0, &db, &wf());
            let instr = step["instruction"].as_str().unwrap();
            assert!(instr.contains("CONTINUATION"), "variant {variant} should have CONTINUATION in instruction");
            assert!(instr.contains("If you must stop"), "variant {variant} should allow partial completion");
            assert_eq!(step["continue"], true, "variant {variant} should have continue=true");
        }
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
    fn test_check_repository_workflow_texts_mention_resume_token() {
        // Only scan_repository workflow text should mention resume tokens now
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let setup_instr = setup["instruction"].as_str().unwrap();
        assert!(
            setup_instr.contains("resume"),
            "setup.scan should mention resume token for scan_repository"
        );
    }

    #[test]
    fn test_check_repository_schema_no_resume() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let check = tools_arr.iter().find(|t| t["name"] == "check_repository").unwrap();
        let props = check["inputSchema"]["properties"].as_object().unwrap();
        assert!(!props.contains_key("resume"), "check_repository should not have resume param");
        assert!(!props.contains_key("time_budget_secs"), "check_repository should not have time_budget_secs param");
        assert!(!props.contains_key("mode"), "check_repository should not have mode param");
    }

    #[test]
    fn test_scan_repository_schema_mentions_embeddings() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let scan = tools_arr.iter().find(|t| t["name"] == "scan_repository").unwrap();
        let desc = scan["description"].as_str().unwrap();
        assert!(
            desc.contains("embeddings"),
            "scan_repository schema description should mention embeddings"
        );
    }

    #[test]
    fn test_scan_repository_schema_has_force_reindex() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let scan = tools_arr.iter().find(|t| t["name"] == "scan_repository").unwrap();
        let props = &scan["inputSchema"]["properties"];
        assert!(props.get("force_reindex").is_some(), "scan_repository should have force_reindex param");
        assert_eq!(props["force_reindex"]["type"], "boolean");
    }

    #[test]
    fn test_workflow_texts_mention_fact_pairs() {
        let update_cv = update_step(4, &serde_json::json!({"cross_validate": true}), &None, &wf());
        let cv_instr = update_cv["instruction"].as_str().unwrap();
        assert!(cv_instr.contains("fact comparison") || cv_instr.contains("fact pairs"), "update.cross_validate should mention facts");
    }

    #[test]
    fn test_workflow_texts_mention_time_budget_secs() {
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let update_scan = update_step(1, &serde_json::json!({}), &None, &wf());

        let setup_instr = setup["instruction"].as_str().unwrap();
        assert!(setup_instr.contains("time_budget_secs=120"), "setup.scan should specify time_budget_secs");

        let scan_instr = update_scan["instruction"].as_str().unwrap();
        assert!(scan_instr.contains("time_budget_secs=120"), "update.scan should specify time_budget_secs");
        assert!(scan_instr.contains("Do NOT stop early"), "update.scan should warn against stopping early");
    }

    #[test]
    fn test_paging_instructions_use_mandatory_language() {
        // Only scan steps should have paging language
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let update_scan = update_step(1, &serde_json::json!({}), &None, &wf());

        for (name, instr) in [
            ("setup.scan", setup["instruction"].as_str().unwrap()),
            ("update.scan", update_scan["instruction"].as_str().unwrap()),
        ] {
            assert!(instr.contains("WILL return"), "{name} should say paging WILL happen");
            assert!(instr.contains("MUST"), "{name} should use MUST language for continuation");
        }
    }

    #[test]
    fn test_time_budget_progress_message_warns_incomplete() {
        let mut resp = serde_json::json!({"ok": true});
        crate::mcp::tools::helpers::apply_time_budget_progress(&mut resp, 3, 10, "check_repository", true, None);
        let msg = resp["message"].as_str().unwrap();
        assert!(msg.contains("MANDATORY"));
        assert!(msg.contains("MUST"));
        assert!(msg.contains("Do NOT stop"));
        assert!(resp["when_done"].as_str().unwrap().contains("MANDATORY"));
        assert_eq!(resp["progress"]["percent_complete"], 30);
    }

    #[test]
    fn test_update_check_step() {
        let step = update_step(2, &serde_json::json!({}), &None, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("check_repository"), "step 2 must instruct check_repository");
    }

    #[test]
    fn test_update_cross_validate_step_when_enabled() {
        let step = update_step(4, &serde_json::json!({"cross_validate": true}), &None, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("get_fact_pairs"), "step 4 with cross_validate=true must instruct get_fact_pairs");
    }

    #[test]
    fn test_update_cross_validate_step_skipped_when_disabled() {
        // Without cross_validate, step 4 is organize
        let step = update_step(4, &serde_json::json!({}), &None, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("organize_analyze"), "step 4 without cross_validate should be organize");
    }

    #[test]
    fn test_update_links_step() {
        let step = update_step(3, &serde_json::json!({}), &None, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("get_link_suggestions"), "step 3 must instruct get_link_suggestions");
    }

    #[test]
    fn test_update_total_steps_with_cross_validate() {
        let step = update_step(1, &serde_json::json!({"cross_validate": true}), &None, &wf());
        assert_eq!(step["total_steps"], 6);
    }

    #[test]
    fn test_update_total_steps_without_cross_validate() {
        let step = update_step(1, &serde_json::json!({}), &None, &wf());
        assert_eq!(step["total_steps"], 5);
    }


    #[test]
    fn test_resolve_intro_requires_evidence() {
        let intro = DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION;
        assert!(intro.contains("EVIDENCE REQUIREMENT"), "intro must mention evidence requirement");
        assert!(intro.contains("verified"), "intro must mention verified confidence");
        assert!(intro.contains("believed"), "intro must mention believed confidence");
        assert!(intro.contains("defer"), "intro must mention defer as valid action");
        assert!(intro.contains("GOOD defer"), "intro must frame defer positively");
    }

    #[test]
    fn test_resolve_answer_instruction_requires_research() {
        let instr = DEFAULT_RESOLVE_ANSWER_INSTRUCTION;
        assert!(instr.contains("ANSWER questions"), "answer instruction must prioritize answering");
        assert!(instr.contains("confidence"), "answer instruction must mention confidence field");
        assert!(instr.contains("Minimize research"), "answer instruction must discourage excessive research");
    }

    #[test]
    fn test_resolve_step2_type_filter_returns_only_matching() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "flt001", &["temporal", "stale", "missing"]);
        let args = serde_json::json!({"question_type": "stale"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0]["type"], "stale");
    }

    #[test]
    fn test_resolve_step2_type_filter_no_match_advances() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "flt002", &["temporal"]);
        let args = serde_json::json!({"question_type": "stale"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        assert_eq!(step["batch"]["questions_remaining"], 0);
        assert!(step["when_done"].as_str().unwrap().contains("step=3"));
    }

    #[test]
    fn test_resolve_step2_no_type_filter_returns_all() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "flt003", &["temporal", "stale", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["batch"]["questions"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_resolve_step2_progress_includes_breakdown() {
        let (db, _tmp) = test_db();
        // One verified (answered), one believed (deferred with "believed:"), one deferred, one unanswered
        let content = "<!-- factbase:brk001 -->\n# Breakdown\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[stale]` Believed (line 5)\n  > believed: still accurate per source\n- [ ] `@q[missing]` Deferred (line 6)\n  > defer: could not find source\n- [ ] `@q[conflict]` Unanswered (line 7)\n";
        insert_test_doc(&db, "brk001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(batch["resolved_verified"], 1);
        assert_eq!(batch["resolved_believed"], 1);
        assert_eq!(batch["resolved_deferred"], 1);
        assert_eq!(batch["resolved_so_far"], 3);
        assert_eq!(batch["questions_remaining"], 1);
    }

    #[test]
    fn test_resolve_step2_empty_queue_includes_breakdown() {
        let (db, _tmp) = test_db();
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(batch["resolved_verified"], 0);
        assert_eq!(batch["resolved_believed"], 0);
        assert_eq!(batch["resolved_deferred"], 0);
    }

    #[test]
    fn test_resolve_step2_excludes_believed_from_batch() {
        let (db, _tmp) = test_db();
        // Insert a doc with one believed answer and one unanswered question
        let content = "<!-- factbase:bel001 -->\n# Believed Test\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[stale]` Old fact is stale\n\
            > believed: Still accurate per Wikipedia\n\
            - [ ] `@q[temporal]` When was this true?\n";
        insert_test_doc(&db, "bel001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        // Believed question should be counted but not in the batch
        assert_eq!(batch["resolved_believed"], 1);
        assert_eq!(batch["questions_remaining"], 1);
        let questions = batch["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 1, "Only unanswered question should be in batch, got: {questions:?}");
        assert_eq!(questions[0]["type"], "temporal");
    }

    #[test]
    fn test_resolve_step2_all_resolved_when_only_believed_remain() {
        let (db, _tmp) = test_db();
        // All questions are believed — none truly unanswered
        let content = "<!-- factbase:bonly1 -->\n# Only Believed\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[stale]` Stale fact\n\
            > believed: Still accurate per Wikipedia\n\
            - [ ] `@q[temporal]` When was this true?\n\
            > believed: Circa 2020 based on context\n";
        insert_test_doc(&db, "bonly1", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["all_resolved"], true, "should be all_resolved when only believed remain");
        assert_eq!(step["continue"], false);
        assert_eq!(step["batch"]["resolved_believed"], 2);
        assert_eq!(step["batch"]["questions_remaining"], 0);
    }

    #[test]
    fn test_resolve_step2_believed_not_re_served_across_batches() {
        let (db, _tmp) = test_db();
        // Simulate: one believed + one unanswered
        let content = "<!-- factbase:cyc01 -->\n# Cycle Test\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[stale]` Already believed\n\
            > believed: Confirmed via search\n\
            - [ ] `@q[temporal]` Truly unanswered\n";
        insert_test_doc(&db, "cyc01", content);

        // First batch: should get only the unanswered question
        let step1 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch1 = &step1["batch"];
        assert_eq!(batch1["questions_remaining"], 1);
        let qs = batch1["questions"].as_array().unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["type"], "temporal");

        // Simulate answering with believed — update DB content
        let updated = "<!-- factbase:cyc01 -->\n# Cycle Test\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[stale]` Already believed\n\
            > believed: Confirmed via search\n\
            - [ ] `@q[temporal]` Truly unanswered\n\
            > believed: Circa 2020\n";
        db.update_document_content("cyc01", updated, "hash2").unwrap();

        // Second batch: both are now believed, should be all_resolved
        let step2 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step2["all_resolved"], true, "no infinite loop: believed answers not re-served");
        assert_eq!(step2["batch"]["resolved_believed"], 2);
        assert_eq!(step2["batch"]["questions_remaining"], 0);
    }

    #[test]
    fn test_resolve_step2_deferred_not_re_served_across_batches() {
        let (db, _tmp) = test_db();
        // Simulate: one deferred + one unanswered
        let content = "<!-- factbase:dfc01 -->\n# Defer Cycle\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[ambiguous]` Filed under X but links point to Y\n\
            > defer: cannot determine correct filing\n\
            - [ ] `@q[temporal]` Truly unanswered\n";
        insert_test_doc(&db, "dfc01", content);

        // First batch: should get only the unanswered question
        let step1 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch1 = &step1["batch"];
        assert_eq!(batch1["resolved_deferred"], 1);
        assert_eq!(batch1["questions_remaining"], 1);
        let qs = batch1["questions"].as_array().unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["type"], "temporal");

        // Simulate deferring the remaining question too
        let updated = "<!-- factbase:dfc01 -->\n# Defer Cycle\n\n- Fact\n\n\
            <!-- factbase:review -->\n\
            - [ ] `@q[ambiguous]` Filed under X but links point to Y\n\
            > defer: cannot determine correct filing\n\
            - [ ] `@q[temporal]` Truly unanswered\n\
            > defer: no source available\n";
        db.update_document_content("dfc01", updated, "hash2").unwrap();

        // Second batch: both are now deferred, should be all_resolved
        let step2 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step2["all_resolved"], true, "no infinite loop: deferred answers not re-served");
        assert_eq!(step2["batch"]["resolved_deferred"], 2);
        assert_eq!(step2["batch"]["questions_remaining"], 0);
    }

    #[test]
    fn test_resolve_answer_instruction_prohibits_scan_check() {
        let instr = DEFAULT_RESOLVE_ANSWER_INSTRUCTION;
        assert!(instr.contains("Do NOT call scan_repository or check_repository"), "answer instruction must prohibit scan/check");
    }

    #[test]
    fn test_resolve_workflow_list_description_mentions_no_scan() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let resolve_wf = workflows.iter().find(|w| w["name"] == "resolve").unwrap();
        let desc = resolve_wf["description"].as_str().unwrap();
        assert!(desc.contains("Does NOT scan or check"), "resolve description should clarify no scan/check");
    }

    // --- resolve variant tests ---

    #[test]
    fn test_resolve_step2_baseline_variant_is_default() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["variant"], "baseline");
        // No evidence_guidance on questions
        let q = &step["batch"]["questions"].as_array().unwrap()[0];
        assert!(q.get("evidence_guidance").is_none());
    }

    #[test]
    fn test_resolve_step2_type_evidence_variant_adds_guidance() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var002", &["temporal", "stale", "ambiguous"]);
        let args = serde_json::json!({"variant": "type_evidence"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        assert_eq!(step["variant"], "type_evidence");
        let questions = step["batch"]["questions"].as_array().unwrap();
        for q in questions {
            assert!(q["evidence_guidance"].is_string(), "type_evidence variant should add evidence_guidance to each question");
        }
        // Check type-specific guidance content
        let temporal_q = questions.iter().find(|q| q["type"] == "temporal").unwrap();
        assert!(temporal_q["evidence_guidance"].as_str().unwrap().contains("specific event date"));
        let stale_q = questions.iter().find(|q| q["type"] == "stale").unwrap();
        assert!(stale_q["evidence_guidance"].as_str().unwrap().contains("current year"));
        let ambiguous_q = questions.iter().find(|q| q["type"] == "ambiguous").unwrap();
        assert!(ambiguous_q["evidence_guidance"].as_str().unwrap().contains("Check the KB first"));
    }

    #[test]
    fn test_resolve_step2_type_evidence_variant_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var003", &["temporal"]);
        let args = serde_json::json!({"variant": "type_evidence"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("varies by question type"), "type_evidence intro should mention type-specific evidence");
        assert!(intro.contains("STALE:"), "type_evidence intro should have STALE section");
        assert!(intro.contains("TEMPORAL:"), "type_evidence intro should have TEMPORAL section");
        assert!(intro.contains("AMBIGUOUS:"), "type_evidence intro should have AMBIGUOUS section");
        assert!(intro.contains("CONFLICT:"), "type_evidence intro should have CONFLICT section");
        assert!(intro.contains("PRECISION:"), "type_evidence intro should have PRECISION section");
    }

    #[test]
    fn test_resolve_step2_type_evidence_variant_answer_instruction() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var004", &["temporal"]);
        let args = serde_json::json!({"variant": "type_evidence"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("evidence_guidance"), "type_evidence answer instruction should reference evidence_guidance field");
    }

    #[test]
    fn test_resolve_step2_research_batch_variant_groups_by_doc() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "rbat01", &["temporal", "stale"]);
        insert_doc_with_questions(&db, "rbat02", &["missing"]);
        let args = serde_json::json!({"variant": "research_batch"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        assert_eq!(step["variant"], "research_batch");
        // Should have document_groups instead of flat questions
        let groups = step["batch"]["document_groups"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "should have 2 document groups");
        // First group should be rbat01 (alphabetical)
        assert_eq!(groups[0]["doc_id"], "rbat01");
        assert_eq!(groups[0]["questions"].as_array().unwrap().len(), 2);
        // Second group should be rbat02
        assert_eq!(groups[1]["doc_id"], "rbat02");
        assert_eq!(groups[1]["questions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_resolve_step2_research_batch_variant_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "rbat03", &["temporal"]);
        let args = serde_json::json!({"variant": "research_batch"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let intro = step["intro"].as_str().unwrap();
        assert!(intro.contains("research-first"), "research_batch intro should mention research-first approach");
        assert!(intro.contains("PHASE 1"), "research_batch intro should have Phase 1");
        assert!(intro.contains("PHASE 2"), "research_batch intro should have Phase 2");
        assert!(intro.contains("get_entity"), "research_batch intro should mention get_entity");
    }

    #[test]
    fn test_resolve_step2_research_batch_variant_answer_instruction() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "rbat04", &["temporal"]);
        let args = serde_json::json!({"variant": "research_batch"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("research-first"), "research_batch answer instruction should mention research-first");
        assert!(instr.contains("document group"), "research_batch answer instruction should mention document groups");
    }

    #[test]
    fn test_resolve_step2_baseline_has_flat_questions() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "flat01", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // Baseline should have flat questions array, not document_groups
        assert!(step["batch"]["questions"].is_array());
        assert!(step["batch"].get("document_groups").is_none());
    }

    #[test]
    fn test_resolve_step2_variant_empty_queue_still_works() {
        let (db, _tmp) = test_db();
        for variant in &["baseline", "type_evidence", "research_batch"] {
            let args = serde_json::json!({"variant": variant});
            let step = resolve_step(2, &args, &None, 0, &db, &wf());
            assert_eq!(step["batch"]["questions_remaining"], 0);
            assert!(step["when_done"].as_str().unwrap().contains("step=3"));
        }
    }

    #[test]
    fn test_resolve_step2_type_evidence_all_types_have_guidance() {
        // Verify every QuestionType has a non-empty evidence guidance
        let types = [
            QuestionType::Stale,
            QuestionType::Temporal,
            QuestionType::Ambiguous,
            QuestionType::Conflict,
            QuestionType::Precision,
            QuestionType::Missing,
            QuestionType::Duplicate,
            QuestionType::Corruption,
        ];
        for qt in &types {
            let guidance = type_evidence_guidance(qt);
            assert!(!guidance.is_empty(), "type_evidence_guidance should be non-empty for {:?}", qt);
        }
    }

    #[test]
    fn test_resolve_step2_variant_preserves_type_filter() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "vflt01", &["temporal", "stale", "missing"]);
        let args = serde_json::json!({"variant": "type_evidence", "question_type": "stale"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0]["type"], "stale");
        assert!(questions[0]["evidence_guidance"].is_string());
    }

    #[test]
    fn test_resolve_step2_comma_separated_type_filter() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "cft001", &["temporal", "stale", "missing", "ambiguous"]);
        let args = serde_json::json!({"question_type": "stale,missing"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 2);
        let types: Vec<&str> = questions.iter().filter_map(|q| q["type"].as_str()).collect();
        assert!(types.contains(&"stale"));
        assert!(types.contains(&"missing"));
        assert!(!types.contains(&"temporal"));
    }

    #[test]
    fn test_resolve_step2_type_distribution_included() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tdi001", &["temporal", "temporal", "stale", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let dist = step["type_distribution"].as_array().unwrap();
        assert!(!dist.is_empty());
        // temporal should be first (count=2)
        assert_eq!(dist[0]["type"], "temporal");
        assert_eq!(dist[0]["count"], 2);
    }

    #[test]
    fn test_resolve_step2_type_distribution_with_filter() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tdf001", &["temporal", "stale", "missing"]);
        let args = serde_json::json!({"question_type": "stale"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        // Distribution shows ALL types, not just filtered
        let dist = step["type_distribution"].as_array().unwrap();
        assert_eq!(dist.len(), 3);
        // But only stale questions in the batch
        assert_eq!(step["batch"]["questions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_resolve_step2_type_filter_in_response() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tfr001", &["temporal", "stale"]);
        // With filter
        let step = resolve_step(2, &serde_json::json!({"question_type": "stale"}), &None, 0, &db, &wf());
        let filter = step["type_filter"].as_array().unwrap();
        assert_eq!(filter.len(), 1);
        assert_eq!(filter[0], "stale");
        // Without filter
        let step2 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step2["type_filter"].is_null());
    }

    #[test]
    fn test_resolve_step2_comma_filter_all_resolved_reflects_filter() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "cfr001", &["temporal", "stale"]);
        // Filter for a type that has no questions
        let step = resolve_step(2, &serde_json::json!({"question_type": "conflict"}), &None, 0, &db, &wf());
        assert_eq!(step["all_resolved"], true);
        assert_eq!(step["batch"]["questions_remaining"], 0);
        // But type_distribution still shows the real types
        let dist = step["type_distribution"].as_array().unwrap();
        assert_eq!(dist.len(), 2);
    }

    #[test]
    fn test_workflow_schema_has_variant_param() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let props = &wf_tool["inputSchema"]["properties"];
        assert!(props.get("variant").is_some(), "workflow schema should have variant param");
        let variant_enum = props["variant"]["enum"].as_array().unwrap();
        let values: Vec<&str> = variant_enum.iter().filter_map(|v| v.as_str()).collect();
        assert!(values.contains(&"baseline"));
        assert!(values.contains(&"type_evidence"));
        assert!(values.contains(&"research_batch"));
    }

    #[test]
    fn test_resolve_step2_glossary_auto_resolves_acronym_questions() {
        let (db, _tmp) = test_db();
        // Insert a glossary document defining "HCLS"
        let glossary_content = "<!-- factbase:gls001 -->\n# Glossary\n\n- **HCLS**: Healthcare and Life Sciences\n";
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));
        db.upsert_document(&Document {
            id: "gls001".to_string(),
            content: glossary_content.to_string(),
            title: "Glossary".to_string(),
            file_path: "definitions/glossary.md".to_string(),
            doc_type: Some("definition".to_string()),
            ..Document::test_default()
        }).unwrap();

        // Insert a doc with an ambiguous acronym question about HCLS
        let doc_content = "<!-- factbase:acr001 -->\n# Project\n\n- Expanding HCLS practice\n\n<!-- factbase:review -->\n- [ ] `@q[ambiguous]` \"Expanding HCLS practice\" - what does \"HCLS\" mean in this context?\n";
        db.upsert_document(&Document {
            id: "acr001".to_string(),
            content: doc_content.to_string(),
            title: "Project".to_string(),
            file_path: "acr001.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        // The HCLS question should be auto-resolved, not in the batch
        assert_eq!(batch["questions_remaining"], 0, "glossary-defined acronym question should be auto-resolved");
        assert_eq!(batch["glossary_auto_resolved"], 1, "should report glossary_auto_resolved count");
    }

    #[test]
    fn test_resolve_step2_glossary_does_not_resolve_non_acronym_questions() {
        let (db, _tmp) = test_db();
        // Insert a glossary document
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));
        db.upsert_document(&Document {
            id: "gls002".to_string(),
            content: "<!-- factbase:gls002 -->\n# Glossary\n\n- **HCLS**: Healthcare\n".to_string(),
            title: "Glossary".to_string(),
            file_path: "definitions/glossary.md".to_string(),
            doc_type: Some("definition".to_string()),
            ..Document::test_default()
        }).unwrap();

        // Insert a doc with a non-acronym ambiguous question (location)
        let doc_content = "<!-- factbase:loc001 -->\n# Person\n\n- Lives in NYC\n\n<!-- factbase:review -->\n- [ ] `@q[ambiguous]` \"Lives in NYC\" - is this home, work, or another type of location?\n";
        db.upsert_document(&Document {
            id: "loc001".to_string(),
            content: doc_content.to_string(),
            title: "Person".to_string(),
            file_path: "loc001.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        // Location question should NOT be auto-resolved
        assert_eq!(batch["questions_remaining"], 1, "non-acronym question should remain");
    }

    #[test]
    fn test_resolve_step2_variant_from_config() {
        let (db, _tmp) = test_db();
        let mut wf_config = WorkflowsConfig::default();
        wf_config.resolve_variant = Some("type_evidence".into());

        // No variant in args — should use config
        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf_config);
        assert_eq!(step["variant"], "type_evidence");
        assert_eq!(step["variant_source"], "config");
    }

    #[test]
    fn test_resolve_step2_variant_arg_overrides_config() {
        let (db, _tmp) = test_db();
        let mut wf_config = WorkflowsConfig::default();
        wf_config.resolve_variant = Some("type_evidence".into());

        // Explicit arg overrides config
        let step = resolve_step2_batch(
            &serde_json::json!({"variant": "research_batch"}),
            &None, &db, &wf_config,
        );
        assert_eq!(step["variant"], "research_batch");
        assert_eq!(step["variant_source"], "arg");
    }

    #[test]
    fn test_resolve_step2_variant_default_when_no_config() {
        let (db, _tmp) = test_db();
        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf());
        assert_eq!(step["variant"], "baseline");
        assert_eq!(step["variant_source"], "default");
    }

    #[test]
    fn test_resolve_step2_custom_prompt_override() {
        let (db, _tmp) = test_db();
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;
        test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));

        // Insert a doc with a question so we get a real batch
        let doc_content = "<!-- factbase:cst001 -->\n# Entity\n\n- Some fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Source is old\n";
        db.upsert_document(&Document {
            id: "cst001".to_string(),
            content: doc_content.to_string(),
            title: "Entity".to_string(),
            file_path: "cst001.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        let mut wf_config = WorkflowsConfig::default();
        wf_config.templates.insert("resolve.answer".into(), "CUSTOM INSTRUCTION {ctx}".into());

        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf_config);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.starts_with("CUSTOM INSTRUCTION"), "should use custom prompt override");
    }

    #[test]
    fn test_merge_repo_prompts_overrides_global() {
        let mut global = WorkflowsConfig::default();
        global.templates.insert("resolve.answer".into(), "global answer".into());
        global.resolve_variant = Some("baseline".into());

        let mut repo = WorkflowsConfig::default();
        repo.templates.insert("resolve.answer".into(), "repo answer".into());
        repo.resolve_variant = Some("type_evidence".into());

        global.merge(&repo);
        assert_eq!(global.templates["resolve.answer"], "repo answer");
        assert_eq!(global.resolve_variant.as_deref(), Some("type_evidence"));
    }

    #[test]
    fn test_load_review_docs_from_disk_prefers_disk_content() {
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;

        let (db, tmp) = test_db();
        let repo_path = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        test_repo_in_db(&db, "test-repo", &repo_path);

        // DB content has NO review queue
        let db_content = "<!-- factbase:dsk001 -->\n# Disk Test\n\n- Fact\n";
        db.upsert_document(&Document {
            id: "dsk001".to_string(),
            content: db_content.to_string(),
            title: "Disk Test".to_string(),
            file_path: "dsk001.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        // Disk file HAS review queue with weak-source questions
        let disk_content = "<!-- factbase:dsk001 -->\n# Disk Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Line 4: Citation needed\n";
        std::fs::write(repo_path.join("dsk001.md"), disk_content).unwrap();

        let docs = load_review_docs_from_disk(&db);
        assert_eq!(docs.len(), 1, "should find the doc via disk content");
        assert!(docs[0].content.contains("@q[weak-source]"), "should use disk content");
    }

    #[test]
    fn test_load_review_docs_from_disk_falls_back_to_db() {
        let (db, _tmp) = test_db();
        // DB content HAS review queue, but no disk file exists
        let content = "<!-- factbase:fb001 -->\n# Fallback\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source\n";
        insert_test_doc(&db, "fb001", content);

        let docs = load_review_docs_from_disk(&db);
        assert_eq!(docs.len(), 1, "should find the doc via DB content fallback");
        assert!(docs[0].content.contains("@q[stale]"));
    }

    #[test]
    fn test_resolve_step2_finds_disk_only_questions() {
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;

        let (db, tmp) = test_db();
        let repo_path = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        test_repo_in_db(&db, "test-repo", &repo_path);

        // DB content has NO review queue (has_review_queue = FALSE)
        let db_content = "<!-- factbase:dsk002 -->\n# Disk Only\n\n- Fact\n";
        db.upsert_document(&Document {
            id: "dsk002".to_string(),
            content: db_content.to_string(),
            title: "Disk Only".to_string(),
            file_path: "dsk002.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        // Disk file has weak-source questions
        let disk_content = "<!-- factbase:dsk002 -->\n# Disk Only\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Line 4: Vague citation\n- [ ] `@q[weak-source]` Line 5: Missing URL\n";
        std::fs::write(repo_path.join("dsk002.md"), disk_content).unwrap();

        // Type filter for weak-source should find the questions from disk
        let step = resolve_step(2, &serde_json::json!({"question_type": "weak-source"}), &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 2, "should find weak-source questions from disk");
        assert_eq!(questions[0]["type"], "weak-source");
    }

    #[test]
    fn test_compute_type_distribution_reads_disk() {
        use crate::database::tests::test_repo_in_db;
        use crate::models::Document;

        let (db, tmp) = test_db();
        let repo_path = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        test_repo_in_db(&db, "test-repo", &repo_path);

        // DB content has NO review queue
        db.upsert_document(&Document {
            id: "dist01".to_string(),
            content: "<!-- factbase:dist01 -->\n# Dist\n\n- Fact\n".to_string(),
            title: "Dist".to_string(),
            file_path: "dist01.md".to_string(),
            ..Document::test_default()
        }).unwrap();

        // Disk file has questions
        std::fs::write(
            repo_path.join("dist01.md"),
            "<!-- factbase:dist01 -->\n# Dist\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Vague\n- [ ] `@q[temporal]` Missing date\n",
        ).unwrap();

        let dist = compute_type_distribution(&db);
        assert_eq!(dist.len(), 2);
        let ws = dist.iter().find(|(qt, _)| *qt == QuestionType::WeakSource);
        assert!(ws.is_some(), "should find weak-source from disk");
        assert_eq!(ws.unwrap().1, 1);
    }

    #[test]
    fn test_continuation_guidance_none_for_small_queue() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 5);
        let result = build_continuation_guidance(5, 10, 50, &dist, &[]);
        assert!(result.is_none(), "small queues should not produce guidance");
    }

    #[test]
    fn test_continuation_guidance_momentum_over_100() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 150);
        let result = build_continuation_guidance(150, 50, 50, &dist, &[]).unwrap();
        assert!(result.contains("⚡"), "should have lightning emoji");
        assert!(result.contains("150"), "should mention remaining count");
        assert!(result.contains("cleared 50"), "should mention progress");
        assert!(result.contains("momentum"), "should urge momentum");
        // Should NOT have batch estimate (that's >500 only)
        assert!(!result.contains("batches remaining"), "should not have batch estimate under 500");
    }

    #[test]
    fn test_continuation_guidance_batches_over_500() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::WeakSource, 4421);
        let filter = vec![QuestionType::WeakSource];
        let result = build_continuation_guidance(4421, 79, 50, &dist, &filter).unwrap();
        assert!(result.contains("⚡"), "should have lightning emoji");
        assert!(result.contains("4421"), "should mention remaining count");
        assert!(result.contains("batches remaining"), "should have batch estimate");
        assert!(result.contains("question_type=weak-source"), "should include filter hint");
    }

    #[test]
    fn test_continuation_guidance_type_cleared_suggests_next() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 0);
        dist.insert(QuestionType::Ambiguous, 25);
        let filter = vec![QuestionType::Temporal];
        // remaining=25 is the ambiguous count (temporal is cleared, agent sees ambiguous next)
        let result = build_continuation_guidance(25, 30, 50, &dist, &filter).unwrap();
        assert!(result.contains("✅"), "should have checkmark");
        assert!(result.contains("temporal: 0 remaining"), "should note cleared type");
        assert!(result.contains("ambiguous"), "should suggest next type");
        assert!(result.contains("25 remaining"), "should show next type count");
    }

    #[test]
    fn test_continuation_guidance_only_weak_source_remains() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::WeakSource, 200);
        let result = build_continuation_guidance(200, 100, 50, &dist, &[]).unwrap();
        assert!(result.contains("Only weak-source remains"), "should note only weak-source left");
        assert!(result.contains("repetitive patterns"), "should mention patterns");
    }

    #[test]
    fn test_continuation_guidance_in_step2_response() {
        let (db, _tmp) = test_db();
        // Insert >100 questions to trigger guidance
        let types_10: Vec<&str> = vec!["temporal"; 10];
        for i in 0..11 {
            insert_doc_with_questions(&db, &format!("cg{:03}", i), &types_10);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("continuation_guidance").is_some(), "should have continuation_guidance for large queue");
        let guidance = step["continuation_guidance"].as_str().unwrap();
        assert!(guidance.contains("⚡"), "guidance should be directive");
    }

    #[test]
    fn test_continuation_guidance_absent_for_small_step2() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "sm001", &["temporal", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("continuation_guidance").is_none(), "small queue should not have continuation_guidance");
    }
}
