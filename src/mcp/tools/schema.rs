//! MCP tool schema definitions.
//!
//! Contains the JSON schema for all 16 MCP tools exposed by factbase.

use serde_json::Value;

/// Returns the complete list of available MCP tools with their schemas.
///
/// This is returned in response to `tools/list` requests.
pub fn tools_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "search_knowledge",
                "description": "Search factbase by meaning or title. Use this when the user asks to look up, find, or search for something in factbase. For multi-step tasks like 'research X and add to factbase' or 'fix the review queue', use workflow_start instead.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Semantic search query" },
                        "title_filter": { "type": "string", "description": "Filter by title (partial match)" },
                        "limit": { "type": "integer", "description": "Max results (default: 10)" },
                        "offset": { "type": "integer", "description": "Skip results for pagination (default: 0)" },
                        "doc_type": { "type": "string", "description": "Filter by document type" },
                        "repo": { "type": "string", "description": "Filter by repository" },
                        "as_of": { "type": "string", "description": "Filter to facts valid at date (YYYY, YYYY-MM, or YYYY-MM-DD)" },
                        "during": { "type": "string", "description": "Filter to facts valid during range (YYYY..YYYY or YYYY-MM..YYYY-MM)" },
                        "exclude_unknown": { "type": "boolean", "description": "Exclude facts with @t[?] tags or no temporal tags (default: false)" }
                    }
                }
            },
            {
                "name": "search_temporal",
                "description": "Search factbase with temporal filtering. Find facts that were valid at a specific date or during a date range. Returns temporal metadata (tag types, date ranges, confidence) for each result.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Semantic search query" },
                        "as_of": { "type": "string", "description": "Filter to facts valid at date (YYYY, YYYY-MM, or YYYY-MM-DD)" },
                        "during": { "type": "string", "description": "Filter to facts valid during range (YYYY..YYYY or YYYY-MM..YYYY-MM)" },
                        "exclude_unknown": { "type": "boolean", "description": "Exclude facts with @t[?] tags or no temporal tags (default: false)" },
                        "boost_recent": { "type": "boolean", "description": "Boost ranking of facts with recent @t[~...] dates (default: false)" },
                        "limit": { "type": "integer", "description": "Max results (default: 10)" },
                        "doc_type": { "type": "string", "description": "Filter by document type" },
                        "repo": { "type": "string", "description": "Filter by repository" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "get_entity",
                "description": "Get a factbase document by ID, including its full content and links to/from other documents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" },
                        "include_preview": { "type": "boolean", "description": "Include 500-char content preview" },
                        "max_content_length": { "type": "integer", "description": "Truncate content to this length (0 = no truncation)" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "list_entities",
                "description": "List documents in factbase with optional type, repo, or title filters.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_type": { "type": "string", "description": "Filter by type" },
                        "repo": { "type": "string", "description": "Filter by repository" },
                        "title_filter": { "type": "string", "description": "Filter by title pattern (SQL LIKE, use % for wildcards)" },
                        "limit": { "type": "integer", "description": "Max results" }
                    }
                }
            },
            {
                "name": "list_repositories",
                "description": "List all factbase repositories."
            },
            {
                "name": "get_document_stats",
                "description": "Get quick statistics for a factbase document including temporal coverage, source coverage, link counts, and review queue status. Lighter weight than get_entity (no content returned).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "get_perspective",
                "description": "Get the factbase repository's perspective — its organization, focus area, and quality policies.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Repository ID" }
                    },
                    "required": ["repo"]
                }
            },
            {
                "name": "create_document",
                "description": "Create a new document in factbase.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Repository ID" },
                        "path": { "type": "string", "description": "Relative file path" },
                        "title": { "type": "string", "description": "Document title" },
                        "content": { "type": "string", "description": "Document content" }
                    },
                    "required": ["repo", "path", "title"]
                }
            },
            {
                "name": "update_document",
                "description": "Update an existing factbase document's title or content.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" },
                        "title": { "type": "string", "description": "New title" },
                        "content": { "type": "string", "description": "New content" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "delete_document",
                "description": "Delete a factbase document by ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "bulk_create_documents",
                "description": "Create multiple factbase documents in one operation (atomic: all succeed or all fail).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Repository ID" },
                        "documents": {
                            "type": "array",
                            "description": "Array of documents to create (max 100)",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "path": { "type": "string", "description": "Relative file path" },
                                    "title": { "type": "string", "description": "Document title" },
                                    "content": { "type": "string", "description": "Document content" }
                                },
                                "required": ["path", "title"]
                            }
                        }
                    },
                    "required": ["repo", "documents"]
                }
            },
            {
                "name": "search_content",
                "description": "Search factbase for exact text matches (like grep). No embeddings needed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Text pattern to search for (case-insensitive)" },
                        "limit": { "type": "integer", "description": "Max results (default: 10)" },
                        "doc_type": { "type": "string", "description": "Filter by document type" },
                        "repo": { "type": "string", "description": "Filter by repository" },
                        "context": { "type": "integer", "description": "Number of context lines before/after each match (default: 0)" }
                    },
                    "required": ["pattern"]
                }
            },
            {
                "name": "get_review_queue",
                "description": "List pending review questions across factbase documents. Use include_context=true to get surrounding lines for each question.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Filter by repository ID" },
                        "doc_id": { "type": "string", "description": "Filter by document ID" },
                        "type": { "type": "string", "description": "Filter by question type (temporal, conflict, missing, ambiguous, stale, duplicate)" },
                        "include_context": { "type": "boolean", "description": "Include surrounding lines from the document for each question (default: false)" }
                    }
                }
            },
            {
                "name": "answer_question",
                "description": "Answer a review question in factbase. Modifies the document to record the answer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_id": { "type": "string", "description": "Document ID containing the question" },
                        "question_index": { "type": "integer", "description": "0-based index of the question in the Review Queue" },
                        "answer": { "type": "string", "description": "Answer text to add" }
                    },
                    "required": ["doc_id", "question_index", "answer"]
                }
            },
            {
                "name": "generate_questions",
                "description": "Generate review questions for a factbase document. Use dry_run=true to preview without modifying the file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_id": { "type": "string", "description": "Document ID to generate questions for" },
                        "dry_run": { "type": "boolean", "description": "If true, return questions without modifying the file (default: false)" }
                    },
                    "required": ["doc_id"]
                }
            },
            {
                "name": "bulk_answer_questions",
                "description": "Answer multiple factbase review questions at once (atomic: all succeed or all fail).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "answers": {
                            "type": "array",
                            "description": "Array of answers to apply (max 50)",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "doc_id": { "type": "string", "description": "Document ID containing the question" },
                                    "question_index": { "type": "integer", "description": "0-based index of the question in the Review Queue" },
                                    "answer": { "type": "string", "description": "Answer text to add" }
                                },
                                "required": ["doc_id", "question_index", "answer"]
                            }
                        }
                    },
                    "required": ["answers"]
                }
            },
            {
                "name": "workflow_start",
                "description": "Start a guided factbase workflow. Each step tells you exactly what to do and which tool to call next.\n\nUse this when the user says things like:\n- 'fix the review queue' or 'resolve factbase issues' → workflow='resolve'\n- 'research [topic]' or 'add [person/company] to factbase' → workflow='ingest', topic='...'\n- 'improve the data' or 'fill in gaps in factbase' or 'enrich [type] documents' → workflow='enrich'\n- 'what can factbase do' or 'what workflows are available' → workflow='list'\n\nAfter each step, call workflow_next to get the next instruction.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workflow": { "type": "string", "description": "Workflow name: 'resolve', 'ingest', 'enrich', or 'list'" },
                        "topic": { "type": "string", "description": "For ingest: what to research (e.g., a person's name, company, project)" },
                        "doc_type": { "type": "string", "description": "For enrich: document type to focus on (e.g., 'person', 'company')" },
                        "repo": { "type": "string", "description": "Repository ID (optional, uses first repo if omitted)" }
                    },
                    "required": ["workflow"]
                }
            },
            {
                "name": "workflow_next",
                "description": "Get the next step in an active workflow. Call this after completing the current step. The response tells you what to do next and which tool to call.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workflow": { "type": "string", "description": "Workflow name (same as workflow_start)" },
                        "step": { "type": "integer", "description": "Step number to advance to (default: 2)" },
                        "topic": { "type": "string", "description": "For ingest: the topic being researched" },
                        "doc_type": { "type": "string", "description": "For enrich: document type being enriched" },
                        "repo": { "type": "string", "description": "Repository ID (optional)" }
                    },
                    "required": ["workflow"]
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_list_returns_all_tools() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools should be array");

        assert_eq!(tools.len(), 18, "should have 18 tools");

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"search_knowledge"));
        assert!(names.contains(&"search_temporal"));
        assert!(names.contains(&"get_entity"));
        assert!(names.contains(&"get_document_stats"));
        assert!(names.contains(&"get_review_queue"));
        assert!(names.contains(&"answer_question"));
        assert!(names.contains(&"generate_questions"));
        assert!(names.contains(&"bulk_answer_questions"));
        assert!(names.contains(&"list_entities"));
        assert!(names.contains(&"list_repositories"));
        assert!(names.contains(&"get_perspective"));
        assert!(names.contains(&"create_document"));
        assert!(names.contains(&"update_document"));
        assert!(names.contains(&"delete_document"));
        assert!(names.contains(&"bulk_create_documents"));
        assert!(names.contains(&"search_content"));

        // Verify tools with required params have inputSchema
        for tool in tools {
            assert!(tool["name"].is_string(), "tool should have name");
            assert!(
                tool["description"].is_string(),
                "tool should have description"
            );
        }

        // Verify search_knowledge has title_filter param
        let search = tools
            .iter()
            .find(|t| t["name"] == "search_knowledge")
            .expect("search_knowledge tool should exist");
        let props = search["inputSchema"]["properties"]
            .as_object()
            .expect("should have properties");
        assert!(props.contains_key("query"));
        assert!(props.contains_key("title_filter"));
        assert!(props.contains_key("as_of"));
        assert!(props.contains_key("during"));
        assert!(props.contains_key("exclude_unknown"));
    }

    #[test]
    fn test_tools_list_required_params() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        // Tools with required params should have them in schema
        let get_entity = tools.iter().find(|t| t["name"] == "get_entity").unwrap();
        let required = get_entity["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|v| v == "id"));

        let get_perspective = tools
            .iter()
            .find(|t| t["name"] == "get_perspective")
            .unwrap();
        let required = get_perspective["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|v| v == "repo"));

        let create_doc = tools
            .iter()
            .find(|t| t["name"] == "create_document")
            .unwrap();
        let required = create_doc["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|v| v == "repo"));
        assert!(required.iter().any(|v| v == "path"));
        assert!(required.iter().any(|v| v == "title"));
    }

    #[test]
    fn test_tools_list_search_temporal_schema() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let search_temporal = tools
            .iter()
            .find(|t| t["name"] == "search_temporal")
            .unwrap();
        let props = search_temporal["inputSchema"]["properties"]
            .as_object()
            .expect("properties");

        // Should have temporal-specific params
        assert!(props.contains_key("as_of"));
        assert!(props.contains_key("during"));
        assert!(props.contains_key("exclude_unknown"));
        assert!(props.contains_key("boost_recent"));

        // query should be required
        let required = search_temporal["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|v| v == "query"));
    }

    #[test]
    fn test_tools_list_bulk_create_schema() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let bulk_create = tools
            .iter()
            .find(|t| t["name"] == "bulk_create_documents")
            .unwrap();
        let props = bulk_create["inputSchema"]["properties"]
            .as_object()
            .expect("properties");

        assert!(props.contains_key("repo"));
        assert!(props.contains_key("documents"));

        // documents should be array type
        let docs_schema = &props["documents"];
        assert_eq!(docs_schema["type"], "array");
    }

    #[test]
    fn test_tools_list_unique_names() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        let mut unique_names = names.clone();
        unique_names.sort();
        unique_names.dedup();

        assert_eq!(
            names.len(),
            unique_names.len(),
            "all tool names should be unique"
        );
    }

    #[test]
    fn test_search_knowledge_no_required_fields() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let search = tools
            .iter()
            .find(|t| t["name"] == "search_knowledge")
            .unwrap();

        // search_knowledge has no required fields - all params are optional
        assert!(
            search["inputSchema"]["required"].is_null(),
            "search_knowledge should have no required fields"
        );
    }

    #[test]
    fn test_bulk_create_max_limit_documented() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let bulk_create = tools
            .iter()
            .find(|t| t["name"] == "bulk_create_documents")
            .unwrap();
        let docs_desc = bulk_create["inputSchema"]["properties"]["documents"]["description"]
            .as_str()
            .expect("documents description");

        assert!(
            docs_desc.contains("max 100"),
            "bulk_create_documents should document max 100 limit"
        );
    }

    #[test]
    fn test_delete_document_required_id() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let delete = tools
            .iter()
            .find(|t| t["name"] == "delete_document")
            .unwrap();
        let required = delete["inputSchema"]["required"]
            .as_array()
            .expect("required array");

        assert!(
            required.iter().any(|v| v == "id"),
            "delete_document should require id"
        );
    }
}
