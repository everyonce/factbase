//! MCP tool schema definitions.
//!
//! Exposes 2 tools: `workflow` (guided multi-step entry point) and `factbase`
//! (unified operations tool). Old individual tool names are kept as dispatch
//! aliases for backward compatibility but are not listed in the schema.

use serde_json::Value;
use std::path::Path;

/// Load a schema description override from `.factbase/schema/<tool>.md`.
fn load_schema_override(tool_name: &str, repo_path: Option<&Path>) -> Option<String> {
    let rp = repo_path?;
    let path = rp
        .join(".factbase")
        .join("schema")
        .join(format!("{tool_name}.md"));
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Returns the complete list of available MCP tools with their schemas.
///
/// If `repo_path` is provided, checks for schema description overrides
/// in `.factbase/schema/<tool>.md`.
pub fn tools_list_with_overrides(repo_path: Option<&Path>) -> Value {
    serde_json::json!({
        "tools": [
            search_schema(repo_path),
            workflow_schema(repo_path),
            factbase_schema(repo_path),
        ]
    })
}

/// Returns the complete list of available MCP tools with their schemas.
///
/// This is returned in response to `tools/list` requests.
pub fn tools_list() -> Value {
    tools_list_with_overrides(None)
}

/// Returns the list of old tool names that are kept as dispatch aliases.
/// These are NOT in the schema but still accepted by handle_tool_call.
#[cfg(test)]
pub fn legacy_tool_names() -> &'static [&'static str] {
    &[
        "search_knowledge",
        "get_entity",
        "list_entities",
        "get_perspective",
        "create_document",
        "update_document",
        "delete_document",
        "bulk_create_documents",
        "get_review_queue",
        "get_deferred_items",
        "answer_questions",
        "check_repository",
        "scan_repository",
        "detect_links",
        "get_authoring_guide",
        "organize_analyze",
        "organize",
        "embeddings_export",
        "embeddings_import",
        "embeddings_status",
        "get_link_suggestions",
        "store_links",
        "get_fact_pairs",
    ]
}

/// Op names that were removed but return helpful errors for backward compat.
pub fn removed_op_messages() -> &'static [(&'static str, &'static str)] {
    &[
        ("repos", "'repos' op removed. Use op='perspective' instead — it returns KB config, stats, and repository info."),
        ("init", "'init' op removed. Repositories auto-initialize on first scan. Use op='scan' with a registered repo path."),
        ("search_content", "'search_content' op removed. Use the standalone 'search' tool with mode='content' instead."),
    ]
}

/// Legacy tool names that were removed but return helpful errors.
pub fn removed_legacy_tool_messages() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "list_repositories",
            "'list_repositories' removed. Use factbase(op='perspective') instead.",
        ),
        (
            "search_content",
            "'search_content' removed. Use the 'search' tool with mode='content' instead.",
        ),
        (
            "migrate_links",
            "'migrate_links' removed. Link migration is no longer needed.",
        ),
    ]
}

fn search_schema(repo_path: Option<&Path>) -> Value {
    let default_desc = "Search the factbase. Returns entities with outgoing links.\nModes: semantic (default) or content (exact text/regex).\nFilters: doc_type, title_filter, as_of, during, exclude_unknown, boost_recent.";
    let desc =
        load_schema_override("search", repo_path).unwrap_or_else(|| default_desc.to_string());
    serde_json::json!({
        "name": "search",
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query (semantic or content pattern)" },
                "mode": { "type": "string", "enum": ["semantic", "content"], "description": "Search mode (default: semantic)" },
                "limit": { "type": "integer", "description": "Max results (default: 10)" },
                "doc_type": { "type": "string", "description": "Filter by document type" },
                "title_filter": { "type": "string", "description": "Filter by title (partial match)" },
                "as_of": { "type": "string", "description": "Filter to facts valid at date (YYYY, YYYY-MM, or YYYY-MM-DD)" },
                "during": { "type": "string", "description": "Filter to facts valid during range (YYYY..YYYY)" },
                "exclude_unknown": { "type": "boolean", "description": "Exclude facts with @t[?] tags" },
                "boost_recent": { "type": "boolean", "description": "Boost ranking of recent dates" },
                "offset": { "type": "integer", "description": "Pagination offset" },
                "pattern": { "type": "string", "description": "Text pattern for content mode" },
                "context": { "type": "integer", "description": "Context lines around content matches" }
            },
            "required": ["query"]
        }
    })
}

fn workflow_schema(repo_path: Option<&Path>) -> Value {
    let default_desc = "Guided multi-step workflows for factbase tasks. workflow= to specify: create, add, maintain, refresh, correct, transition\nCall with step=1 to start. Use workflow='list' for details.\nOPERATING PRINCIPLE: Routing behavior depends on the workflow type — do NOT apply the same rule to all cases.\n- For correct/transition: the user is telling you the truth. Route to correct/transition IMMEDIATELY as your FIRST action — do NOT search or verify their claim first. Searching first may cause you to answer from training data and skip the workflow.\n- For add/ingest: search the KB first to avoid creating duplicates. Then proceed with ingestion.\n- For clarification (unclear entity named): ask ONE focused question — do NOT search to formulate it.\n- Uncertain about WHAT ACTION? → Execute with the best-fit workflow. The workflow handles validation.\nIMPORTANT: If the user explicitly names a workflow (e.g. \"use the correction workflow\"), ALWAYS use that workflow — do NOT override.\nRouting rules (apply in order):\n- 'build', 'create', 'start', 'new KB' → workflow(create)\n- 'add [new topic/entity]' → workflow(add, topic=...)\n- 'add [note/flag/tag] to [existing entity]' → workflow(correct) [add vs correct: add=CREATE new entities not yet in KB; correct=MODIFY existing entities (add notes, annotations). 'Add a note to X' → correct if X exists]\n- 'scan', 'index', 'reindex' → workflow(maintain) — do NOT call factbase(op='scan') directly\n- 'check for new', 'look for updates', 'what's new' → workflow(refresh) [add vs refresh: add=CREATE new docs; refresh=UPDATE existing docs]\n- factual correction about existing entity → workflow(correct) IMMEDIATELY as FIRST action. Do NOT search first (may cause you to answer from training data)\n- change that happened over time → workflow(transition)\n- vague improvement requests ('clean up', 'fix', 'improve', 'organize') → workflow(maintain, user_message='<user exact words>') — workflow will ask for scope confirmation; no entity named → ASK one focused clarifying question\ncorrect vs transition — was the old information ever actually true?\n  NO → it was always wrong → use correct; YES → it was true until a specific point → use transition\n⚠️ KB IS SOURCE OF TRUTH: ALWAYS use factbase workflows FIRST — do NOT answer from training data, use web_search, or query memory instead of the KB.\n⚠️ CLARIFICATION: If no entity or change is specified and you cannot infer a reasonable default, ask ONE focused clarifying question. Do NOT ask when a reasonable default workflow exists (e.g. 'Make the KB better' → maintain).";
    let desc =
        load_schema_override("workflow", repo_path).unwrap_or_else(|| default_desc.to_string());
    serde_json::json!({
        "name": "workflow",
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": {
                "workflow": { "type": "string", "description": "Workflow name: 'create', 'add', 'maintain', 'refresh', 'correct', 'transition', 'resolve', or 'list'. Legacy aliases also accepted: bootstrap, setup, update, ingest, enrich, improve\n\ncorrect: Use when the old information was ALWAYS WRONG — a factual error, misidentification, or false claim that was never true. Example: a company name was consistently misspelled (it was NEVER called 48U, it was always FortyAU). The agent analyzed the situation wrongly and stored a false fact.\n\ntransition: Use when the old information WAS TRUE at the time, but the entity itself changed. Example: a company that used to be called Advent Health Partners and was ACQUIRED and renamed to Trend Health Partners. The name Advent was valid until the acquisition date.\n\nKey test: ask 'was the old value ever actually true?' — if NO → correct. If YES, it was true until a specific date → transition.\n\nrefresh: Use when the user wants to check for recent updates, new developments, latest news, or whether anything has changed about a topic. Trigger phrases: 'check for updates', 'look for recent', 'what's new', 'has anything changed', 'recent news/developments/discoveries', 'latest info'. Example: 'Has anything changed with [topic]?' → refresh, topic='[topic]'. IMPORTANT: refresh=UPDATE existing docs with new info from external research; add=CREATE new docs." },
                "step": { "type": "integer", "description": "Step number (default: 1 = start)" },
                "domain": { "type": "string", "description": "For create: domain description (e.g. 'mycology', 'ancient Mediterranean history')" },
                "entity_types": { "type": "string", "description": "For create: optional comma-separated entity types (e.g. 'species, habitats, researchers')" },
                "path": { "type": "string", "description": "For create: directory path for the new repository" },
                "topic": { "type": "string", "description": "For add: what to research (triggers ingest mode)" },
                "doc_type": { "type": "string", "description": "For add/refresh/resolve: document type to focus on" },
                "doc_id": { "type": "string", "description": "For add: document ID to improve. For refresh: specific document to refresh." },
                "correction": { "type": "string", "description": "For correct: free text describing what is wrong and what the true fact is." },
                "change": { "type": "string", "description": "For transition: what changed — free text (e.g. 'Acme Corp rebranded to NewCo as of today')." },
                "effective_date": { "type": "string", "description": "For transition: when the change happened (ISO date). Defaults to today if omitted." },
                "nomenclature": { "type": "string", "description": "For transition step 2: how to reference the entity going forward (user's choice from the options presented)." },
                "source": { "type": "string", "description": "For correct/transition: optional citation (who said it, when)." },
                "question_type": { "type": "string", "description": "For resolve step 2: filter questions by type. Comma-separated for multiple types (e.g. 'temporal,ambiguous')." },
                "variant": { "type": "string", "enum": ["baseline", "type_evidence", "research_batch"], "description": "For resolve: prompt variant to use." },
                "cross_validate": { "type": "boolean", "description": "For maintain: include cross-document fact validation step (default: false)." },
                "user_message": { "type": "string", "description": "For maintain: pass the user's original message when routing vague requests ('clean up', 'fix everything', 'improve the KB'). Enables scope-confirm gate to ask for clarification before starting a long maintenance cycle." },
                "skip": {
                    "oneOf": [
                        { "type": "string", "description": "Comma-separated step names to skip" },
                        { "type": "array", "items": { "type": "string" }, "description": "Step names to skip" }
                    ],
                    "description": "For add (improve mode): steps to skip. Valid names: 'cleanup', 'resolve', 'enrich', 'check'"
                },
            },
            "required": ["workflow"]
        }
    })
}

fn factbase_schema(repo_path: Option<&Path>) -> Value {
    let default_desc = "Knowledge base operations. Use op= to specify:\n\nDOCUMENTS: get_entity(id), create(path,title,content), update(id,content), delete(id), bulk_create(documents[]), list(doc_type?,limit?)\nQUALITY: check(doc_id?), scan(time_budget_secs?) — re-index documents (for full maintenance use workflow(maintain) instead), detect_links(time_budget_secs?)\nREVIEW: review_queue(doc_id?), answer(doc_id,question_index,answer), deferred()\nORGANIZE: organize(action=analyze|move|merge|split|delete|retype|execute_suggestions)\nLINKS: links(action=suggest|store), fact_pairs(min_similarity?)\nMETA: perspective(), authoring_guide(), embeddings(action=export|import|status), doctor(), status()\n⚠️ For multi-step operations (maintain, add, correct, refresh), use workflow() — not individual ops. Use factbase() directly only for: simple lookups, single targeted updates, or when a workflow tells you to.\n⚠️ KB IS SOURCE OF TRUTH: When factbase is configured, ALWAYS query the KB first — do NOT answer from training data, use web_search, or query memory/other tools instead of the KB.";
    let desc =
        load_schema_override("factbase", repo_path).unwrap_or_else(|| default_desc.to_string());
    serde_json::json!({
        "name": "factbase",
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": {
                "op": {
                    "type": "string",
                    "enum": [
                        "get_entity", "list", "perspective",
                        "create", "update", "delete", "bulk_create",
                        "scan", "check", "detect_links",
                        "review_queue", "answer", "deferred",
                        "organize", "links", "fact_pairs", "embeddings",
                        "authoring_guide", "doctor", "status"
                    ],
                    "description": "Operation to perform"
                },
                // Common
                "doc_id": { "type": "string", "description": "Document ID" },
                "limit": { "type": "integer", "description": "Max results" },
                "offset": { "type": "integer", "description": "Pagination offset" },
                // Filters
                "title_filter": { "type": "string", "description": "Filter by title (partial match)" },
                "doc_type": { "type": "string", "description": "Filter by document type" },
                // Entity
                "id": { "type": "string", "description": "Document ID (get_entity, update, delete)" },
                "detail": { "type": "string", "description": "get_entity: 'full' or 'stats'" },
                "include_preview": { "type": "boolean", "description": "Include 500-char content preview" },
                "max_content_length": { "type": "integer", "description": "Truncate content to this length" },
                // Document CRUD
                "path": { "type": "string", "description": "File path (create)" },
                "title": { "type": "string", "description": "Document title" },
                "content": { "type": "string", "description": "Document content" },
                "documents": { "type": "array", "description": "Array of {path, title, content} for bulk_create (max 100)", "items": { "type": "object" } },
                // Organization suggestions (update op)
                "suggested_move": { "type": "string", "description": "Advisory: target directory path for file move (stored as pending suggestion)" },
                "suggested_rename": { "type": "string", "description": "Advisory: new filename for file rename (stored as pending suggestion)" },
                "suggested_title": { "type": "string", "description": "Advisory: new entity title (stored as pending suggestion)" },
                // Scan
                "force_reindex": { "type": "boolean", "description": "Force re-generation of all embeddings" },
                "skip_embeddings": { "type": "boolean", "description": "Skip embedding generation" },
                "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600)" },
                "resume": { "type": "string", "description": "Resume token from previous call" },
                // Check
                "doc_ids": { "type": "array", "items": { "type": "string" }, "description": "Check specific documents" },
                "dry_run": { "type": "boolean", "description": "Preview without modifying" },
                // Review
                "type": { "type": "string", "description": "Question type filter" },
                "status": { "type": "string", "description": "Question status filter" },
                "question_index": { "type": "integer", "description": "0-based question index" },
                "answer": { "type": "string", "description": "Answer text" },
                "confidence": { "type": "string", "enum": ["verified", "believed"], "description": "Answer confidence" },
                "answers": { "type": "array", "description": "Bulk answers array", "items": { "type": "object" } },
                // Organize
                "action": { "type": "string", "description": "Sub-action: analyze/merge/split/delete/move/retype/apply/execute_suggestions (organize), suggest/store (links), export/import/status (embeddings)" },
                "to": { "type": "string", "description": "Destination folder (organize move)" },
                "new_type": { "type": "string", "description": "New type (organize retype)" },
                "source_id": { "type": "string", "description": "Source document ID (organize merge)" },
                "target_id": { "type": "string", "description": "Target document ID (organize merge)" },
                "sections": { "type": "array", "description": "Array of {title, content} for split", "items": { "type": "object" } },
                "persist": { "type": "boolean", "description": "Persist type override" },
                "focus": { "type": "string", "enum": ["duplicates", "structure"], "description": "Organize analysis focus" },
                "merge_threshold": { "type": "number", "description": "Merge similarity threshold" },
                "split_threshold": { "type": "number", "description": "Split similarity threshold" },
                // Links
                "min_similarity": { "type": "number", "description": "Minimum similarity threshold" },
                "include_types": { "type": "array", "items": { "type": "string" }, "description": "Include candidate types" },
                "exclude_types": { "type": "array", "items": { "type": "string" }, "description": "Exclude candidate types" },
                "links": { "type": "array", "description": "Links to store [{source_id, target_id}]", "items": { "type": "object" } },
                // Embeddings
                "data": { "type": "string", "description": "JSONL data for embeddings import" },
                "force": { "type": "boolean", "description": "Force import despite dimension mismatch" }
            },
            "required": ["op"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_list_has_three_tools() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools should be array");
        assert_eq!(
            tools.len(),
            3,
            "should have exactly 3 tools: search + workflow + factbase"
        );

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"search"));
        assert!(names.contains(&"workflow"));
        assert!(names.contains(&"factbase"));
    }

    #[test]
    fn test_search_schema_has_required_query() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let required = search["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "query"));
    }

    #[test]
    fn test_factbase_schema_has_required_op() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let required = fb["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "op"));
    }

    #[test]
    fn test_factbase_schema_op_enum_values() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let ops = fb["inputSchema"]["properties"]["op"]["enum"]
            .as_array()
            .unwrap();
        let op_strs: Vec<&str> = ops.iter().filter_map(|v| v.as_str()).collect();

        let expected = [
            "get_entity",
            "list",
            "perspective",
            "create",
            "update",
            "delete",
            "bulk_create",
            "scan",
            "check",
            "detect_links",
            "review_queue",
            "answer",
            "deferred",
            "organize",
            "links",
            "fact_pairs",
            "embeddings",
            "authoring_guide",
            "doctor",
            "status",
        ];
        for op in &expected {
            assert!(op_strs.contains(op), "missing op: {op}");
        }
        assert_eq!(op_strs.len(), expected.len(), "unexpected extra ops");
    }

    #[test]
    fn test_factbase_description_is_compact() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        let lines: Vec<&str> = desc.lines().collect();
        assert!(
            lines.len() <= 15,
            "factbase description should be <=15 lines, got {}",
            lines.len()
        );
        assert!(
            desc.contains("op="),
            "factbase description should mention op="
        );
    }

    #[test]
    fn test_search_description_is_compact() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let s = tools.iter().find(|t| t["name"] == "search").unwrap();
        let desc = s["description"].as_str().unwrap();
        let lines: Vec<&str> = desc.lines().collect();
        assert!(
            lines.len() <= 15,
            "search description should be <=15 lines, got {}",
            lines.len()
        );
    }

    #[test]
    fn test_all_descriptions_under_20_lines() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            let desc = tool["description"].as_str().unwrap();
            let lines: Vec<&str> = desc.lines().collect();
            assert!(
                lines.len() <= 21,
                "{name} description should be <=21 lines, got {}",
                lines.len()
            );
        }
    }

    #[test]
    fn test_factbase_schema_has_doc_type_not_type_for_filtering() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let props = fb["inputSchema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("doc_type"), "should have doc_type param");
    }

    #[test]
    fn test_search_schema_has_mode() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let mode = &search["inputSchema"]["properties"]["mode"];
        let mode_enum = mode["enum"].as_array().unwrap();
        let values: Vec<&str> = mode_enum.iter().filter_map(|v| v.as_str()).collect();
        assert!(values.contains(&"semantic"));
        assert!(values.contains(&"content"));
    }

    #[test]
    fn test_legacy_tool_names_covers_active_tools() {
        let names = legacy_tool_names();
        assert!(names.contains(&"search_knowledge"));
        assert!(names.contains(&"scan_repository"));
        assert!(names.contains(&"check_repository"));
        assert!(names.contains(&"get_entity"));
        assert!(names.contains(&"create_document"));
        assert!(names.contains(&"get_fact_pairs"));
        // Removed tools should NOT be in legacy list
        assert!(!names.contains(&"list_repositories"));
        assert!(!names.contains(&"init_repository"));
        assert!(!names.contains(&"search_content"));
        assert!(!names.contains(&"migrate_links"));
    }

    #[test]
    fn test_removed_ops_have_messages() {
        let removed = removed_op_messages();
        let ops: Vec<&str> = removed.iter().map(|(op, _)| *op).collect();
        assert!(ops.contains(&"repos"));
        assert!(ops.contains(&"init"));
        assert!(ops.contains(&"search_content"));
    }

    #[test]
    fn test_removed_legacy_tools_have_messages() {
        let removed = removed_legacy_tool_messages();
        let names: Vec<&str> = removed.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&"list_repositories"));
        assert!(names.contains(&"migrate_links"));
        // init_repository is no longer removed — it's available via factbase(op='init_repository')
        assert!(!names.contains(&"init_repository"));
    }

    #[test]
    fn test_tools_list_unique_names() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len());
    }

    #[test]
    fn test_search_schema_has_temporal_params() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let props = search["inputSchema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("as_of"));
        assert!(props.contains_key("during"));
        assert!(props.contains_key("exclude_unknown"));
        assert!(props.contains_key("boost_recent"));
    }

    #[test]
    fn test_factbase_schema_has_scan_params() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let props = fb["inputSchema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("force_reindex"));
        assert!(props.contains_key("time_budget_secs"));
        assert!(props.contains_key("resume"));
    }

    #[test]
    fn test_workflow_schema_has_operating_principle() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("OPERATING PRINCIPLE"),
            "workflow description should have OPERATING PRINCIPLE at top level"
        );
        assert!(
            desc.starts_with("Guided multi-step workflows")
                && desc.contains("OPERATING PRINCIPLE"),
            "OPERATING PRINCIPLE should appear near the top of the description"
        );
        assert!(
            desc.contains("correct/transition") && desc.contains("IMMEDIATELY"),
            "operating principle should say correct/transition routes immediately"
        );
        assert!(
            desc.contains("add/ingest") && desc.contains("search the KB first"),
            "operating principle should say add/ingest searches first for dedup"
        );
        assert!(
            desc.contains("ask ONE focused question"),
            "operating principle should handle ambiguous entity case"
        );
        // Principle must appear before routing rules
        let principle_pos = desc.find("OPERATING PRINCIPLE").unwrap();
        let routing_pos = desc.find("Routing rules").unwrap();
        assert!(
            principle_pos < routing_pos,
            "OPERATING PRINCIPLE must appear before routing rules"
        );
    }

    #[test]
    fn test_workflow_schema_has_variant_param() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let props = &wf["inputSchema"]["properties"];
        assert!(props.get("variant").is_some());
        let variant_enum = props["variant"]["enum"].as_array().unwrap();
        let values: Vec<&str> = variant_enum.iter().filter_map(|v| v.as_str()).collect();
        assert!(values.contains(&"baseline"));
        assert!(values.contains(&"type_evidence"));
        assert!(values.contains(&"research_batch"));
    }

    #[test]
    fn test_workflow_description_is_compact() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        let lines: Vec<&str> = desc.lines().collect();
        assert!(
            lines.len() <= 21,
            "workflow description should be <=21 lines, got {}",
            lines.len()
        );
        assert!(desc.contains("create"));
        assert!(desc.contains("maintain"));
    }

    #[test]
    fn test_workflow_description_has_clarification_instruction() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("CLARIFICATION") || desc.contains("clarifying question"),
            "workflow description should include clarification instruction"
        );
        assert!(
            desc.contains("ONE") || desc.contains("one"),
            "workflow description should say to ask only one question"
        );
        assert!(
            desc.contains("reasonable default"),
            "workflow description should say not to ask when a reasonable default exists"
        );
    }

    #[test]
    fn test_workflow_schema_has_decision_rule() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("was the old information ever actually true"),
            "workflow description should include correct vs transition decision rule"
        );
        assert!(desc.contains("NO"), "should have NO branch");
        assert!(desc.contains("YES"), "should have YES branch");
    }

    #[test]
    fn test_workflow_schema_has_user_override() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("ALWAYS use that workflow"),
            "workflow description should tell agent to respect explicit user workflow choice"
        );
    }

    #[test]
    fn test_workflow_schema_has_call_immediately_guidance() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("CALL IMMEDIATELY") || desc.contains("IMMEDIATELY"),
            "workflow description should instruct agent to call correct/transition/refresh immediately"
        );
        assert!(
            desc.contains("FIRST action") || desc.contains("FIRST tool call"),
            "workflow description should say to call as first action"
        );
        assert!(
            desc.contains("Do NOT search") || desc.contains("do NOT search"),
            "workflow description should warn against searching first"
        );
    }

    #[test]
    fn test_factbase_schema_has_workflow_first_guidance() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(
            desc.contains("workflow()"),
            "factbase description should direct agents to use workflow() for multi-step operations"
        );
    }

    #[test]
    fn test_workflow_description_routes_scan_to_maintain() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("scan") && desc.contains("maintain"),
            "workflow description should route 'scan' to maintain"
        );
        assert!(
            desc.contains("index") || desc.contains("reindex"),
            "workflow description should route 'index'/'reindex' to maintain"
        );
    }

    #[test]
    fn test_workflow_schema_has_kb_priority_guidance() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("KB IS SOURCE OF TRUTH") || desc.contains("source of truth"),
            "workflow description should assert KB is source of truth"
        );
        assert!(
            desc.contains("training data"),
            "workflow description should warn against answering from training data"
        );
        assert!(
            desc.contains("web_search"),
            "workflow description should warn against using web_search instead of KB"
        );
    }

    #[test]
    fn test_factbase_schema_has_kb_priority_guidance() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(
            desc.contains("KB IS SOURCE OF TRUTH") || desc.contains("source of truth"),
            "factbase description should assert KB is source of truth"
        );
        assert!(
            desc.contains("training data"),
            "factbase description should warn against answering from training data"
        );
        assert!(
            desc.contains("web_search"),
            "factbase description should warn against using web_search instead of KB"
        );
    }

    #[test]
    fn test_workflow_description_routes_add_note_to_correct() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("add vs correct"),
            "workflow description should distinguish add vs correct"
        );
        assert!(
            desc.contains("Add a note to X") || desc.contains("add a note"),
            "workflow description should route 'add a note to X' to correct"
        );
        assert!(
            desc.contains("correct") && desc.contains("existing entities"),
            "workflow description should say correct is for existing entities"
        );
    }

    #[test]
    fn test_factbase_scan_op_notes_maintain_preference() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(
            desc.contains("workflow(maintain)"),
            "factbase scan op description should note workflow(maintain) preference"
        );
    }


    #[test]
    fn test_schema_override_no_repo_path() {
        // Without repo path, should use defaults
        let result = tools_list_with_overrides(None);
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let desc = search["description"].as_str().unwrap();
        assert!(desc.contains("Search the factbase"));
    }

    #[test]
    fn test_schema_override_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = tools_list_with_overrides(Some(dir.path()));
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let desc = search["description"].as_str().unwrap();
        assert!(desc.contains("Search the factbase"));
    }

    #[test]
    fn test_schema_override_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let schema_dir = dir.path().join(".factbase/schema");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(schema_dir.join("search.md"), "Custom search description").unwrap();
        let result = tools_list_with_overrides(Some(dir.path()));
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        assert_eq!(
            search["description"].as_str().unwrap(),
            "Custom search description"
        );
    }

    #[test]
    fn test_schema_override_empty_file_uses_default() {
        let dir = tempfile::tempdir().unwrap();
        let schema_dir = dir.path().join(".factbase/schema");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(schema_dir.join("search.md"), "  \n  ").unwrap();
        let result = tools_list_with_overrides(Some(dir.path()));
        let tools = result["tools"].as_array().unwrap();
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        let desc = search["description"].as_str().unwrap();
        assert!(
            desc.contains("Search the factbase"),
            "Empty file should fall back to default"
        );
    }

    #[test]
    fn test_schema_override_partial() {
        // Override only one tool, others keep defaults
        let dir = tempfile::tempdir().unwrap();
        let schema_dir = dir.path().join(".factbase/schema");
        std::fs::create_dir_all(&schema_dir).unwrap();
        std::fs::write(schema_dir.join("workflow.md"), "Custom workflow desc").unwrap();
        let result = tools_list_with_overrides(Some(dir.path()));
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        assert_eq!(wf["description"].as_str().unwrap(), "Custom workflow desc");
        let fb = tools.iter().find(|t| t["name"] == "factbase").unwrap();
        assert!(
            fb["description"].as_str().unwrap().contains("op="),
            "factbase should keep default"
        );
    }

    // ── Routing Benchmark Suite (v12) ────────────────────────────────────────
    //
    // Standard test set: 10 prompts (P1–P10) + 5 clarification prompts (P11–P15).
    // These define the expected routing decisions for regression testing.
    //
    // Harness instruction (must be included when running live benchmarks):
    //   "If you need to ask a clarifying question, output 'ASK: <question>' and
    //    stop. Otherwise call the appropriate tool."
    //
    // Results baseline (v12): all 3 models (Opus, Sonnet, Haiku) scored 15/15.
    // See docs/workflow-routing-final-v12.md for full results.

    /// Routing benchmark prompt definitions with expected outcomes.
    /// Each entry: (prompt, expected_routing, category)
    #[cfg(test)]
    pub fn routing_benchmark_prompts() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            // Standard 10 (P1–P10) — from v6 suite, all models 10/10
            (
                "I think there are some mistakes in how we've recorded the early church",
                "maintain",
                "standard",
            ),
            (
                "The apostle Paul didn't write Ephesians — modern scholars attribute it to a student of Paul",
                "correct",
                "standard",
            ),
            (
                "Refresh the KB with the latest Dead Sea Scrolls scholarship",
                "refresh",
                "standard",
            ),
            (
                "We need to update our records — the Gospel of Mark was actually written AFTER Luke, not before",
                "correct",
                "standard",
            ),
            (
                "Can you help me understand what the KB says about baptism?",
                "search",
                "standard",
            ),
            (
                "I want to reorganize the KB so that all epistles are grouped together",
                "organize",
                "standard",
            ),
            (
                "The KB needs updating — there's been a lot of new work on the historical Paul recently",
                "refresh",
                "standard",
            ),
            (
                "I think we should correct the record on the Synoptic Problem — our KB has it wrong",
                "correct",
                "standard",
            ),
            (
                "Can you check whether our Dead Sea Scrolls content is accurate and complete?",
                "maintain",
                "standard",
            ),
            (
                "The Gospel of John was written by John the Apostle — but I've seen this disputed. What does our KB say?",
                "search",
                "standard",
            ),
            // Clarification 5 (P11–P15) — from v11 suite, Sonnet 5/5
            // These prompts have no clear referent; correct response is ASK.
            ("Fix John", "ASK", "clarification"),
            ("Update it", "ASK", "clarification"),
            ("That needs to be corrected", "ASK", "clarification"),
            ("Fix the entry", "ASK", "clarification"),
            // P15: ASK is preferred; workflow(maintain) is also defensible.
            ("The dates are wrong", "ASK", "clarification"),
        ]
    }

    #[test]
    fn test_routing_benchmark_has_15_prompts() {
        let prompts = routing_benchmark_prompts();
        assert_eq!(prompts.len(), 15, "benchmark suite must have 15 prompts");
    }

    #[test]
    fn test_routing_benchmark_has_10_standard_and_5_clarification() {
        let prompts = routing_benchmark_prompts();
        let standard: Vec<_> = prompts.iter().filter(|p| p.2 == "standard").collect();
        let clarification: Vec<_> = prompts.iter().filter(|p| p.2 == "clarification").collect();
        assert_eq!(standard.len(), 10, "must have 10 standard prompts");
        assert_eq!(clarification.len(), 5, "must have 5 clarification prompts");
    }

    #[test]
    fn test_routing_benchmark_clarification_prompts_expect_ask() {
        let prompts = routing_benchmark_prompts();
        for (prompt, expected, category) in &prompts {
            if *category == "clarification" {
                assert_eq!(
                    *expected, "ASK",
                    "clarification prompt '{prompt}' must expect ASK"
                );
            }
        }
    }

    #[test]
    fn test_routing_benchmark_standard_prompts_cover_all_workflows() {
        let prompts = routing_benchmark_prompts();
        let standard_routes: Vec<&str> = prompts
            .iter()
            .filter(|p| p.2 == "standard")
            .map(|p| p.1)
            .collect();
        // All major routing targets must appear in the standard suite
        for target in &["maintain", "correct", "refresh", "search", "organize"] {
            assert!(
                standard_routes.contains(target),
                "standard suite must include a '{target}' routing case"
            );
        }
    }

    #[test]
    fn test_routing_benchmark_harness_instruction_in_workflow_schema() {
        // The workflow tool description must contain the clarification instruction
        // that the harness injects: agents should output ASK for ambiguous prompts.
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("clarifying question") || desc.contains("CLARIFICATION"),
            "workflow schema must include clarification instruction for ambiguous prompts"
        );
    }

    #[test]
    fn test_workflow_description_has_routing_rules_section() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("Routing rules"),
            "workflow description should have explicit 'Routing rules' section"
        );
    }

    #[test]
    fn test_workflow_description_routes_build_to_create() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("build") || desc.contains("new KB"),
            "workflow description should route 'build'/'new KB' to create"
        );
        assert!(
            desc.contains("workflow(create)"),
            "workflow description should show workflow(create) as routing target"
        );
    }

    #[test]
    fn test_workflow_description_routes_change_over_time_to_transition() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("change that happened over time") || desc.contains("happened over time"),
            "workflow description should route 'change that happened over time' to transition"
        );
        assert!(
            desc.contains("workflow(transition)") || desc.contains("transition"),
            "workflow description should show transition as routing target"
        );
    }

    #[test]
    fn test_workflow_description_routes_no_entity_to_ask() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("no entity named") || desc.contains("no entity"),
            "workflow description should route 'no entity named' to ASK"
        );
        assert!(
            desc.contains("ASK"),
            "workflow description should say ASK when no entity is named"
        );
    }
}
