//! MCP tool schema definitions.
//!
//! Contains the JSON schema for all 25 MCP tools exposed by factbase.

use serde_json::Value;

/// Returns the complete list of available MCP tools with their schemas.
///
/// This is returned in response to `tools/list` requests.
pub fn tools_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "search_knowledge",
                "description": "Search factbase by meaning, title, or temporal range.\n\nTriggers: 'what do we know about X', 'find X', 'search for X', 'look up X', 'who is X', 'tell me about X'\n\nFor multi-step tasks like 'research X', 'update the factbase', or 'fix issues', use workflow instead.",
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
                        "exclude_unknown": { "type": "boolean", "description": "Exclude facts with @t[?] tags (default: false)" },
                        "boost_recent": { "type": "boolean", "description": "Boost ranking of recent @t[~...] dates and return temporal metadata (default: false)" }
                    }
                }
            },
            {
                "name": "get_entity",
                "description": "Get a document by ID. Returns full content and links by default, or just stats with detail='stats'.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" },
                        "detail": { "type": "string", "description": "'full' (default) for content+links, 'stats' for counts only" },
                        "include_preview": { "type": "boolean", "description": "Include 500-char content preview" },
                        "max_content_length": { "type": "integer", "description": "Truncate content to this length (0 = no truncation)" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "list_entities",
                "description": "List documents with optional type, repo, or title filters.",
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
                "description": "List all factbase repositories.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_perspective",
                "description": "Get repository context: organization, focus area, and quality policies.",
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
                "description": "Create a new document. Call get_authoring_guide first if unsure about format.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Repository ID" },
                        "path": { "type": "string", "description": "Relative file path" },
                        "title": { "type": "string", "description": "Document title" },
                        "content": { "type": "string", "description": "Document body (do NOT include # Title heading — it is added automatically from the title field)" }
                    },
                    "required": ["repo", "path", "title"]
                }
            },
            {
                "name": "update_document",
                "description": "Update a document's title or content.",
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
                "description": "Delete a document by ID.",
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
                "description": "Create multiple documents atomically (max 100).",
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
                                    "content": { "type": "string", "description": "Document body (do NOT include # Title heading — it is added automatically)" }
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
                "description": "Exact text search (like grep). No embeddings needed.",
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
                "description": "List review questions. Defaults to unanswered only. Use status filter to see answered, deferred, or all. Each question includes question_index for use with answer_questions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Filter by repository ID" },
                        "doc_id": { "type": "string", "description": "Filter by document ID" },
                        "type": { "type": "string", "description": "Filter by question type (temporal, conflict, missing, ambiguous, stale, duplicate, corruption, precision)" },
                        "status": { "type": "string", "description": "Filter by status: 'unanswered' (default), 'answered', 'deferred', 'all'" },
                        "limit": { "type": "integer", "description": "Max questions to return (default: 10)" },
                        "offset": { "type": "integer", "description": "Skip this many questions for pagination (default: 0)" }
                    }
                }
            },
            {
                "name": "answer_questions",
                "description": "Answer or defer review questions. For a single question: provide doc_id, question_index, answer. For bulk: provide answers array. Prefix with 'defer:' to leave in queue with a note. Use confidence field to distinguish verified (applied) from believed (stays in queue for human review).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_id": { "type": "string", "description": "Document ID (single mode)" },
                        "question_index": { "type": "integer", "description": "0-based question index (single mode)" },
                        "answer": { "type": "string", "description": "Answer text or 'defer: <reason>' (single mode)" },
                        "confidence": { "type": "string", "enum": ["verified", "believed"], "description": "Confidence level. 'verified' (default): confirmed via external source, will be applied. 'believed': confident from training data but no external confirmation, stays in queue for human review." },
                        "answers": {
                            "type": "array",
                            "description": "Array of answers for bulk mode (max 50)",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "doc_id": { "type": "string" },
                                    "question_index": { "type": "integer" },
                                    "answer": { "type": "string" },
                                    "confidence": { "type": "string", "enum": ["verified", "believed"], "description": "'verified' (default) or 'believed' (stays in queue)" }
                                },
                                "required": ["doc_id", "question_index", "answer"]
                            }
                        }
                    }
                }
            },
            {
                "name": "check_repository",
                "description": "Run quality checks on a repository. Requires a `mode` parameter to select what to check. Each mode is time-boxed and WILL return `continue: true` with a `resume` token for non-trivial repositories — you MUST call again passing the resume token until done.\n\nModes:\n- 'questions': Per-document quality checks (stale, temporal, source, missing). Pages via resume token.\n- 'cross_validate': Cross-document fact comparison via pre-computed embeddings. Pages via resume token.\n- 'discover': Entity suggestions + vocabulary extraction. Usually completes in one call.\n- 'embeddings': Generate fact-level embeddings for cross-validation. Pages via resume token.\n\nIf doc_id is provided, checks just that document (ignores mode).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["questions", "cross_validate", "discover", "embeddings"], "description": "Required. 'questions' for per-doc quality checks, 'cross_validate' for cross-doc fact comparison, 'discover' for entity suggestions + vocabulary, 'embeddings' for fact embedding generation." },
                        "repo": { "type": "string", "description": "Repository ID (optional)" },
                        "doc_id": { "type": "string", "description": "Check a single document (optional, checks all if omitted). Ignores mode when set." },
                        "dry_run": { "type": "boolean", "description": "Preview without modifying files (default: false)" },
                        "deep_check": { "type": "boolean", "description": "Deprecated: accepted but ignored. Use mode='cross_validate' instead." },
                        "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600, default from config). Operation returns progress and asks to be called again if budget is exceeded." },
                        "resume": { "type": "string", "description": "Opaque resume token from a previous call's response. Pass it back to continue where you left off." },
                        "checked_pair_ids": { "type": "array", "items": { "type": "string" }, "description": "Deprecated: ignored. Kept for backward compatibility." },
                        "checked_doc_ids": { "type": "array", "items": { "type": "string" }, "description": "Deprecated: ignored. Kept for backward compatibility." }
                    },
                    "required": ["mode"]
                }
            },
            {
                "name": "generate_questions",
                "description": "Generate review questions for a single document or all documents. Lighter than check_repository (no entity discovery or deep cross-validation). For large repositories, may return partial results with `continue: true` — call again with the resume token to process remaining documents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_id": { "type": "string", "description": "Document ID (optional, generates for all documents if omitted)" },
                        "repo": { "type": "string", "description": "Filter by repository ID (optional, used when doc_id is omitted)" },
                        "dry_run": { "type": "boolean", "description": "Preview questions without modifying files (default: false)" },
                        "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600, default from config). Operation returns progress and asks to be called again if budget is exceeded." },
                        "resume": { "type": "string", "description": "Opaque resume token from a previous call's response. Pass it back to continue where you left off." }
                    }
                }
            },
            {
                "name": "scan_repository",
                "description": "Re-index documents, generate document embeddings, and detect entity links. Fact-level embeddings for cross-validation are generated separately via check_repository with mode='embeddings'. Use this when the user says 'scan the factbase' or 'rescan'. For a full quality check, use workflow with workflow='update' instead. This tool is time-boxed and WILL return `continue: true` with a resume token for non-trivial repositories — you MUST call again passing the resume token to complete.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Repository ID (optional, scans first repo if omitted)" },
                        "force_reindex": { "type": "boolean", "description": "Force re-generation of all embeddings even if content is unchanged (default: false). When true, time_budget_secs is ignored to prevent infinite restart loops." },
                        "skip_embeddings": { "type": "boolean", "description": "Skip embedding generation — index documents into DB without calling embedding provider (default: false). Useful when importing pre-computed embeddings." },
                        "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600, default from config). Ignored when force_reindex is true. Operation returns progress and asks to be called again if budget is exceeded." },
                        "resume": { "type": "string", "description": "Opaque resume token from a previous call's response. Pass it back to continue where you left off." }
                    }
                }
            },
            {
                "name": "init_repository",
                "description": "Initialize a new factbase repository at a directory path. Creates .factbase/ and registers it.\n\nTriggers: 'add this folder to factbase', 'initialize factbase at /path', 'set up a new knowledge base'",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to initialize as a repository" },
                        "id": { "type": "string", "description": "Repository ID (optional, defaults to directory name)" },
                        "name": { "type": "string", "description": "Display name (optional, defaults to id)" }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "apply_review_answers",
                "description": "Apply answered review questions to document content via LLM rewrite. For large repositories, may return partial results with `continue: true` — call again with the resume token to process remaining documents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "doc_id": { "type": "string", "description": "Apply only for this document (optional, applies all if omitted)" },
                        "repo": { "type": "string", "description": "Filter by repository ID (optional)" },
                        "dry_run": { "type": "boolean", "description": "Preview changes without modifying files (default: false)" },
                        "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600, default from config). Operation returns progress and asks to be called again if budget is exceeded." },
                        "resume": { "type": "string", "description": "Opaque resume token from a previous call's response. Pass it back to continue where you left off." }
                    }
                }
            },
            {
                "name": "workflow",
                "description": "Run a guided factbase workflow. Each step tells you exactly what to do and which tool to call next.\n\nUse this when the user says things like:\n- 'I want to make a factbase repo about mushrooms' or 'design a KB for ancient history' → workflow='bootstrap', domain='mycology' (or 'ancient Mediterranean history', etc.)\n- 'set up a new factbase' or 'create a knowledge base' → workflow='setup', path='...'\n- 'update the factbase' or 'check the factbase' or 'resync' or 'do a quality check' or 'check for issues' → workflow='update'\n- 'fix the review queue' or 'resolve issues' or 'resolve conflicts' → workflow='resolve'\n- 'research [topic]' or 'add [person/company] to factbase' → workflow='ingest', topic='...'\n- 'improve the data' or 'fill in gaps' or 'enrich [type] documents' → workflow='enrich'\n- 'improve [entity]' or 'improve document X' or 'make X better' → workflow='improve', doc_id='...'\n- 'what can factbase do' or 'what workflows are available' → workflow='list'\n\nThe 'bootstrap' workflow uses the LLM to generate domain-specific suggestions (document types, folder structure, templates, temporal patterns, source types, and example documents). Use it BEFORE 'setup' when the user describes a non-obvious domain.\n\nCall again with the next step number to advance.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workflow": { "type": "string", "description": "Workflow name: 'bootstrap', 'setup', 'update', 'resolve', 'ingest', 'enrich', 'improve', or 'list'" },
                        "step": { "type": "integer", "description": "Step number (default: 1 = start)" },
                        "domain": { "type": "string", "description": "For bootstrap: domain description (e.g. 'mycology', 'ancient Mediterranean history', 'indie video games')" },
                        "entity_types": { "type": "string", "description": "For bootstrap: optional comma-separated entity types the user wants to track (e.g. 'species, habitats, researchers')" },
                        "path": { "type": "string", "description": "For setup/bootstrap: directory path for the new repository" },
                        "topic": { "type": "string", "description": "For ingest: what to research" },
                        "doc_type": { "type": "string", "description": "For enrich: document type to focus on" },
                        "doc_id": { "type": "string", "description": "For improve: document ID to improve" },
                        "question_type": { "type": "string", "enum": ["stale", "temporal", "ambiguous", "conflict", "precision", "duplicate", "missing"], "description": "For resolve step 2: filter questions by type. Omit to get all types." },
                        "variant": { "type": "string", "enum": ["baseline", "type_evidence", "research_batch"], "description": "For resolve: prompt variant to use. 'baseline' (default) uses standard prompts. 'type_evidence' uses type-specific evidence standards per question type. 'research_batch' restructures workflow to research per-document first, then answer all questions for that document." },
                        "cross_validate": { "type": "boolean", "description": "For update: include cross-document fact validation step (default: false). If true, workflow includes a cross_validate mode step after questions." },
                        "skip": {
                            "oneOf": [
                                { "type": "string", "description": "Comma-separated step names to skip" },
                                { "type": "array", "items": { "type": "string" }, "description": "Step names to skip" }
                            ],
                            "description": "For improve: steps to skip. Valid names: 'cleanup', 'resolve', 'enrich', 'check'"
                        },
                        "repo": { "type": "string", "description": "Repository ID (optional)" }
                    },
                    "required": ["workflow"]
                }
            },
            {
                "name": "organize_analyze",
                "description": "Analyze repository for reorganization opportunities: ghost files (duplicate files sharing an ID/title in the same directory), merge candidates (similar docs), split candidates (multi-topic docs), misplaced documents (wrong folder/type), and duplicate entries. Use focus='duplicates' for detailed duplicate/stale entry info only, or focus='structure' for misplaced document detection only. Supports time-boxing via time_budget_secs; pass completed_phases to resume.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Filter by repository ID (optional)" },
                        "focus": { "type": "string", "enum": ["duplicates", "structure"], "description": "Focus on a specific analysis type. 'duplicates' returns detailed duplicate/stale entry info. 'structure' returns misplaced document candidates." },
                        "merge_threshold": { "type": "number", "description": "Minimum similarity for merge candidates (default: 0.95)" },
                        "split_threshold": { "type": "number", "description": "Maximum similarity for split candidates (default: 0.5)" },
                        "time_budget_secs": { "type": "integer", "description": "Time budget in seconds (5-600). Falls back to server.time_budget_secs config." },
                        "completed_phases": { "type": "array", "items": { "type": "string" }, "description": "Cursor: phases already completed in a previous call (returned when deadline fires)." },
                        "analyzed_doc_ids": { "type": "array", "items": { "type": "string" }, "description": "Cursor: document IDs already analyzed (for future within-phase resumption)." }
                    }
                }
            },
            {
                "name": "organize",
                "description": "Execute a knowledge base reorganization action: merge two documents, split a multi-topic document, move a document to a different folder, change a document's type, or process orphan assignments.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["merge", "split", "move", "retype", "apply"], "description": "The reorganization action to perform" },
                        "doc1": { "type": "string", "description": "First document ID (merge)" },
                        "doc2": { "type": "string", "description": "Second document ID (merge)" },
                        "into": { "type": "string", "description": "Which document to keep — must be doc1 or doc2 (merge, optional)" },
                        "doc_id": { "type": "string", "description": "Document ID (split, move, retype)" },
                        "at": { "type": "string", "description": "Split at specific section title (split, optional)" },
                        "to": { "type": "string", "description": "Destination folder relative to repo root (move)" },
                        "new_type": { "type": "string", "description": "New type to assign (retype)" },
                        "persist": { "type": "boolean", "description": "Persist type override to file (retype, default: false)" },
                        "repo": { "type": "string", "description": "Repository ID (apply, optional)" },
                        "dry_run": { "type": "boolean", "description": "Preview without executing (merge, split, move; default: false)" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "get_deferred_items",
                "description": "Get deferred review items that need human attention. Returns a focused summary of items previously deferred by agents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Filter by repository ID" },
                        "type": { "type": "string", "description": "Filter by question type (temporal, conflict, missing, ambiguous, stale, duplicate, corruption, precision)" },
                        "limit": { "type": "integer", "description": "Max items to return (default: 10)" },
                        "offset": { "type": "integer", "description": "Skip items for pagination (default: 0)" }
                    }
                }
            },
            {
                "name": "get_authoring_guide",
                "description": "Get document formatting rules, temporal tag syntax, source citation format, and templates.\n\nTriggers: 'how should I format documents', 'what format does factbase use', 'how do I write a factbase document'",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "embeddings_export",
                "description": "Export pre-computed vector embeddings as JSONL. Includes model metadata and chunk boundaries for portable distribution.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo": { "type": "string", "description": "Filter by repository ID (optional, exports all if omitted)" }
                    }
                }
            },
            {
                "name": "embeddings_import",
                "description": "Import pre-computed vector embeddings from JSONL data. Validates model and dimension compatibility. Skips embeddings for documents not in the database.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "data": { "type": "string", "description": "JSONL string with embedding header and records" },
                        "force": { "type": "boolean", "description": "Force import even if embedding dimensions don't match (default: false)" }
                    },
                    "required": ["data"]
                }
            },
            {
                "name": "embeddings_status",
                "description": "Check embedding index status: document coverage, model info, dimension, orphaned chunks.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
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

        assert_eq!(tools.len(), 25, "should have 25 tools");

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"search_knowledge"));
        assert!(names.contains(&"get_entity"));
        assert!(names.contains(&"get_review_queue"));
        assert!(names.contains(&"answer_questions"));
        assert!(names.contains(&"check_repository"));
        assert!(names.contains(&"scan_repository"));
        assert!(names.contains(&"init_repository"));
        assert!(names.contains(&"apply_review_answers"));
        assert!(names.contains(&"list_entities"));
        assert!(names.contains(&"list_repositories"));
        assert!(names.contains(&"get_perspective"));
        assert!(names.contains(&"create_document"));
        assert!(names.contains(&"update_document"));
        assert!(names.contains(&"delete_document"));
        assert!(names.contains(&"bulk_create_documents"));
        assert!(names.contains(&"search_content"));
        assert!(names.contains(&"get_deferred_items"));
        assert!(names.contains(&"get_authoring_guide"));
        assert!(names.contains(&"workflow"));
        assert!(names.contains(&"organize_analyze"));
        assert!(names.contains(&"organize"));
        assert!(names.contains(&"embeddings_export"));
        assert!(names.contains(&"embeddings_import"));
        assert!(names.contains(&"embeddings_status"));
        assert!(names.contains(&"generate_questions"));

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
    fn test_tools_list_search_knowledge_has_temporal_params() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        let search = tools
            .iter()
            .find(|t| t["name"] == "search_knowledge")
            .unwrap();
        let props = search["inputSchema"]["properties"]
            .as_object()
            .expect("properties");

        // Should have temporal params (merged from search_temporal)
        assert!(props.contains_key("as_of"));
        assert!(props.contains_key("during"));
        assert!(props.contains_key("exclude_unknown"));
        assert!(props.contains_key("boost_recent"));
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

    #[test]
    fn test_paging_tool_descriptions_use_mandatory_language() {
        let result = tools_list();
        let tools = result["tools"].as_array().expect("tools array");

        for name in ["check_repository", "scan_repository"] {
            let tool = tools.iter().find(|t| t["name"] == name).unwrap();
            let desc = tool["description"].as_str().unwrap();
            assert!(
                desc.contains("WILL return"),
                "{name} description should say paging WILL happen"
            );
            assert!(
                desc.contains("MUST"),
                "{name} description should use MUST for continuation"
            );
            assert!(
                !desc.contains("may return partial"),
                "{name} description should not use weak 'may return' language"
            );
        }
    }
}
