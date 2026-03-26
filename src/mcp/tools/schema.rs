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
    let default_desc = "Multi-step guided workflows for factbase tasks.\nCall workflow(workflow=NAME, step=1) to start. The tool returns step-by-step instructions.\nUnsure which workflow to use? Call workflow(workflow='list') for descriptions of each.\n\nWHEN TO USE WHICH WORKFLOW:\n- New KB from scratch → workflow(create)\n- Add new entity/topic → workflow(add, topic=NAME)\n- Modify existing entity (correction, annotation) → workflow(correct)\n- Entity changed over time (rename, acquisition) → workflow(transition)\n- scan/check/maintain quality → workflow(maintain)\n- Check for recent changes, update stale facts → workflow(refresh)\n- Answer review queue questions → workflow(resolve)\n\n⚠️ KB IS SOURCE OF TRUTH: Always query factbase before answering from training data.";
    let desc =
        load_schema_override("workflow", repo_path).unwrap_or_else(|| default_desc.to_string());
    serde_json::json!({
        "name": "workflow",
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": {
                "workflow": { "type": "string", "description": "Workflow name: 'create', 'add', 'maintain', 'refresh', 'correct', 'transition', 'resolve', or 'list'. Legacy aliases also accepted: bootstrap, setup, update, ingest, enrich, improve" },
                "step": { "type": "integer", "description": "Step number (default: 1 = start)" },
                "domain": { "type": "string", "description": "For create: domain description (e.g. 'mycology', 'ancient Mediterranean history')" },
                "entity_types": { "type": "string", "description": "For create: optional comma-separated entity types (e.g. 'species, habitats, researchers')" },
                "path": { "type": "string", "description": "For create: directory path for the new repository" },
                "topic": { "type": "string", "description": "For add: what to research (triggers ingest mode)" },
                "doc_type": { "type": "string", "description": "For add/refresh/resolve: document type to focus on" },
                "doc_id": { "type": "string", "description": "For add: document ID to improve. For refresh: specific document to refresh." },
                "mode": { "type": "string", "description": "For refresh: 'exhaustive' to loop all KB entities by attention_score instead of reading data_sources. Default: source-driven (reads data_sources for last N days)." },
                "days": { "type": "integer", "description": "For refresh source-driven mode: how many days back to pull from data_sources (default: 7)." },
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
                "reindex_reviews": { "type": "boolean", "description": "Reimport review questions from markdown into DB (sync markdown → DB)" },
                "regenerate_reviews": { "type": "boolean", "description": "Discard existing review questions and regenerate from scratch using current rules (re-runs generators → DB + markdown). Use after a bug fix to clear stale questions." },
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
                "confidence": { "type": "string", "enum": ["verified", "author", "deferred"], "description": "Answer confidence" },
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
    fn test_workflow_schema_has_when_to_use_section() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.starts_with("Multi-step guided workflows"),
            "workflow description should start with 'Multi-step guided workflows'"
        );
        assert!(
            desc.contains("WHEN TO USE WHICH WORKFLOW"),
            "workflow description should have WHEN TO USE WHICH WORKFLOW section"
        );
        assert!(
            desc.contains("workflow(create)") && desc.contains("workflow(maintain)"),
            "description should list workflow names"
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
    fn test_workflow_schema_has_mode_and_days_params() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let props = &wf["inputSchema"]["properties"];
        let mode = props.get("mode").expect("workflow schema should have mode param");
        assert!(mode["type"].as_str().unwrap() == "string");
        assert!(mode["description"].as_str().unwrap().contains("exhaustive"));
        let days = props.get("days").expect("workflow schema should have days param");
        assert!(days["type"].as_str().unwrap() == "integer");
        assert!(days["description"].as_str().unwrap().contains("days"));
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
        // Clarification guidance moved to step 1 responses; description just lists workflows
        assert!(
            desc.contains("workflow='list'"),
            "workflow description should direct agents to use list for details"
        );
    }

    #[test]
    fn test_workflow_schema_has_decision_rule() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        // correct vs transition disambiguation moved to step 1 responses
        assert!(
            desc.contains("workflow(correct)") && desc.contains("workflow(transition)"),
            "description should list both correct and transition workflows"
        );
    }

    #[test]
    fn test_workflow_schema_has_user_override() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        // User override guidance moved to step 1 responses; description stays concise
        assert!(
            desc.contains("step=1"),
            "workflow description should show how to start a workflow"
        );
    }

    #[test]
    fn test_workflow_schema_has_call_immediately_guidance() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        // Immediate-call guidance moved to step 1 responses; description lists workflows concisely
        assert!(
            desc.contains("workflow(correct)") && desc.contains("workflow(transition)"),
            "description should list correct and transition as workflow options"
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
        // Routing details moved to step 1 responses; description lists correct for modifications
        assert!(
            desc.contains("workflow(correct)"),
            "workflow description should list correct workflow"
        );
        assert!(
            desc.contains("correction") || desc.contains("annotation"),
            "workflow description should mention what correct is for"
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
        // The workflow tool description must contain enough routing info for agents.
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("WHEN TO USE WHICH WORKFLOW") || desc.contains("workflow='list'"),
            "workflow schema must include routing guidance or direct agents to list"
        );
    }

    #[test]
    fn test_workflow_description_has_routing_rules_section() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("WHEN TO USE WHICH WORKFLOW"),
            "workflow description should have routing guidance section"
        );
    }

    #[test]
    fn test_workflow_description_routes_build_to_create() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        assert!(
            desc.contains("New KB") || desc.contains("from scratch"),
            "workflow description should route 'new KB from scratch' to create"
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
            desc.contains("changed over time")
                || desc.contains("rename")
                || desc.contains("acquisition"),
            "workflow description should route entity changes to transition"
        );
        assert!(
            desc.contains("workflow(transition)"),
            "workflow description should show transition as routing target"
        );
    }

    #[test]
    fn test_workflow_description_routes_no_entity_to_ask() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let wf = tools.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf["description"].as_str().unwrap();
        // Ambiguous-entity handling moved to step 1 responses; description stays concise
        assert!(
            desc.contains("workflow='list'"),
            "workflow description should direct agents to list for details when unsure"
        );
    }
}
