//! MCP tool schema definitions.
//!
//! Exposes 2 tools: `workflow` (guided multi-step entry point) and `factbase`
//! (unified operations tool). Old individual tool names are kept as dispatch
//! aliases for backward compatibility but are not listed in the schema.

use serde_json::Value;

/// Returns the complete list of available MCP tools with their schemas.
///
/// This is returned in response to `tools/list` requests.
pub fn tools_list() -> Value {
    serde_json::json!({
        "tools": [
            search_schema(),
            workflow_schema(),
            factbase_schema(),
        ]
    })
}

/// Returns the list of old tool names that are kept as dispatch aliases.
/// These are NOT in the schema but still accepted by handle_tool_call.
#[cfg(test)]
pub fn legacy_tool_names() -> &'static [&'static str] {
    &[
        "search_knowledge",
        "get_entity", "list_entities", "get_perspective",
        "create_document", "update_document", "delete_document", "bulk_create_documents",
        "get_review_queue", "get_deferred_items", "answer_questions",
        "check_repository", "scan_repository", "detect_links",
        "get_authoring_guide",
        "organize_analyze", "organize",
        "embeddings_export", "embeddings_import", "embeddings_status",
        "get_link_suggestions", "store_links", "get_fact_pairs",
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
        ("list_repositories", "'list_repositories' removed. Use factbase(op='perspective') instead."),
        ("init_repository", "'init_repository' removed. Repositories auto-initialize on first scan."),
        ("search_content", "'search_content' removed. Use the 'search' tool with mode='content' instead."),
        ("migrate_links", "'migrate_links' removed. Link migration is no longer needed."),
    ]
}

fn search_schema() -> Value {
    serde_json::json!({
        "name": "search",
        "description": "Search the factbase. Returns entities with outgoing links.\nModes: semantic (default) or content (exact text/regex).\nFilters: doc_type, title_filter, as_of, during, exclude_unknown, boost_recent.",
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

fn workflow_schema() -> Value {
    serde_json::json!({
        "name": "workflow",
        "description": "Guided multi-step workflows for factbase tasks. workflow= to specify:\ncreate, add, maintain, refresh, correct, transition\nCall with step=1 to start. Use workflow='list' for details.\n⚠️ If IO/body errors from answer_questions, split into smaller batches.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workflow": { "type": "string", "description": "Workflow name: 'create', 'add', 'maintain', 'refresh', 'correct', 'transition', 'resolve', or 'list'. Legacy aliases also accepted: bootstrap, setup, update, ingest, enrich, improve" },
                "step": { "type": "integer", "description": "Step number (default: 1 = start)" },
                "domain": { "type": "string", "description": "For create: domain description (e.g. 'mycology', 'ancient Mediterranean history')" },
                "entity_types": { "type": "string", "description": "For create: optional comma-separated entity types (e.g. 'species, habitats, researchers')" },
                "path": { "type": "string", "description": "For create: directory path for the new repository" },
                "topic": { "type": "string", "description": "For add: what to research (triggers ingest mode)" },
                "doc_type": { "type": "string", "description": "For add/refresh: document type to focus on" },
                "doc_id": { "type": "string", "description": "For add: document ID to improve. For refresh: specific document to refresh." },
                "correction": { "type": "string", "description": "For correct: free text describing what is wrong and what the true fact is." },
                "change": { "type": "string", "description": "For transition: what changed — free text (e.g. 'XSOLIS renamed to PRIMA-X as of today')." },
                "effective_date": { "type": "string", "description": "For transition: when the change happened (ISO date). Defaults to today if omitted." },
                "nomenclature": { "type": "string", "description": "For transition step 2: how to reference the entity going forward (user's choice from the options presented)." },
                "source": { "type": "string", "description": "For correct/transition: optional citation (who said it, when)." },
                "question_type": { "type": "string", "description": "For resolve step 2: filter questions by type. Comma-separated for multiple types (e.g. 'temporal,ambiguous')." },
                "variant": { "type": "string", "enum": ["baseline", "type_evidence", "research_batch"], "description": "For resolve: prompt variant to use." },
                "cross_validate": { "type": "boolean", "description": "For maintain: include cross-document fact validation step (default: false)." },
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

fn factbase_schema() -> Value {
    serde_json::json!({
        "name": "factbase",
        "description": "Knowledge base operations. Use op= to specify:\n\nDOCUMENTS: get_entity(id), create(path,title,content), update(id,content), delete(id), bulk_create(documents[]), list(doc_type?,limit?)\nQUALITY: check(doc_id?), scan(time_budget_secs?), detect_links(time_budget_secs?)\nREVIEW: review_queue(doc_id?), answer(doc_id,question_index,answer), deferred()\nORGANIZE: organize(action=analyze|move|merge|split|delete|retype|execute_suggestions)\nLINKS: links(action=suggest|store), fact_pairs(min_similarity?)\nMETA: perspective(), authoring_guide(), embeddings(action=export|import|status)",
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
                        "authoring_guide"
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
        assert_eq!(tools.len(), 3, "should have exactly 3 tools: search + workflow + factbase");

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
            "get_entity", "list", "perspective",
            "create", "update", "delete", "bulk_create",
            "scan", "check", "detect_links",
            "review_queue", "answer", "deferred",
            "organize", "links", "fact_pairs", "embeddings",
            "authoring_guide",
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
        assert!(lines.len() <= 15, "factbase description should be <=15 lines, got {}", lines.len());
        assert!(desc.contains("op="), "factbase description should mention op=");
    }

    #[test]
    fn test_search_description_is_compact() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        let s = tools.iter().find(|t| t["name"] == "search").unwrap();
        let desc = s["description"].as_str().unwrap();
        let lines: Vec<&str> = desc.lines().collect();
        assert!(lines.len() <= 15, "search description should be <=15 lines, got {}", lines.len());
    }

    #[test]
    fn test_all_descriptions_under_15_lines() {
        let result = tools_list();
        let tools = result["tools"].as_array().unwrap();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            let desc = tool["description"].as_str().unwrap();
            let lines: Vec<&str> = desc.lines().collect();
            assert!(lines.len() <= 15, "{name} description should be <=15 lines, got {}", lines.len());
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
        assert!(names.contains(&"init_repository"));
        assert!(names.contains(&"migrate_links"));
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
        assert!(lines.len() <= 15, "workflow description should be <=15 lines, got {}", lines.len());
        assert!(desc.contains("create"));
        assert!(desc.contains("maintain"));
    }
}
