//! Guided workflow tools for AI agents.
//!
//! Provides step-by-step instructions that agents follow, calling other
//! factbase MCP tools along the way. The workflow tools don't do work
//! themselves — they just tell the agent what to do next.
//!
//! Workflows read the repository perspective to tailor instructions
//! to the knowledge base's purpose and policies.

pub(crate) mod helpers;
mod instructions;
mod variants;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::{Perspective, QuestionType};
use serde_json::Value;
use std::collections::HashMap;

use super::helpers::{load_glossary_terms, load_perspective, resolve_repo_filter};
use super::review::format_question_json;
use super::{get_str_arg, get_str_arg_required, get_u64_arg};
use crate::config::workflows::WorkflowsConfig;
use crate::processor::parse_review_queue;
use crate::question_generator::extract_acronym_from_question;
use helpers::*;
use instructions::*;
use variants::*;

/// Start a guided workflow.
pub fn workflow(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let workflow_name = get_str_arg_required(args, "workflow")?;
    let step = get_u64_arg(args, "step", 1) as usize;
    let repo_resolved = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let perspective = load_perspective(db, repo_resolved.as_deref());
    let repo_path = resolve_repo_path(db, repo_resolved.as_deref());

    // Build workflow config with priority: TOML files < config.yaml < prompts.yaml
    let mut wf_config = WorkflowsConfig::default();
    if let Some(ref rp) = repo_path {
        if let Some(toml_overrides) = WorkflowsConfig::load_instruction_files(rp) {
            wf_config.merge(&toml_overrides);
        }
    }
    let global_config = crate::Config::load(None).unwrap_or_default().workflows;
    wf_config.merge(&global_config);
    if let Some(ref rp) = repo_path {
        if let Some(repo_prompts) = WorkflowsConfig::load_repo_prompts(rp) {
            wf_config.merge(&repo_prompts);
        }
    }

    let deferred = || {
        db.count_deferred_questions(repo_resolved.as_deref())
            .unwrap_or(0)
    };

    match workflow_name.as_str() {
        // --- Primary workflows (new 4-verb design) ---
        "create" => Ok(create_step(step, args, &wf_config)),
        "add" => {
            let topic = get_str_arg(args, "topic");
            let doc_id = get_str_arg(args, "doc_id");
            if topic.is_some() {
                Ok(rebrand_step(
                    ingest_step(step, args, &perspective, &wf_config),
                    "ingest",
                    "add",
                ))
            } else if doc_id.is_some() {
                let skip = parse_skip_steps(args);
                Ok(rebrand_step(
                    improve_step(step, doc_id, &perspective, &skip, db, &wf_config),
                    "improve",
                    "add",
                ))
            } else {
                Ok(rebrand_step(
                    enrich_step(
                        step,
                        args,
                        &perspective,
                        db,
                        repo_resolved.as_deref(),
                        &wf_config,
                    ),
                    "enrich",
                    "add",
                ))
            }
        }
        "maintain" => Ok(maintain_step(
            step,
            args,
            &perspective,
            deferred(),
            db,
            &wf_config,
        )),
        "refresh" => Ok(refresh_step(
            step,
            args,
            &perspective,
            deferred(),
            db,
            &wf_config,
        )),
        "correct" => Ok(correct_step(step, args, &wf_config)),
        "transition" => Ok(transition_step(step, args, &wf_config)),

        // --- Standalone (power user) ---
        "resolve" => Ok(resolve_step(
            step,
            args,
            &perspective,
            deferred(),
            db,
            &wf_config,
        )),

        // --- Legacy aliases ---
        "bootstrap" | "setup" => Ok(rebrand_step(
            create_step(step, args, &wf_config),
            "create",
            workflow_name.as_str(),
        )),
        "update" => Ok(rebrand_step(
            maintain_step(step, args, &perspective, deferred(), db, &wf_config),
            "maintain",
            "update",
        )),
        "ingest" => Ok(rebrand_step(
            ingest_step(step, args, &perspective, &wf_config),
            "ingest",
            "ingest",
        )),
        "enrich" => Ok(rebrand_step(
            enrich_step(
                step,
                args,
                &perspective,
                db,
                repo_resolved.as_deref(),
                &wf_config,
            ),
            "enrich",
            "enrich",
        )),
        "improve" => {
            let doc_id = get_str_arg(args, "doc_id");
            let skip = parse_skip_steps(args);
            Ok(improve_step(
                step,
                doc_id,
                &perspective,
                &skip,
                db,
                &wf_config,
            ))
        }

        "list" => Ok(serde_json::json!({
            "workflows": [
                {"name": "create", "description": "Build a knowledge base from scratch. Provide domain='mycology' (or any domain) to get tailored structure suggestions, then guided setup: init, perspective, first documents, scan, verify. Accepts: domain, entity_types, path."},
                {"name": "add", "description": "Grow the knowledge base. With topic='X': research and create new entities. With doc_id='X': improve that specific document. With neither: scan for gaps and enrich existing docs."},
                {"name": "maintain", "description": "Internal quality maintenance: scan, detect links, check quality, link suggestions, organize analysis, resolve all questions, cleanup scan, report. Answers from existing knowledge only — no external research. With doc_id: maintain just that document."},
                {"name": "refresh", "description": "Research-enabled update: scan, check, then actively research entities using available tools (web search, etc.) to find latest info, update facts, add temporal tags, resolve questions, cleanup, report. USE THIS when the user asks about recent updates, new developments, latest news, or whether anything has changed. Trigger phrases: 'check for updates', 'look for recent', 'what\\'s new', 'has anything changed', 'recent news/developments/discoveries'. Example: 'Has anything changed with [topic]?' → refresh, topic='[topic]'. IMPORTANT: refresh=UPDATE existing docs with new info; add=CREATE new docs. Optional filters: doc_type, doc_id, staleness threshold."},
                {"name": "correct", "description": "Propagate a fact correction across the entire KB. Provide correction='what is wrong and what is true' and optional source='who said it, when'. Finds all documents containing the false claim and fixes them with an audit trail."},
                {"name": "transition", "description": "Handle temporal entity changes — renames, mergers, acquisitions, role changes. Unlike correct (which fixes false claims), transition handles things that WERE true and CHANGED. Asks how to reference the entity going forward before making changes. Provide change='what changed' and optional effective_date, source."},
                {"name": "resolve", "description": "(Advanced) Answer existing review queue questions, apply changes, cleanup. Use maintain instead for full maintenance. Optionally pass question_type to filter by type."},
            ],
            "aliases": {
                "bootstrap": "create", "setup": "create",
                "update": "maintain",
                "ingest": "add", "enrich": "add", "improve": "add"
            }
        })),
        _ => Ok(serde_json::json!({
            "error": format!("Unknown workflow '{}'. Call workflow with workflow='list' to see available workflows.", workflow_name)
        })),
    }
}

fn setup_step(step: usize, args: &Value, wf: &WorkflowsConfig) -> Value {
    let path = get_str_arg(args, "path").unwrap_or("the target directory");
    let total = 6;
    match step {
        1 => serde_json::json!({
            "workflow": "setup",
            "step": 1, "total_steps": total,
            "title": "Step 1 of 6: Set Up Repository Directory",
            "instruction": resolve(wf, "setup.init", DEFAULT_SETUP_INIT_INSTRUCTION, &[("path", path)]),
            "next_tool": "filesystem",
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
                resolve(
                    wf,
                    "setup.validate_ok",
                    DEFAULT_SETUP_VALIDATE_OK_INSTRUCTION,
                    &[("detail", &detail)],
                )
            } else {
                resolve(
                    wf,
                    "setup.validate_error",
                    DEFAULT_SETUP_VALIDATE_ERROR_INSTRUCTION,
                    &[("detail", &detail)],
                )
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
        }
        4 => serde_json::json!({
            "workflow": "setup",
            "step": 4, "total_steps": total,
            "title": "Step 4 of 6: Create Documents",
            "instruction": resolve(wf, "setup.create", DEFAULT_SETUP_CREATE_INSTRUCTION, &[("format_rules", FORMAT_RULES)]),
            "next_tool": "factbase", "suggested_op": "authoring_guide",
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=5) to continue to Step 5 of 6"
        }),
        5 => serde_json::json!({
            "workflow": "setup",
            "step": 5, "total_steps": total,
            "title": "Step 5 of 6: Scan & Verify",
            "instruction": resolve(wf, "setup.scan", DEFAULT_SETUP_SCAN_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "⚠️ REQUIRED: Call workflow(workflow='setup', step=6) to continue to Step 6 of 6"
        }),
        6 => {
            let path = get_str_arg(args, "path").unwrap_or("the target directory");
            let mut resp = serde_json::json!({
                "workflow": "setup",
                "step": 6, "total_steps": total,
                "title": "Step 6 of 6: Complete",
                "instruction": resolve(wf, "setup.complete", DEFAULT_SETUP_COMPLETE_INSTRUCTION, &[]),
                "complete": true
            });
            if let Some(obsidian) = apply_obsidian_files(path) {
                resp["obsidian_git_setup"] = obsidian;
            }
            resp
        }
        _ => serde_json::json!({
            "workflow": "setup",
            "complete": true,
            "instruction": "Workflow complete."
        }),
    }
}

/// Run the bootstrap workflow: return instructions for the agent to generate domain-specific KB structure.
pub fn bootstrap(args: &Value) -> Result<Value, FactbaseError> {
    let domain = get_str_arg_required(args, "domain")?;
    let entity_types = get_str_arg(args, "entity_types");

    let prompts = crate::Config::load(None).unwrap_or_default().prompts;
    let prompt = build_bootstrap_prompt(&domain, entity_types, &prompts, None);

    Ok(serde_json::json!({
        "workflow": "bootstrap",
        "domain": domain,
        "instruction": prompt,
        "expected_format": "Generate a JSON object with these 4 fields: document_types (array of {name, description}), folder_structure (array of paths), templates (object mapping type to markdown template), perspective ({focus, allowed_types}). Then proceed to setup.",
        "next_steps": [
            "Generate the JSON suggestions based on the instruction above.",
            "Use the suggestions as reference when configuring perspective.yaml (YAML format, not markdown) and creating documents.",
            "⚠️ REQUIRED NEXT: Call workflow(workflow='create', step=2) to continue setup.",
            "Do NOT skip — the next steps provide guidance including format rules for temporal tags and source footnotes."
        ],
        "note": "These are suggestions — adapt them to your needs. The templates and folder structure can be modified at any time.",
        "when_done": "⚠️ REQUIRED: Call workflow(workflow='create', step=2) to continue"
    }))
}

fn create_step(step: usize, args: &Value, wf: &WorkflowsConfig) -> Value {
    let domain = get_str_arg(args, "domain");
    let total = if domain.is_some() { 7 } else { 6 };

    // Step 1 with domain: bootstrap. Without domain: init.
    if domain.is_some() {
        match step {
            1 => {
                // Bootstrap: generate domain-specific structure
                match bootstrap(args) {
                    Ok(mut v) => {
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("workflow".into(), Value::String("create".into()));
                            obj.insert("step".into(), serde_json::json!(1));
                            obj.insert("total_steps".into(), serde_json::json!(total));
                            obj.insert(
                                "title".into(),
                                Value::String(format!(
                                    "Step 1 of {total}: Design Knowledge Base Structure"
                                )),
                            );
                            obj.insert("when_done".into(), Value::String(format!("⚠️ REQUIRED: Call workflow(workflow='create', step=2) to continue to Step 2 of {total}")));
                        }
                        v
                    }
                    Err(_) => serde_json::json!({
                        "workflow": "create", "step": 1, "total_steps": total,
                        "error": "Bootstrap requires a 'domain' parameter."
                    }),
                }
            }
            s => {
                // Steps 2-7 map to setup steps 1-6
                let mut v = setup_step(s - 1, args, wf);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("workflow".into(), Value::String("create".into()));
                    obj.insert("step".into(), serde_json::json!(s));
                    obj.insert("total_steps".into(), serde_json::json!(total));
                    obj.insert(
                        "title".into(),
                        Value::String(format!(
                            "Step {s} of {total}: {}",
                            match s {
                                2 => "Initialize Repository",
                                3 => "Configure Perspective",
                                4 => "Validate Perspective",
                                5 => "Create Documents",
                                6 => "Scan & Verify",
                                _ => "Complete",
                            }
                        )),
                    );
                    // Fix when_done routing
                    if s < total {
                        obj.insert("when_done".into(), Value::String(format!(
                            "⚠️ REQUIRED: Call workflow(workflow='create', step={}) to continue to Step {} of {total}", s + 1, s + 1)));
                    }
                    // Replace setup complete instruction with create complete
                    if s == total {
                        obj.insert(
                            "instruction".into(),
                            Value::String(resolve(
                                wf,
                                "create.complete",
                                DEFAULT_CREATE_COMPLETE_INSTRUCTION,
                                &[],
                            )),
                        );
                        obj.insert("complete".into(), Value::Bool(true));
                    }
                }
                // Fix instruction references to setup workflow
                if let Some(instr) = v
                    .get("instruction")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                {
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert(
                            "instruction".into(),
                            Value::String(instr.replace("workflow='setup'", "workflow='create'")),
                        );
                    }
                }
                v
            }
        }
    } else {
        // No domain: steps 1-6 map directly to setup steps 1-6
        match step {
            s @ 1..=5 => {
                let mut v = setup_step(s, args, wf);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("workflow".into(), Value::String("create".into()));
                    obj.insert("total_steps".into(), serde_json::json!(total));
                    if s < total {
                        obj.insert("when_done".into(), Value::String(format!(
                            "⚠️ REQUIRED: Call workflow(workflow='create', step={}) to continue to Step {} of {total}", s + 1, s + 1)));
                    }
                }
                if let Some(instr) = v
                    .get("instruction")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                {
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert(
                            "instruction".into(),
                            Value::String(instr.replace("workflow='setup'", "workflow='create'")),
                        );
                    }
                }
                v
            }
            _ => {
                let path = get_str_arg(args, "path").unwrap_or("the target directory");
                let mut resp = serde_json::json!({
                    "workflow": "create", "step": total, "total_steps": total,
                    "title": format!("Step {total} of {total}: Complete"),
                    "instruction": resolve(wf, "create.complete", DEFAULT_CREATE_COMPLETE_INSTRUCTION, &[]),
                    "complete": true
                });
                if let Some(obsidian) = apply_obsidian_files(path) {
                    resp["obsidian_git_setup"] = obsidian;
                }
                resp
            }
        }
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
    let total = 6;
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
                "instruction": resolve(wf, "resolve.queue", DEFAULT_RESOLVE_QUEUE_INSTRUCTION, &[("ctx", &ctx)]),
                "next_tool": "workflow",
                "suggested_args": suggested,
                "policy": {"stale_days": stale},
                "deferred_count": deferred,
                "total_unanswered": total_unanswered,
                "type_distribution": type_dist_json,
                "recommended_order": rec_order,
                "resolve_batch_size": wf.resolve_batch_size(),
                "when_done": "Call workflow with workflow='resolve', step=2, question_type=<next_type>"
            })
        }
        2 => resolve_step2_batch(args, perspective, db, wf),
        3 => serde_json::json!({
            "workflow": "resolve",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "resolve.apply", DEFAULT_RESOLVE_APPLY_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "get_entity",
            "when_done": "Call workflow with workflow='resolve', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "resolve",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "resolve.verify", DEFAULT_RESOLVE_VERIFY_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "check",
            "suggested_args": {"dry_run": true},
            "when_done": "Call workflow with workflow='resolve', step=5"
        }),
        5 => serde_json::json!({
            "workflow": "resolve",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "resolve.cleanup", DEFAULT_RESOLVE_CLEANUP_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "After scan completes, call workflow with workflow='resolve', step=6"
        }),
        6 => {
            let type_dist = compute_type_distribution(db);
            let remaining: usize = type_dist.iter().map(|(_, c)| c).sum();
            let new_deferred = db.count_deferred_questions(None).unwrap_or(0);
            serde_json::json!({
                "workflow": "resolve",
                "step": 6, "total_steps": total,
                "instruction": "Resolve workflow complete. Report the final state of the knowledge base.",
                "remaining_questions": remaining,
                "deferred_questions": new_deferred,
                "complete": true
            })
        }
        _ => serde_json::json!({
            "workflow": "resolve",
            "complete": true,
            "instruction": "Workflow complete. All review questions have been processed."
        }),
    }
}

// --- Maintain workflow ---

fn maintain_step(
    step: usize,
    _args: &Value,
    perspective: &Option<Perspective>,
    deferred: usize,
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
    let ctx = perspective_context(perspective);
    let total = 7;
    match step {
        1 => {
            let mut resp = serde_json::json!({
                "workflow": "maintain",
                "step": 1, "total_steps": total,
                "instruction": resolve(wf, "maintain.scan", DEFAULT_MAINTAIN_SCAN_INSTRUCTION, &[("ctx", &ctx)]),
                "next_tool": "factbase", "suggested_op": "scan",
                "when_done": "Call workflow with workflow='maintain', step=2"
            });
            if let Some((reason, doc_count)) = detect_full_rebuild(db) {
                let est_secs = doc_count * 2;
                let est_display = if est_secs >= 60 {
                    format!("~{} minutes", est_secs / 60)
                } else {
                    format!("~{est_secs} seconds")
                };
                resp["requires_confirmation"] = Value::Bool(true);
                resp["confirmation_reason"] = Value::String("full embedding rebuild".into());
                resp["confirmation_details"] = Value::String(format!(
                    "All {doc_count} documents need re-embedding because {reason}. Estimated time: {est_display}."
                ));
            }
            resp
        }
        2 => serde_json::json!({
            "workflow": "maintain",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "maintain.detect_links", DEFAULT_MAINTAIN_DETECT_LINKS_INSTRUCTION, &[("ctx", &ctx)]),
            "next_tool": "factbase", "suggested_op": "detect_links",
            "when_done": "Call workflow with workflow='maintain', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "maintain",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "maintain.check", DEFAULT_MAINTAIN_CHECK_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "check",
            "when_done": "Call workflow with workflow='maintain', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "maintain",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "maintain.links", DEFAULT_MAINTAIN_LINKS_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "links",
            "when_done": "Call workflow with workflow='maintain', step=5"
        }),
        5 => serde_json::json!({
            "workflow": "maintain",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "maintain.organize", DEFAULT_MAINTAIN_ORGANIZE_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "organize",
            "when_done": "Call workflow with workflow='maintain', step=6"
        }),
        6 => {
            let type_dist = compute_type_distribution(db);
            let total_unanswered: usize = type_dist.iter().map(|(_, c)| c).sum();
            if total_unanswered == 0 {
                return serde_json::json!({
                    "workflow": "maintain",
                    "step": 6, "total_steps": total,
                    "instruction": "No review questions found — the knowledge base is clean. Skip to the final report.",
                    "total_unanswered": 0,
                    "when_done": "Call workflow with workflow='maintain', step=7"
                });
            }
            serde_json::json!({
                "workflow": "maintain",
                "step": 6, "total_steps": total,
                "instruction": resolve(wf, "maintain.resolve", DEFAULT_MAINTAIN_RESOLVE_INSTRUCTION, &[("ctx", &ctx)]),
                "next_tool": "workflow",
                "suggested_args": {"workflow": "resolve", "step": 1},
                "total_unanswered": total_unanswered,
                "deferred_count": deferred,
                "when_done": "After resolve workflow completes, call workflow with workflow='maintain', step=7"
            })
        }
        _ => {
            let type_dist = compute_type_distribution(db);
            let remaining: usize = type_dist.iter().map(|(_, c)| c).sum();
            let new_deferred = db.count_deferred_questions(None).unwrap_or(0);
            let mut resp = serde_json::json!({
                "workflow": "maintain",
                "step": 7, "total_steps": total,
                "instruction": resolve(wf, "maintain.report", DEFAULT_MAINTAIN_REPORT_INSTRUCTION, &[]),
                "remaining_questions": remaining,
                "deferred_questions": new_deferred,
                "complete": true
            });
            if is_obsidian_format(perspective) {
                resp["tip"] = serde_json::json!("If you renamed files in Obsidian since the last scan, run factbase(op=scan) to sync the database with the new paths.");
            }
            resp
        }
    }
}

// --- Refresh workflow (research-enabled maintenance) ---

fn refresh_step(
    step: usize,
    args: &Value,
    perspective: &Option<Perspective>,
    deferred: usize,
    db: &Database,
    wf: &WorkflowsConfig,
) -> Value {
    let ctx = perspective_context(perspective);
    let total = 6;
    match step {
        1 => serde_json::json!({
            "workflow": "refresh",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "refresh.scan", DEFAULT_REFRESH_SCAN_INSTRUCTION, &[("ctx", &ctx)]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "Call workflow with workflow='refresh', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "refresh",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "refresh.check", DEFAULT_REFRESH_CHECK_INSTRUCTION, &[("ctx", &ctx)]),
            "next_tool": "factbase", "suggested_op": "check",
            "when_done": "Call workflow with workflow='refresh', step=3"
        }),
        3 => {
            // Build entity list for research, filtered by args
            let doc_type = get_str_arg(args, "doc_type");
            let doc_id = get_str_arg(args, "doc_id");
            let repo = get_str_arg(args, "repo");
            let quality = if let Some(id) = doc_id {
                entity_quality(db, id)
                    .map(|q| Value::Array(vec![q]))
                    .unwrap_or(Value::Array(vec![]))
            } else {
                bulk_quality(db, doc_type, repo)
            };
            serde_json::json!({
                "workflow": "refresh",
                "step": 3, "total_steps": total,
                "instruction": resolve(wf, "refresh.research", DEFAULT_REFRESH_RESEARCH_INSTRUCTION, &[("ctx", &ctx), ("format_rules", FORMAT_RULES)]),
                "entity_quality": quality,
                "note": "Entities sorted by attention_score (highest first). Focus on stale and low-coverage entities.",
                "when_done": "Call workflow with workflow='refresh', step=4"
            })
        }
        4 => {
            let type_dist = compute_type_distribution(db);
            let total_unanswered: usize = type_dist.iter().map(|(_, c)| c).sum();
            if total_unanswered == 0 {
                return serde_json::json!({
                    "workflow": "refresh",
                    "step": 4, "total_steps": total,
                    "instruction": "No review questions to resolve. Skip to cleanup.",
                    "total_unanswered": 0,
                    "when_done": "Call workflow with workflow='refresh', step=5"
                });
            }
            serde_json::json!({
                "workflow": "refresh",
                "step": 4, "total_steps": total,
                "instruction": resolve(wf, "refresh.resolve", DEFAULT_REFRESH_RESOLVE_INSTRUCTION, &[("ctx", &ctx)]),
                "next_tool": "workflow",
                "suggested_args": {"workflow": "resolve", "step": 1},
                "total_unanswered": total_unanswered,
                "deferred_count": deferred,
                "when_done": "After resolve completes, call workflow with workflow='refresh', step=5"
            })
        }
        5 => serde_json::json!({
            "workflow": "refresh",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "refresh.cleanup", DEFAULT_REFRESH_CLEANUP_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "Call workflow with workflow='refresh', step=6"
        }),
        _ => {
            let type_dist = compute_type_distribution(db);
            let remaining: usize = type_dist.iter().map(|(_, c)| c).sum();
            let new_deferred = db.count_deferred_questions(None).unwrap_or(0);
            let mut resp = serde_json::json!({
                "workflow": "refresh",
                "step": 6, "total_steps": total,
                "instruction": resolve(wf, "refresh.report", DEFAULT_REFRESH_REPORT_INSTRUCTION, &[]),
                "remaining_questions": remaining,
                "deferred_questions": new_deferred,
                "complete": true
            });
            if is_obsidian_format(perspective) {
                resp["tip"] = serde_json::json!("If you renamed files in Obsidian since the last scan, run factbase(op=scan) to sync the database with the new paths.");
            }
            resp
        }
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

    // --- Weak-source triage pre-step ---
    // Only fires when question_type=weak-source is explicitly requested.
    // When triage_results are provided, apply VALID dismissals and attach hints to INVALID/WEAK.
    // When no triage_results yet, return a triage batch for the agent to label.
    let triage_results = args.get("triage_results")
        .and_then(|v| v.as_array())
        .cloned();

    let is_weak_source_filter = get_str_arg(args, "question_type")
        .map(|s| s.split(',').any(|t| t.trim().eq_ignore_ascii_case("weak-source")))
        .unwrap_or(false);

    if is_weak_source_filter {
        if let Some(ref results) = triage_results {
            // Apply triage: auto-dismiss VALID, attach suggestions to INVALID/WEAK
            let docs = load_review_docs_from_disk(db);
            let mut weak_source_questions: Vec<(String, usize)> = Vec::new();
            for doc in &docs {
                if let Some(questions) = crate::processor::parse_review_queue(&doc.content) {
                    for (idx, q) in questions.iter().enumerate() {
                        if !q.answered && !q.is_deferred() && !q.is_believed()
                            && q.question_type == QuestionType::WeakSource
                        {
                            weak_source_questions.push((doc.id.clone(), idx));
                        }
                    }
                }
            }
            for (i, (doc_id, q_idx)) in weak_source_questions.iter().enumerate() {
                let verdict = results.iter()
                    .find(|r| r.get("index").and_then(|v| v.as_u64()) == Some(i as u64 + 1))
                    .and_then(|r| r.get("verdict"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if verdict.eq_ignore_ascii_case("VALID") {
                    let _ = auto_dismiss_question(db, doc_id, *q_idx);
                } else if !verdict.is_empty() {
                    let suggestion = results.iter()
                        .find(|r| r.get("index").and_then(|v| v.as_u64()) == Some(i as u64 + 1))
                        .and_then(|r| r.get("suggestion"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !suggestion.is_empty() {
                        if let Ok(doc) = db.require_document(doc_id) {
                            let marker = "<!-- factbase:review -->";
                            if let Some(marker_pos) = doc.content.find(marker) {
                                let (before, after) = doc.content.split_at(marker_pos);
                                let queue_content = &after[marker.len()..];
                                let hint_answer = format!("hint: {suggestion}");
                                if let Some(modified) = super::review::modify_question_in_queue(
                                    queue_content, *q_idx, &hint_answer, false,
                                ) {
                                    let new_content = format!("{before}{marker}{modified}");
                                    let new_hash = crate::processor::content_hash(&new_content);
                                    let _ = db.update_document_content(doc_id, &new_content, &new_hash);
                                }
                            }
                        }
                    }
                }
            }
            // Fall through to normal batch logic after applying triage
        } else {
            // No triage_results yet — return triage batch
            let docs = load_review_docs_from_disk(db);
            let mut weak_source_batch: Vec<serde_json::Value> = Vec::new();
            let triage_batch_size = crate::question_generator::CITATION_TRIAGE_BATCH_SIZE;
            for doc in &docs {
                if weak_source_batch.len() >= triage_batch_size { break; }
                if let Some(questions) = crate::processor::parse_review_queue(&doc.content) {
                    for (idx, q) in questions.iter().enumerate() {
                        if weak_source_batch.len() >= triage_batch_size { break; }
                        if !q.answered && !q.is_deferred() && !q.is_believed()
                            && q.question_type == QuestionType::WeakSource
                            && !q.answer.as_deref().unwrap_or("").starts_with("hint:")
                        {
                            weak_source_batch.push(serde_json::json!({
                                "index": weak_source_batch.len() + 1,
                                "doc_id": doc.id,
                                "doc_title": doc.title,
                                "question_index": idx,
                                "description": q.description,
                            }));
                        }
                    }
                }
            }
            if !weak_source_batch.is_empty() {
                let mut triage_prompt = String::from(
                    "Evaluate these citations. For each, ask: could someone with access to this KB's domain \
                     find the exact source using only the information provided? Consider:\n\
                     - Source authority (is this a primary/authoritative source for the claim?)\n\
                     - Accessibility (can the URL be reached? Is it behind a paywall?)\n\
                     - Specificity (does it point to a specific page, not just a homepage?)\n\
                     - Duplicates (is this the same source as another footnote?)\n\
                     - Fabrication risk (does this source actually exist?)\n\
                     Respond: VALID|INVALID|WEAK — reason — suggestion with specific replacement if applicable\n\n",
                );
                if let Some(policy) = perspective.as_ref().and_then(|p| p.internal_sources.as_deref()) {
                    triage_prompt.push_str(&format!(
                        "This KB has the following internal source policy:\n{policy}\nUse this to judge whether internal citations are sufficient.\n\n"
                    ));
                }
                for item in &weak_source_batch {
                    triage_prompt.push_str(&format!(
                        "{}. [doc: {}] {}\n",
                        item["index"].as_u64().unwrap_or(0),
                        item["doc_title"].as_str().unwrap_or(""),
                        item["description"].as_str().unwrap_or(""),
                    ));
                }
                return serde_json::json!({
                    "workflow": "resolve",
                    "step": 2, "total_steps": total_steps,
                    "triage_pre_step": true,
                    "instruction": resolve(wf, "resolve.weak_source_triage", DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION, &[]),
                    "weak_source_questions": weak_source_batch.len(),
                    "citations": weak_source_batch,
                    "triage_prompt": triage_prompt,
                    "continue": true,
                    "when_done": "After labeling, call workflow(workflow='resolve', step=2, question_type='weak-source', triage_results=[{index, verdict, suggestion}, ...]) to apply labels and continue"
                });
            }
        }
    }

    // Optional question_type filter (comma-separated)
    let type_filter: Vec<QuestionType> = get_str_arg(args, "question_type")
        .map(|s| {
            s.split(',')
                .filter_map(|t| t.trim().parse::<QuestionType>().ok())
                .collect()
        })
        .unwrap_or_default();

    // Optional doc_type filter
    let doc_type_filter = get_str_arg(args, "doc_type");

    // Collect all questions from the review queue (prefer disk content)
    let docs = load_review_docs_from_disk(db);
    let mut unanswered: Vec<Value> = Vec::new();
    let mut resolved_verified: usize = 0;
    let mut resolved_believed: usize = 0;
    let mut resolved_deferred: usize = 0;
    let mut type_distribution: HashMap<QuestionType, usize> = HashMap::new();

    // Load glossary terms to auto-dismiss acronym questions already covered
    let glossary_terms = load_glossary_terms(db, None);
    let mut glossary_auto_resolved: usize = 0;

    for doc in &docs {
        // Apply doc_type filter
        if let Some(dt_filter) = doc_type_filter {
            let matches = doc
                .doc_type
                .as_deref()
                .is_some_and(|dt| dt.eq_ignore_ascii_case(dt_filter));
            if !matches {
                continue;
            }
        }
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
                            if glossary_terms
                                .iter()
                                .any(|t| t.eq_ignore_ascii_case(&acronym))
                            {
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

    let resolved_so_far =
        resolved_verified + resolved_believed + resolved_deferred + glossary_auto_resolved;

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

    // Include active filter in response
    let active_filter: Value = if type_filter.is_empty() {
        Value::Null
    } else {
        type_filter
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .into()
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

    let batch_size = wf.resolve_batch_size();
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
        _ => (
            DEFAULT_RESOLVE_ANSWER_INSTRUCTION,
            DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION,
        ),
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

    let pct = if total_questions > 0 {
        (resolved_so_far * 100) / total_questions
    } else {
        0
    };

    // Collect batch question types for checkpoint summary
    let _last_batch_types: Vec<String> = {
        let mut types: Vec<String> = batch
            .iter()
            .filter_map(|q| q["type"].as_str().map(|s| s.to_string()))
            .collect();
        types.sort();
        types.dedup();
        types
    };

    // Slim subsequent batches: only send full instruction/patterns/conflicts on first batch
    let instruction_value = if is_first_batch {
        Value::String(instruction)
    } else {
        Value::String("LOOP: continue=true means call workflow(resolve, step=2) immediately after answering. continue=false means done. You do not decide when to stop — not context size, not your judgment. Your runtime compacts automatically.".to_string())
    };

    let mut result = serde_json::json!({
        "workflow": "resolve",
        "step": 2, "total_steps": total_steps,
        "instruction": instruction_value,
        "next_tool": "factbase", "suggested_op": "answer",
        "variant": variant,
        "variant_source": variant_source,
        "type_filter": active_filter,
        "continue": true,
        "batch": batch_value,
        "completion_gate": format!("{resolved_so_far}/{total_questions} resolved ({pct}%). Call workflow resolve step=2."),
        "when_done": "Call workflow with workflow='resolve', step=2 immediately."
    });

    // Only include conflict_patterns when first batch or batch contains conflict questions
    let batch_has_conflicts = batch.iter().any(|q| q["type"].as_str() == Some("conflict"));
    if is_first_batch || batch_has_conflicts {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("conflict_patterns".to_string(), serde_json::json!({
            "parallel_overlap": "Two overlapping facts about different entities that may legitimately coexist. Answer: 'Not a conflict: parallel overlap'.",
            "same_entity_transition": "Two overlapping facts about the same entity where one likely supersedes the other. Adjust the earlier entry's end date.",
            "date_imprecision": "Small overlap relative to date ranges — likely data-source imprecision. Adjust the boundary date.",
            "unknown": "No recognized pattern — investigate which fact is current."
        }));
        }
    }

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
        if let Some(obj) = result.as_object_mut() {
            obj.insert("intro".to_string(), Value::String(intro));
        }
    }

    if glossary_auto_resolved > 0 {
        if let Some(obj) = result.as_object_mut() {
            obj.insert(
                "glossary_auto_resolved".to_string(),
                serde_json::json!(glossary_auto_resolved),
            );
        }
    }

    if is_first_batch && !patterns.is_empty() {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("patterns_detected".to_string(), Value::Array(patterns));
        }
    }

    result
}

fn ingest_step(
    step: usize,
    args: &Value,
    perspective: &Option<Perspective>,
    wf: &WorkflowsConfig,
) -> Value {
    let topic = get_str_arg(args, "topic").unwrap_or("the requested topic");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 5;
    match step {
        1 => serde_json::json!({
            "workflow": "ingest",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "ingest.search", DEFAULT_INGEST_SEARCH_INSTRUCTION, &[("topic", topic), ("ctx", &ctx)]),
            "next_tool": "factbase", "suggested_op": "search",
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
            "next_tool": "factbase", "suggested_op": "bulk_create",
            "when_done": "Call workflow with workflow='ingest', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "ingest",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "ingest.verify", DEFAULT_INGEST_VERIFY_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "check",
            "when_done": "Call workflow with workflow='ingest', step=5"
        }),
        5 => serde_json::json!({
            "workflow": "ingest",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "ingest.links", DEFAULT_INGEST_LINKS_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "links",
            "complete": true
        }),
        _ => serde_json::json!({
            "workflow": "ingest",
            "complete": true,
            "instruction": "Workflow complete. Documents have been created/updated."
        }),
    }
}

fn enrich_step(
    step: usize,
    args: &Value,
    perspective: &Option<Perspective>,
    db: &Database,
    repo: Option<&str>,
    wf: &WorkflowsConfig,
) -> Value {
    let doc_type = get_str_arg(args, "doc_type").unwrap_or("all types");
    let ctx = perspective_context(perspective);
    let fields = required_fields_hint(perspective);
    let total = 5;
    match step {
        1 => {
            let type_filter = if doc_type != "all types" {
                Some(doc_type)
            } else {
                None
            };
            let quality = bulk_quality(db, type_filter, repo);
            serde_json::json!({
                "workflow": "enrich",
                "step": 1, "total_steps": total,
                "instruction": resolve(wf, "enrich.review", DEFAULT_ENRICH_REVIEW_INSTRUCTION, &[("ctx", &ctx)]),
                "entity_quality": quality,
                "next_tool": "factbase", "suggested_op": "get_entity",
                "when_done": "Call workflow with workflow='enrich', step=2"
            })
        }
        2 => serde_json::json!({
            "workflow": "enrich",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "enrich.gaps", DEFAULT_ENRICH_GAPS_INSTRUCTION, &[("fields", &fields)]),
            "next_tool": "factbase", "suggested_op": "get_entity",
            "when_done": "Call workflow with workflow='enrich', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "enrich",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "enrich.research", DEFAULT_ENRICH_RESEARCH_INSTRUCTION, &[("ctx", &ctx), ("format_rules", FORMAT_RULES)]),
            "next_tool": "factbase", "suggested_op": "update",
            "when_done": "Call workflow with workflow='enrich', step=4"
        }),
        4 => serde_json::json!({
            "workflow": "enrich",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "enrich.scan", DEFAULT_ENRICH_SCAN_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "Call workflow with workflow='enrich', step=5"
        }),
        5 => serde_json::json!({
            "workflow": "enrich",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "enrich.verify", DEFAULT_ENRICH_VERIFY_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "check",
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
    let doc_hint = doc_id
        .map(|id| format!(" for document '{id}'"))
        .unwrap_or_default();
    let doc_arg = doc_id.map(|id| serde_json::json!(id));
    let skipped: Vec<&str> = skip.iter().map(|s| s.as_str()).collect();
    let next_step_hint = if step < total {
        format!(
            "Call workflow with workflow='improve', step={}{}",
            step + 1,
            doc_id
                .map(|id| format!(", doc_id='{id}'"))
                .unwrap_or_default()
        )
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
            "next_tool": "factbase", "suggested_op": "get_entity",
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
            "next_tool": "factbase", "suggested_op": "review_queue",
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
            "next_tool": "factbase", "suggested_op": "get_entity",
            "suggested_args": {"id": doc_arg},
            "when_done": next_step_hint
        }),
        "scan" => serde_json::json!({
            "workflow": "improve",
            "step": step, "total_steps": total,
            "step_name": "scan",
            "doc_id": doc_arg,
            "skipped_steps": skipped,
            "instruction": resolve(wf, "improve.scan", DEFAULT_IMPROVE_SCAN_INSTRUCTION, &[("doc_hint", &doc_hint)]),
            "next_tool": "factbase", "suggested_op": "scan",
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
                "next_tool": "factbase", "suggested_op": "check",
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

fn correct_step(step: usize, args: &Value, wf: &WorkflowsConfig) -> Value {
    let correction = get_str_arg(args, "correction").unwrap_or("(no correction provided)");
    let source = get_str_arg(args, "source").unwrap_or("(not specified)");
    let source_note = if source != "(not specified)" {
        format!("Source: {source}")
    } else {
        String::new()
    };
    let source_footnote = if source != "(not specified)" {
        format!("\n   e. Footnote text: \"{source}\"")
    } else {
        String::new()
    };
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let temporal_hint = format!(
        " @t[=<date>] using the correction date. If no temporal context was identified in step 1, fall back to today's date: @t[={today}]"
    );
    let total = 4;
    match step {
        1 => serde_json::json!({
            "workflow": "correct",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "correct.parse", DEFAULT_CORRECT_PARSE_INSTRUCTION,
                &[("correction", correction), ("source_note", &source_note)]),
            "correction": correction,
            "source": source,
            "when_done": "Call workflow with workflow='correct', step=2"
        }),
        2 => serde_json::json!({
            "workflow": "correct",
            "step": 2, "total_steps": total,
            "instruction": resolve(wf, "correct.search", DEFAULT_CORRECT_SEARCH_INSTRUCTION,
                &[("correction", correction)]),
            "next_tool": "factbase", "suggested_op": "search",
            "when_done": "Call workflow with workflow='correct', step=3"
        }),
        3 => serde_json::json!({
            "workflow": "correct",
            "step": 3, "total_steps": total,
            "instruction": resolve(wf, "correct.fix", DEFAULT_CORRECT_FIX_INSTRUCTION,
                &[("correction", correction), ("source_note", &source_note),
                  ("source_footnote", &source_footnote), ("temporal_hint", &temporal_hint)]),
            "next_tool": "factbase", "suggested_op": "get_entity",
            "when_done": "Call workflow with workflow='correct', step=4"
        }),
        _ => serde_json::json!({
            "workflow": "correct",
            "step": 4, "total_steps": total,
            "instruction": resolve(wf, "correct.cleanup", DEFAULT_CORRECT_CLEANUP_INSTRUCTION,
                &[("correction", correction), ("source", source)]),
            "next_tool": "factbase", "suggested_op": "scan",
            "complete": true
        }),
    }
}

fn transition_step(step: usize, args: &Value, wf: &WorkflowsConfig) -> Value {
    let change = get_str_arg(args, "change").unwrap_or("(no change provided)");
    let source = get_str_arg(args, "source").unwrap_or("(not specified)");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let effective_date = get_str_arg(args, "effective_date")
        .map(|s| s.to_string())
        .unwrap_or_else(|| today.clone());
    let nomenclature = get_str_arg(args, "nomenclature");
    let source_note = if source != "(not specified)" {
        format!("Source: {source}")
    } else {
        String::new()
    };
    let source_footnote = if source != "(not specified)" {
        format!("\n   - Add source footnote: \"{source}\"")
    } else {
        String::new()
    };
    let nom_for_report = nomenclature.unwrap_or("(not specified)").to_string();
    let total = 7;
    match step {
        1 => serde_json::json!({
            "workflow": "transition",
            "step": 1, "total_steps": total,
            "instruction": resolve(wf, "transition.parse", DEFAULT_TRANSITION_PARSE_INSTRUCTION,
                &[("change", change), ("source_note", &source_note), ("today", &today)]),
            "change": change,
            "source": source,
            "effective_date": effective_date,
            "when_done": "Call workflow with workflow='transition', step=2"
        }),
        2 => {
            if let Some(nom) = nomenclature {
                // User provided their choice — proceed to search (step 3)
                serde_json::json!({
                    "workflow": "transition",
                    "step": 3, "total_steps": total,
                    "instruction": resolve(wf, "transition.search", DEFAULT_TRANSITION_NOMENCLATURE_CONFIRMED,
                        &[("nomenclature", nom), ("change", change)]),
                    "nomenclature": nom,
                    "next_tool": "factbase", "suggested_op": "search",
                    "when_done": "Call workflow with workflow='transition', step=4"
                })
            } else {
                // Ask the user for nomenclature preference
                serde_json::json!({
                    "workflow": "transition",
                    "step": 2, "total_steps": total,
                    "instruction": resolve(wf, "transition.nomenclature_question", DEFAULT_TRANSITION_NOMENCLATURE_QUESTION,
                        &[("change", change)]),
                    "awaiting_input": true,
                    "input_param": "nomenclature",
                    "when_done": "Ask the user which option they prefer, then call workflow with workflow='transition', step=2, nomenclature='<their choice>'"
                })
            }
        }
        3 => {
            let nom = nomenclature.unwrap_or("(not yet specified)");
            serde_json::json!({
                "workflow": "transition",
                "step": 3, "total_steps": total,
                "instruction": resolve(wf, "transition.search", DEFAULT_TRANSITION_NOMENCLATURE_CONFIRMED,
                    &[("nomenclature", nom), ("change", change)]),
                "nomenclature": nom,
                "next_tool": "factbase", "suggested_op": "search",
                "when_done": "Call workflow with workflow='transition', step=4"
            })
        }
        4 => {
            let nom = nomenclature.unwrap_or("(not yet specified)");
            serde_json::json!({
                "workflow": "transition",
                "step": 4, "total_steps": total,
                "instruction": resolve(wf, "transition.apply", DEFAULT_TRANSITION_APPLY_INSTRUCTION,
                    &[("change", change), ("nomenclature", nom),
                      ("effective_date", &effective_date), ("source_note", &source_note),
                      ("source_footnote", &source_footnote)]),
                "next_tool": "factbase", "suggested_op": "get_entity",
                "when_done": "Call workflow with workflow='transition', step=5"
            })
        }
        5 => serde_json::json!({
            "workflow": "transition",
            "step": 5, "total_steps": total,
            "instruction": resolve(wf, "transition.organize", DEFAULT_TRANSITION_ORGANIZE_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "organize",
            "when_done": "Call workflow with workflow='transition', step=6"
        }),
        6 => serde_json::json!({
            "workflow": "transition",
            "step": 6, "total_steps": total,
            "instruction": resolve(wf, "transition.maintain", DEFAULT_TRANSITION_MAINTAIN_INSTRUCTION, &[]),
            "next_tool": "factbase", "suggested_op": "scan",
            "when_done": "Call workflow with workflow='transition', step=7"
        }),
        _ => serde_json::json!({
            "workflow": "transition",
            "step": 7, "total_steps": total,
            "instruction": resolve(wf, "transition.report", DEFAULT_TRANSITION_REPORT_INSTRUCTION,
                &[("change", change), ("effective_date", &effective_date),
                  ("source", source), ("nomenclature", &nom_for_report)]),
            "complete": true
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::models::{Perspective, ReviewPerspective};
    use std::collections::HashMap;

    // Legacy update workflow constants — kept for test coverage only.
    const DEFAULT_UPDATE_SCAN_INSTRUCTION: &str = "Re-index the factbase to pick up file changes.\n\n1. Call factbase(op='scan') with time_budget_secs=120.\n   \u{26a0}\u{fe0f} PAGING: This tool is time-boxed. It WILL return `continue: true` with a `resume` token for any non-trivial repository.\n   When it does, you MUST call it again passing the resume token until `continue` is no longer in the response.\n   This may take many iterations \u{2014} that is normal. Do NOT stop early, skip ahead, or report partial results.\n2. Record: documents_total, temporal_coverage_pct, source_coverage_pct{ctx}";
    const DEFAULT_UPDATE_CHECK_INSTRUCTION: &str = "Run quality checks to find stale facts, missing sources, temporal gaps, and other issues.\n\n1. Call factbase(op='check') (one call \u{2014} no paging needed).\n2. Record: questions_total, breakdown by type (stale, conflict, temporal, missing)\n   - Mostly stale \u{2192} KB is aging, needs fresh sources\n   - Mostly temporal \u{2192} facts lack dates, timeline is murky\n   - Mostly missing \u{2192} claims lack evidence\n\n\u{26a0}\u{fe0f} TEMPORAL QUESTION FILTERING: After check completes, review the generated temporal questions. Some will have confidence='low' with a reason. Dismiss low-confidence temporal questions about:\n- Stable feature descriptions or capability lists\n- Glossary definitions or reference material\n- Facts sourced from official documentation pages that describe current capabilities\nOnly keep temporal questions about claims that could genuinely become outdated. Dismiss the rest via factbase(op='answer') with answer='dismiss: [reason]'.";
    const DEFAULT_UPDATE_DETECT_LINKS_INSTRUCTION: &str = "Detect cross-document links via title string matching.\n\n1. Call factbase(op='detect_links') with time_budget_secs=120.\n   \u{26a0}\u{fe0f} PAGING: This tool is time-boxed. It WILL return `continue: true` with a `resume` token for large repositories.\n   When it does, you MUST call it again passing the resume token until `continue` is no longer in the response.\n2. Record: links_detected, docs_processed\n3. Save links_detected as LINKS_BEFORE \u{2014} you'll compare after link suggestions{ctx}";
    const DEFAULT_UPDATE_CROSS_VALIDATE_INSTRUCTION: &str = "Review cross-document fact pairs to find contradictions between documents.\n\n1. Call factbase(op='fact_pairs') to retrieve embedding-similar fact pairs across documents.\n   - Each pair contains two facts from different documents with their text, line numbers, and similarity score.\n   - Pairs where a cross-check question already exists are excluded.\n\n2. For each pair, classify the relationship:\n   - CONSISTENT: Facts are compatible or about different aspects\n   - CONTRADICTS: Facts give different answers to the same question about the same entity\n   - SUPERSEDES: One fact provides newer information that replaces the other\n\n3. For CONTRADICTS or SUPERSEDES pairs, create a review question:\n   - Call factbase(op='answer') with the target doc_id, the fact's line number as question_index context,\n     and a description like: \"Cross-check with {other_doc_title}: {fact_text} \u{2014} {reason}\"\n   - Use @q[conflict] for contradictions, @q[stale] for superseded facts\n\n4. Record: pairs_reviewed, conflicts_found";
    const DEFAULT_UPDATE_LINKS_INSTRUCTION: &str = "Review link suggestions to improve cross-document connectivity.\n\n1. Call factbase(op='links') TWICE for better coverage:\n   a. Cross-type discovery: use exclude_types matching the most common doc type (e.g., exclude_types=[\"person\"] if reviewing people docs) with min_similarity=0.5. This finds connections between different entity types.\n   b. Same-type discovery: use include_types matching a specific type with min_similarity=0.7. This finds related entities of the same kind.\n2. Review each suggestion: does the candidate document genuinely relate to the source?\n3. For confirmed links, call factbase(op='links', action='store') with the source_id and target_id pairs.\n   - factbase(op='links', action='store') writes `References:` to source files and `Referenced by:` to target files, and updates the database.\n4. Record: links_added, documents_modified";
    const DEFAULT_UPDATE_ORGANIZE_INSTRUCTION: &str = "Analyze the knowledge base structure for improvement opportunities.\n\n1. Call factbase(op='organize', action='analyze') (one call \u{2014} no paging needed).\n2. Record candidates:\n   - Merge: documents that overlap significantly \u{2014} telling the same story twice\n   - Split: documents covering multiple distinct topics\n   - Misplaced: documents whose type doesn't match their content\n   - Duplicates: repeated facts across documents\n3. Do NOT execute changes \u{2014} just record what you find";
    const DEFAULT_UPDATE_SUMMARY_INSTRUCTION: &str = "Write a diagnostic report combining metrics and assessment.\n\n## Scan & Links\n- Documents: X | Links: X\n- Temporal coverage: X% | Source coverage: X%\n- Link health: [healthy / needs work / poor] \u{2014} each doc should average 1+ link\n\n## Quality Issues\n- Total questions: X (stale: X, conflict: X, temporal: X, missing: X)\n- Dominant issue type tells you the KB's biggest weakness\n\n## Organization\n- Merge/split/misplaced/duplicate candidates found\n\n## Health Assessment\nOne paragraph: overall KB health, biggest strength, biggest gap, and top 3 priorities ordered by impact.";

    /// Legacy update_step — kept for test coverage of the update workflow instruction content.
    fn update_step(
        step: usize,
        args: &Value,
        perspective: &Option<Perspective>,
        wf: &WorkflowsConfig,
        db: &Database,
    ) -> Value {
        let ctx = perspective_context(perspective);
        let do_cv = args
            .get("cross_validate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let total = if do_cv { 7 } else { 6 };
        match step {
            1 => {
                let mut resp = serde_json::json!({
                    "workflow": "update",
                    "step": 1, "total_steps": total,
                    "instruction": resolve(wf, "update.scan", DEFAULT_UPDATE_SCAN_INSTRUCTION, &[("ctx", &ctx)]),
                    "next_tool": "factbase", "suggested_op": "scan",
                    "when_done": "Call workflow with workflow='update', step=2"
                });
                if let Some((reason, doc_count)) = detect_full_rebuild(db) {
                    let est_secs = doc_count * 2;
                    let est_display = if est_secs >= 60 {
                        format!("~{} minutes", est_secs / 60)
                    } else {
                        format!("~{est_secs} seconds")
                    };
                    resp["requires_confirmation"] = Value::Bool(true);
                    resp["confirmation_reason"] = Value::String("full embedding rebuild".into());
                    resp["confirmation_details"] = Value::String(format!(
                        "All {doc_count} documents need re-embedding because {reason}. Estimated time: {est_display}."
                    ));
                    resp["instruction"] = Value::String(format!(
                        "\u{26a0}\u{fe0f} Embedding rebuild required: {doc_count} documents need re-embedding ({reason}). \
                         Estimated time: {est_display}.\n\n\
                         If the user has already confirmed (e.g., said \"go ahead\" or \"re-embed\"), proceed immediately \
                         by calling factbase(op='scan') with force_reindex=true and time_budget_secs=120.\n\n\
                         Otherwise, ask the user to confirm before proceeding.\n\n\
                         For incremental updates after confirmation, call workflow(workflow='update', step=2) when done."
                    ));
                }
                resp
            }
            2 => serde_json::json!({
                "workflow": "update",
                "step": 2, "total_steps": total,
                "instruction": resolve(wf, "update.detect_links", DEFAULT_UPDATE_DETECT_LINKS_INSTRUCTION, &[("ctx", &ctx)]),
                "next_tool": "factbase", "suggested_op": "detect_links",
                "when_done": "Call workflow with workflow='update', step=3"
            }),
            3 => serde_json::json!({
                "workflow": "update",
                "step": 3, "total_steps": total,
                "instruction": resolve(wf, "update.check", DEFAULT_UPDATE_CHECK_INSTRUCTION, &[]),
                "next_tool": "factbase", "suggested_op": "check",
                "when_done": "Call workflow with workflow='update', step=4"
            }),
            4 => serde_json::json!({
                "workflow": "update",
                "step": 4, "total_steps": total,
                "instruction": resolve(wf, "update.links", DEFAULT_UPDATE_LINKS_INSTRUCTION, &[]),
                "next_tool": "factbase", "suggested_op": "links",
                "when_done": "Call workflow with workflow='update', step=5"
            }),
            5 => {
                if do_cv {
                    serde_json::json!({
                        "workflow": "update",
                        "step": 5, "total_steps": total,
                        "instruction": resolve(wf, "update.cross_validate", DEFAULT_UPDATE_CROSS_VALIDATE_INSTRUCTION, &[]),
                        "next_tool": "factbase", "suggested_op": "fact_pairs",
                        "when_done": "Call workflow with workflow='update', step=6"
                    })
                } else {
                    serde_json::json!({
                        "workflow": "update",
                        "step": 5, "total_steps": total,
                        "instruction": resolve(wf, "update.organize", DEFAULT_UPDATE_ORGANIZE_INSTRUCTION, &[]),
                        "next_tool": "factbase", "suggested_op": "organize",
                        "when_done": format!("Call workflow with workflow='update', step={}", total)
                    })
                }
            }
            6 => {
                if do_cv {
                    serde_json::json!({
                        "workflow": "update",
                        "step": 6, "total_steps": total,
                        "instruction": resolve(wf, "update.organize", DEFAULT_UPDATE_ORGANIZE_INSTRUCTION, &[]),
                        "next_tool": "factbase", "suggested_op": "organize",
                        "when_done": "Call workflow with workflow='update', step=7"
                    })
                } else {
                    serde_json::json!({
                        "workflow": "update",
                        "step": 6, "total_steps": total,
                        "instruction": resolve(wf, "update.summary", DEFAULT_UPDATE_SUMMARY_INSTRUCTION, &[]),
                        "complete": true
                    })
                }
            }
            7 if do_cv => serde_json::json!({
                "workflow": "update",
                "step": 7, "total_steps": total,
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
            format: None,
            link_match_mode: None,
            citation_patterns: None,
            internal_sources: None,
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
        assert!(
            instruction.contains("NEXT:"),
            "create step should have REQUIRED NEXT routing"
        );
        assert!(
            instruction.contains("workflow(workflow='ingest', step=4)"),
            "should route to step 4"
        );
    }

    #[test]
    fn test_ingest_create_recommends_bulk() {
        let step = ingest_step(3, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("factbase(op='bulk_create')"),
            "create step should recommend bulk_create_documents"
        );
        assert_eq!(
            step["next_tool"].as_str().unwrap(),
            "factbase",
            "next_tool should be factbase"
        );
    }

    #[test]
    fn test_ingest_verify_no_dry_run() {
        let step = ingest_step(4, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            !instruction.contains("dry_run"),
            "verify step should not mention dry_run"
        );
        assert!(
            step.get("suggested_args").is_none(),
            "verify step should not have suggested_args with dry_run"
        );
        assert!(
            instruction.contains("doc_ids"),
            "verify step should tell agent to use doc_ids"
        );
    }

    #[test]
    fn test_ingest_verify_has_required_next() {
        let step = ingest_step(4, &serde_json::json!({}), &None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("NEXT:"),
            "verify step should have REQUIRED NEXT routing"
        );
        assert!(
            instruction.contains("workflow(workflow='ingest', step=5)"),
            "should route to step 5"
        );
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
        assert!(
            instruction.contains("SOURCE REQUIREMENT"),
            "ingest create must have source requirement"
        );
        assert!(
            instruction.contains("independently verifiable"),
            "must mention independently verifiable"
        );
        assert!(
            instruction.contains("GOOD citations"),
            "must list good citation examples"
        );
        assert!(
            instruction.contains("BAD citations"),
            "must list bad citation examples"
        );
    }

    #[test]
    fn test_enrich_research_has_source_requirement() {
        let (db, _tmp) = test_db();
        let step = enrich_step(3, &serde_json::json!({}), &None, &db, None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("SOURCE REQUIREMENT"),
            "enrich research must have source requirement"
        );
        assert!(
            instruction.contains("vague"),
            "must mention checking existing vague citations"
        );
    }

    #[test]
    fn test_resolve_intro_has_weak_source_guidance() {
        let intro = DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION;
        assert!(
            intro.contains("WEAK-SOURCE"),
            "resolve intro must have WEAK-SOURCE guidance"
        );
        assert!(
            intro.contains("UNVERIFIED"),
            "must mention UNVERIFIED fallback"
        );
        assert!(
            intro.contains("MAKE THE CITATION MORE SPECIFIC"),
            "must instruct agent to improve citation, not defend it"
        );
        assert!(
            intro.contains("NEVER answer"),
            "must explicitly forbid 'citation is sufficient' answers"
        );
    }

    #[test]
    fn test_type_evidence_weak_source_constructs_url() {
        let guidance = type_evidence_guidance(&QuestionType::WeakSource);
        assert!(
            guidance.contains("MAKE THE CITATION MORE SPECIFIC"),
            "weak-source guidance must instruct agent to improve citation"
        );
        assert!(
            guidance.contains("phonetool.amazon.com"),
            "must give Phonetool URL pattern as example"
        );
        assert!(
            guidance.contains("Slack #channel-name"),
            "must give Slack channel pattern as example"
        );
        assert!(
            guidance.contains("UNVERIFIED"),
            "weak-source guidance must mention UNVERIFIED fallback"
        );
        assert!(
            guidance.contains("Do not invent"),
            "must warn against inventing citations"
        );
        assert!(
            guidance.contains("NEVER answer"),
            "must explicitly forbid 'citation is sufficient' answers"
        );
    }

    #[test]
    fn test_variant_type_evidence_intro_has_weak_source() {
        let intro = VARIANT_TYPE_EVIDENCE_INTRO;
        assert!(
            intro.contains("WEAK-SOURCE"),
            "variant intro must have WEAK-SOURCE guidance"
        );
        assert!(
            intro.contains("MAKE THE CITATION MORE SPECIFIC"),
            "variant must instruct agent to improve citation"
        );
        assert!(
            intro.contains("phonetool.amazon.com"),
            "variant must give Phonetool URL pattern as example"
        );
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
        // Deferred items should NOT appear in instruction (agents misinterpret as "stop")
        assert!(
            !instruction.contains("deferred"),
            "instruction must not mention deferred items"
        );
        // But the count is still available as structured data
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
        assert_eq!(order[1], "stale"); // 2 questions
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
        assert_eq!(order[0], "temporal"); // priority 0
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
        assert!(
            intro.contains("PARALLEL DISPATCH"),
            "intro should contain fan-out hint"
        );
        assert!(
            intro.contains("question_type="),
            "hint should show question_type param"
        );
        assert!(!intro.contains("optional"), "hint should not say optional");
    }

    #[test]
    fn test_subagent_fanout_hint_large_queue_mandatory() {
        let types = vec![("stale".to_string(), 150), ("temporal".to_string(), 80)];
        let hint = subagent_fanout_hint(230, &types);
        assert!(
            hint.contains("MANDATORY"),
            "large queue should use MANDATORY language"
        );
        assert!(
            hint.contains("USE IT NOW"),
            "large queue should say USE IT NOW"
        );
        assert!(
            hint.contains("question_type='stale'"),
            "should include actual types"
        );
        assert!(
            hint.contains("question_type='temporal'"),
            "should include actual types"
        );
    }

    #[test]
    fn test_subagent_fanout_hint_small_queue_not_mandatory() {
        let types = vec![("stale".to_string(), 5)];
        let hint = subagent_fanout_hint(5, &types);
        assert!(
            !hint.contains("MANDATORY"),
            "small queue should not use MANDATORY"
        );
        assert!(
            hint.contains("PARALLEL DISPATCH"),
            "should still suggest parallelism"
        );
    }

    #[test]
    fn test_resolve_answer_instruction_action_framing() {
        assert!(DEFAULT_RESOLVE_ANSWER_INSTRUCTION.contains("ANSWER questions, not analyze"));
        assert!(
            DEFAULT_RESOLVE_ANSWER_INSTRUCTION.contains("not factbase(op='answer') is reducing")
        );
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
        assert!(
            intro.contains("CONFLICT"),
            "intro should cover conflict type"
        );
        assert!(
            intro.contains("[pattern:"),
            "intro should mention pattern tags"
        );
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
        assert!(
            intro.contains("TEMPORAL"),
            "intro should cover temporal type"
        );
        assert!(intro.contains("@t[YYYY]"), "intro must show tag format");
        assert!(
            intro.contains("verified"),
            "intro must require verification date"
        );
        assert!(intro.contains("WRONG"), "intro must flag rejected answers");
        assert!(
            intro.contains("well-known"),
            "intro must explicitly name 'well-known' as rejected"
        );
        assert!(
            intro.contains("no audit trail"),
            "intro must explain why dismissals are rejected"
        );
    }

    #[test]
    fn test_improve_resolve_temporal_requires_tag_in_answer() {
        let (db, _tmp) = test_db();
        let step = improve_step(2, Some("abc123"), &None, &[], &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("MISSING an @t[...]"),
            "improve/resolve temporal guidance must explain the fact line is missing a tag"
        );
        assert!(
            instruction.contains("tag must appear in your answer"),
            "must require the @t tag in the answer"
        );
        assert!(
            instruction.contains("static fact"),
            "improve/resolve must reject 'static fact' dismissals"
        );
        assert!(
            instruction.contains("no audit trail"),
            "improve/resolve must explain why dismissals are rejected"
        );
    }

    // --- improve workflow tests ---

    #[test]
    fn test_improve_step1_cleanup() {
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["workflow"], "improve");
        assert_eq!(step["step"], 1);
        assert_eq!(step["total_steps"], 5);
        assert_eq!(step["step_name"], "cleanup");
        assert_eq!(step["doc_id"], "abc123");
        assert!(step["instruction"].as_str().unwrap().contains("abc123"));
        assert_eq!(step["next_tool"], "factbase");
    }

    #[test]
    fn test_improve_step2_resolve() {
        let (db, _tmp) = test_db();
        let step = improve_step(2, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "resolve");
        assert_eq!(step["next_tool"], "factbase");
        assert_eq!(step["policy"]["stale_days"], 365);
    }

    #[test]
    fn test_improve_step3_enrich() {
        let (db, _tmp) = test_db();
        let step = improve_step(3, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "enrich");
        assert_eq!(step["next_tool"], "factbase");
    }

    #[test]
    fn test_improve_step4_scan() {
        let (db, _tmp) = test_db();
        let step = improve_step(4, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "scan");
        assert_eq!(step["next_tool"], "factbase");
        assert_eq!(step["suggested_op"], "scan");
        assert!(step["instruction"].as_str().unwrap().contains("factbase(op='scan')"));
    }

    #[test]
    fn test_improve_step5_check() {
        let (db, _tmp) = test_db();
        let step = improve_step(5, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "check");
        assert_eq!(step["next_tool"], "factbase");
        assert!(step["complete"].as_bool().unwrap());
    }

    #[test]
    fn test_improve_past_last_step() {
        let (db, _tmp) = test_db();
        let step = improve_step(6, Some("abc123"), &None, &[], &db, &wf());
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
        assert_eq!(step["total_steps"], 4);
    }

    #[test]
    fn test_improve_skip_multiple() {
        let skip = vec!["cleanup".to_string(), "enrich".to_string()];
        let (db, _tmp) = test_db();
        let step1 = improve_step(1, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step1["step_name"], "resolve");
        assert_eq!(step1["total_steps"], 3);
        let step2 = improve_step(2, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step2["step_name"], "scan");
        let step3 = improve_step(3, Some("abc123"), &None, &skip, &db, &wf());
        assert_eq!(step3["step_name"], "check");
        assert!(step3["complete"].as_bool().unwrap());
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
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .contains("current_role"));
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
        let content_b =
            "<!-- factbase:bbb001 -->\n# Beta\n\n- Fact one @t[2024-01] [^1]\n\n---\n[^1]: Source";
        insert_test_doc(&db, "aaa001", content_a);
        insert_test_doc(&db, "bbb001", content_b);
        let step = enrich_step(1, &serde_json::json!({}), &None, &db, None, &wf());
        let quality = step["entity_quality"].as_array().unwrap();
        assert_eq!(quality.len(), 2);
        // First item should have higher attention_score (aaa001 has no tags/sources)
        let first_score = quality[0]["attention_score"].as_u64().unwrap();
        let second_score = quality[1]["attention_score"].as_u64().unwrap();
        assert!(
            first_score >= second_score,
            "should be sorted by attention_score desc"
        );
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
        let (db, _tmp) = test_db();
        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("factbase(op='scan')"),
            "should call factbase(op='scan')"
        );
    }

    #[test]
    fn test_enrich_step3_mentions_link_detection() {
        let (db, _tmp) = test_db();
        let step = enrich_step(3, &serde_json::json!({}), &None, &db, None, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.contains("link detection"),
            "enrich step 3 should mention link detection"
        );
        assert!(
            instruction.contains("exact title"),
            "should emphasize exact titles"
        );
        assert!(
            instruction.contains("Preserve ALL existing content"),
            "should warn about preserving content"
        );
    }

    #[test]
    fn test_enrich_step4_scan() {
        let (db, _tmp) = test_db();
        let step = enrich_step(4, &serde_json::json!({}), &None, &db, None, &wf());
        assert_eq!(step["workflow"], "enrich");
        assert_eq!(step["step"], 4);
        assert_eq!(step["total_steps"], 5);
        assert_eq!(step["next_tool"], "factbase");
        assert_eq!(step["suggested_op"], "scan");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("factbase(op='scan')"), "enrich step 4 must call scan");
        assert!(step.get("complete").is_none() || !step["complete"].as_bool().unwrap_or(false),
            "scan step should not be the final step");
    }

    #[test]
    fn test_enrich_step5_verify_is_complete() {
        let (db, _tmp) = test_db();
        let step = enrich_step(5, &serde_json::json!({}), &None, &db, None, &wf());
        assert_eq!(step["step"], 5);
        assert_eq!(step["total_steps"], 5);
        assert!(step["complete"].as_bool().unwrap(), "step 5 should be complete");
    }

    #[test]
    fn test_improve_scan_step_calls_scan() {
        let (db, _tmp) = test_db();
        let step = improve_step(4, Some("abc123"), &None, &[], &db, &wf());
        assert_eq!(step["step_name"], "scan");
        assert_eq!(step["suggested_op"], "scan");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("factbase(op='scan')"), "improve scan step must call scan");
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
        assert_eq!(step["next_tool"], "filesystem");
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
        assert_eq!(step["next_tool"], "factbase");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("factbase(op='create')"));
        assert!(instruction.contains("@t[=2024]"));
        assert!(instruction.contains("[^1]"));
        assert!(instruction.contains("factbase(op='authoring_guide')"));
    }

    #[test]
    fn test_setup_step5_scan_and_verify() {
        let step = setup_step(5, &serde_json::json!({}), &wf());
        assert_eq!(step["step"], 5);
        assert_eq!(step["next_tool"], "factbase");
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.contains("factbase(op='scan')"));
        assert!(instruction.contains("factbase(op='check')"));
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
                instruction.contains("factbase(op='authoring_guide')"),
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
        // No obsidian_git_setup when no path/perspective
        assert!(step.get("obsidian_git_setup").is_none());
    }

    #[test]
    fn test_setup_step6_obsidian_git_setup_note() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Test\nformat:\n  preset: obsidian\n",
        )
        .unwrap();
        let step = setup_step(6, &serde_json::json!({"path": path}), &wf());
        assert!(step["complete"].as_bool().unwrap());
        let git_setup = &step["obsidian_git_setup"];
        assert!(
            git_setup.is_object(),
            "obsidian_git_setup should be present for obsidian preset"
        );
        let files = git_setup["files_to_commit"].as_array().unwrap();
        let file_strs: Vec<&str> = files.iter().filter_map(|v| v.as_str()).collect();
        assert!(file_strs.contains(&".obsidian/snippets/factbase.css"));
        assert!(file_strs.contains(&".obsidian/app.json"));
        assert!(file_strs.contains(&".gitignore"));
        // Verify files were actually written
        assert!(
            tmp.path().join(".obsidian/snippets/factbase.css").exists(),
            "CSS file should be written"
        );
        assert!(
            tmp.path().join(".obsidian/app.json").exists(),
            "app.json should be written"
        );
    }

    #[test]
    fn test_create_step6_obsidian_writes_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Test\nformat:\n  preset: obsidian\n",
        )
        .unwrap();
        // No domain → step 6 is the final step
        let step = setup_step(6, &serde_json::json!({"path": path}), &wf());
        assert!(step["complete"].as_bool().unwrap());
        assert!(
            step["obsidian_git_setup"].is_object(),
            "obsidian_git_setup should be present"
        );
        assert!(
            tmp.path().join(".obsidian/snippets/factbase.css").exists(),
            "CSS snippet should be written"
        );
        assert!(
            tmp.path().join(".obsidian/app.json").exists(),
            "app.json should be written"
        );
        let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(
            gitignore.contains(".obsidian/snippets/"),
            ".gitignore should track snippets dir"
        );
    }

    #[test]
    fn test_create_final_step_no_domain_obsidian_writes_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: Test\nformat:\n  preset: obsidian\n",
        )
        .unwrap();
        // create_step without domain: step 6 falls to _ => branch
        let step = create_step(6, &serde_json::json!({"path": path}), &wf());
        assert_eq!(step["workflow"], "create");
        assert!(step["complete"].as_bool().unwrap());
        assert!(
            step["obsidian_git_setup"].is_object(),
            "obsidian_git_setup should be present for create final step"
        );
        assert!(
            tmp.path().join(".obsidian/snippets/factbase.css").exists(),
            "CSS snippet should be written by create final step"
        );
        assert!(
            tmp.path().join(".obsidian/app.json").exists(),
            "app.json should be written by create final step"
        );
        let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(
            gitignore.contains(".obsidian/snippets/"),
            ".gitignore should be updated by create final step"
        );
    }

    #[test]
    fn test_create_final_step_no_obsidian_no_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(tmp.path().join("perspective.yaml"), "focus: Test\n").unwrap();
        let step = create_step(6, &serde_json::json!({"path": path}), &wf());
        assert!(step["complete"].as_bool().unwrap());
        assert!(
            step.get("obsidian_git_setup").is_none(),
            "no obsidian_git_setup for non-obsidian preset"
        );
        assert!(
            !tmp.path().join(".obsidian").exists(),
            ".obsidian dir should not be created"
        );
    }

    #[test]
    fn test_setup_step6_no_obsidian_git_setup_for_non_obsidian() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(tmp.path().join("perspective.yaml"), "focus: Test\n").unwrap();
        let step = setup_step(6, &serde_json::json!({"path": path}), &wf());
        assert!(step.get("obsidian_git_setup").is_none());
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
        let prompt = build_bootstrap_prompt("mycology", None, &prompts, None);
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
        let prompt = build_bootstrap_prompt(
            "mycology",
            Some("species, habitats, researchers"),
            &prompts,
            None,
        );
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
        assert!(
            instruction.contains("mycology"),
            "instruction should contain domain"
        );
        assert!(
            instruction.contains("document_types"),
            "instruction should describe expected format"
        );
        assert!(result["next_steps"].is_array());
        let steps = result["next_steps"].as_array().unwrap();
        let all_steps = steps
            .iter()
            .map(|s| s.as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            all_steps.contains("setup"),
            "next_steps should route to setup workflow"
        );
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
            None,
        );
        assert!(prompt.contains("aviation"));
        assert!(prompt.contains("aircraft, airlines, airports"));
        assert!(prompt.contains("document_types"));
        assert!(prompt.contains("templates"));
    }

    #[test]
    fn test_build_bootstrap_prompt_file_override() {
        let tmp = tempfile::TempDir::new().unwrap();
        let prompts_dir = tmp.path().join(".factbase").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(
            prompts_dir.join("bootstrap.txt"),
            "Custom bootstrap for {domain}",
        )
        .unwrap();
        let prompts = crate::config::PromptsConfig::default();
        let result = build_bootstrap_prompt("mycology", None, &prompts, Some(tmp.path()));
        assert_eq!(result, "Custom bootstrap for mycology");
    }

    #[test]
    fn test_build_bootstrap_prompt_no_override_uses_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let prompts = crate::config::PromptsConfig::default();
        let result = build_bootstrap_prompt("mycology", None, &prompts, Some(tmp.path()));
        assert!(
            result.contains("document_types"),
            "should use compiled-in default"
        );
    }

    #[test]
    fn test_workflow_list_includes_bootstrap() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        // bootstrap is now an alias for create
        let aliases = result["aliases"].as_object().unwrap();
        assert_eq!(aliases["bootstrap"], "create");
        let workflows = result["workflows"].as_array().unwrap();
        let names: Vec<&str> = workflows
            .iter()
            .filter_map(|w| w["name"].as_str())
            .collect();
        assert!(names.contains(&"create"));
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
        assert!(
            FORMAT_RULES.contains("Wolfgang Amadeus Mozart"),
            "missing entity name"
        );
        // Descriptions
        assert!(
            FORMAT_RULES.contains("Complex counterpoint"),
            "missing description"
        );
        // Statuses
        assert!(
            FORMAT_RULES.contains("Active Production Status"),
            "missing status"
        );
        // Statistics
        assert!(
            FORMAT_RULES.contains("Total Produced: 650+"),
            "missing statistic"
        );
        // Vague time words
        assert!(FORMAT_RULES.contains("seasonal"), "missing vague time word");
    }

    #[test]
    fn test_bootstrap_prompt_has_temporal_tag_negative_examples() {
        assert!(
            DEFAULT_BOOTSTRAP_PROMPT.contains("NEVER names, descriptions"),
            "missing negative guidance in bootstrap prompt"
        );
        assert!(
            DEFAULT_BOOTSTRAP_PROMPT.contains("Wolfgang Amadeus Mozart"),
            "missing entity name example in bootstrap prompt"
        );
    }

    // --- workflow config override tests ---

    #[test]
    fn test_workflow_config_override_in_step() {
        let (db, _tmp) = test_db();
        let mut wfc = WorkflowsConfig::default();
        wfc.templates
            .insert("update.scan".into(), "Custom scan: {ctx}".into());
        let p = mock_perspective();
        let step = update_step(1, &serde_json::json!({}), &p, &wfc, &db);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(instruction.starts_with("Custom scan:"));
        assert!(instruction.contains("Acme Corp"));
    }

    #[test]
    fn test_workflow_config_override_improve() {
        let mut wfc = WorkflowsConfig::default();
        wfc.templates
            .insert("improve.cleanup".into(), "My cleanup for {doc_hint}".into());
        let (db, _tmp) = test_db();
        let step = improve_step(1, Some("abc123"), &None, &[], &db, &wfc);
        assert!(step["instruction"]
            .as_str()
            .unwrap()
            .starts_with("My cleanup for"));
        assert!(step["instruction"].as_str().unwrap().contains("abc123"));
    }

    #[test]
    fn test_workflow_config_fallback_to_default() {
        // Empty config should produce the same output as default
        let (db, _tmp) = test_db();
        let step_default = update_step(2, &serde_json::json!({}), &None, &wf(), &db);
        let step_empty = update_step(
            2,
            &serde_json::json!({}),
            &None,
            &WorkflowsConfig::default(),
            &db,
        );
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
        assert!(
            step["intro"].is_string(),
            "first batch should include intro"
        );
        let intro = step["intro"].as_str().unwrap();
        assert!(
            intro.contains("TEMPORAL"),
            "intro should describe question types"
        );
        assert!(
            intro.contains("STALE"),
            "intro should describe question types"
        );
    }

    #[test]
    fn test_resolve_step2_subsequent_batch_no_intro() {
        let (db, _tmp) = test_db();
        // Insert a doc with an answered question (resolved) and an unanswered one
        let content = "<!-- factbase:sub001 -->\n# Sub Test\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[missing]` Unanswered (line 5)\n";
        insert_test_doc(&db, "sub001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // resolved_so_far > 0, so no intro
        assert!(
            step.get("intro").is_none(),
            "subsequent batch should not include intro"
        );
    }

    #[test]
    fn test_resolve_step2_subsequent_batch_slim_instruction() {
        let (db, _tmp) = test_db();
        // One answered + one unanswered → resolved_so_far > 0 → subsequent batch
        let content = "<!-- factbase:slm001 -->\n# Slim Test\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[missing]` Unanswered (line 5)\n";
        insert_test_doc(&db, "slm001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.len() < 500,
            "subsequent batch instruction should be compact: {instr}"
        );
        assert!(instr.contains("LOOP"), "should include LOOP: {instr}");
    }

    #[test]
    fn test_resolve_step2_first_batch_full_instruction() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "ful001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.len() > 100,
            "first batch instruction should be full: len={}",
            instr.len()
        );
    }

    #[test]
    fn test_resolve_step2_subsequent_batch_no_patterns() {
        let (db, _tmp) = test_db();
        // Create 5 identical weak-source questions + 1 answered to make it a subsequent batch
        for i in 0..5 {
            let id = format!("snp{:03}", i);
            let content = format!(
                "<!-- factbase:{id} -->\n# Doc {id}\n\n- Fact [^1]\n\n---\n[^1]: Phonetool lookup\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1] \"Phonetool lookup\" is not specific enough to verify (line 4)\n"
            );
            insert_test_doc(&db, &id, &content);
        }
        // Add an answered question to make resolved_so_far > 0
        let ans = "<!-- factbase:snpans -->\n# Answered\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Done (line 4)\n  > @t[2024]\n";
        insert_test_doc(&db, "snpans", ans);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("patterns_detected").is_none(),
            "subsequent batch should omit patterns_detected"
        );
    }

    #[test]
    fn test_resolve_step2_conflict_patterns_omitted_when_no_conflicts() {
        let (db, _tmp) = test_db();
        // Only temporal questions, no conflicts — and subsequent batch
        let content = "<!-- factbase:ncf001 -->\n# No Conflict\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[temporal]` Unanswered (line 5)\n";
        insert_test_doc(&db, "ncf001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("conflict_patterns").is_none(),
            "no conflicts in batch → omit conflict_patterns"
        );
    }

    #[test]
    fn test_resolve_step2_conflict_patterns_present_when_batch_has_conflicts() {
        let (db, _tmp) = test_db();
        // Subsequent batch with a conflict question
        let content = "<!-- factbase:ycf001 -->\n# Has Conflict\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered (line 4)\n  > @t[2024]\n- [ ] `@q[conflict]` Two facts overlap (line 5)\n";
        insert_test_doc(&db, "ycf001", content);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("conflict_patterns").is_some(),
            "batch with conflicts should include conflict_patterns"
        );
    }

    #[test]
    fn test_resolve_step2_first_batch_has_all_fields() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(
            &db,
            "all001",
            &["temporal", "conflict", "missing", "stale", "weak-source"],
        );
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // First batch should have everything
        assert!(step.get("intro").is_some(), "first batch should have intro");
        assert!(
            step.get("conflict_patterns").is_some(),
            "first batch should have conflict_patterns"
        );
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.len() > 100,
            "first batch should have full instruction"
        );
    }

    #[test]
    fn test_resolve_step2_slim_batch_smaller_than_first() {
        let (db, _tmp) = test_db();
        // First batch: all questions unanswered
        insert_doc_with_questions(&db, "sz1001", &["temporal", "stale"]);
        let first = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let first_json = serde_json::to_string(&first).unwrap();

        // Subsequent batch: one answered
        let (db2, _tmp2) = test_db();
        let content = "<!-- factbase:sz2001 -->\n# Slim\n\n- Fact\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Done (line 4)\n  > @t[2024]\n- [ ] `@q[stale]` Stale (line 5)\n";
        insert_test_doc(&db2, "sz2001", content);
        let second = resolve_step(2, &serde_json::json!({}), &None, 0, &db2, &wf());
        let second_json = serde_json::to_string(&second).unwrap();

        assert!(
            second_json.len() < first_json.len(),
            "subsequent batch ({} bytes) should be smaller than first ({} bytes)",
            second_json.len(),
            first_json.len()
        );
    }

    #[test]
    fn test_resolve_step2_batch_size_limits_questions() {
        let (db, _tmp) = test_db();
        // Insert 70 questions across seven docs (more than default batch size of 30)
        let types_10: Vec<&str> = vec!["temporal"; 10];
        for i in 0..7 {
            insert_doc_with_questions(&db, &format!("big{:03}", i), &types_10);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        assert_eq!(
            batch["questions"].as_array().unwrap().len(),
            wf().resolve_batch_size()
        );
        assert_eq!(batch["questions_remaining"], 70);
        assert_eq!(batch["total_batches_estimate"], 3);
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
            .map(|i| {
                serde_json::json!({
                    "type": "temporal",
                    "description": format!("Missing date (line {i})"),
                })
            })
            .collect();
        let patterns = detect_question_patterns(&questions, &questions);
        assert!(
            patterns.is_empty(),
            "groups of 3 or fewer should not be surfaced"
        );
    }

    #[test]
    fn test_detect_question_patterns_multiple_groups() {
        let mut questions: Vec<Value> = (0..5)
            .map(|i| {
                serde_json::json!({
                    "type": "weak-source",
                    "description": format!(r#"Citation [^{i}] "src" is not specific"#),
                })
            })
            .collect();
        questions.extend((0..6).map(|i| {
            serde_json::json!({
                "type": "stale",
                "description": format!(r#""fact {i}" - source from 2020-01-01 may be outdated"#),
            })
        }));
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
        assert!(
            step.get("patterns_detected").is_none(),
            "should not include patterns_detected when no patterns"
        );
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
        wfc.templates
            .insert("resolve.answer_intro".into(), "Custom intro {ctx}".into());
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
        assert!(
            gate.contains("resolved"),
            "gate should have resolved count: {gate}"
        );
        assert!(
            gate.contains("0/2"),
            "gate should have compact counts: {gate}"
        );
        assert!(
            gate.contains("step=2"),
            "gate should tell agent to call step=2: {gate}"
        );
    }

    #[test]
    fn test_resolve_step2_has_checkpoint_fields() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "prg001", &["temporal", "stale"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // checkpoint fields removed from response — file still written to disk
        assert!(
            step.get("progress").is_none(),
            "progress should not be in response"
        );
        assert!(
            step.get("checkpoint_file").is_none(),
            "checkpoint_file removed from response"
        );
        assert!(
            step.get("checkpoint_hint").is_none(),
            "checkpoint_hint removed from response"
        );
    }

    #[test]
    fn test_resolve_step2_empty_queue_has_no_completion_gate() {
        let (db, _tmp) = test_db();
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("completion_gate").is_none());
        assert!(step.get("progress").is_none());
    }

    #[test]
    fn test_resolve_step2_answer_instruction_discourages_early_stopping() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "skip01", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("DO NOT STOP"),
            "instruction should be assertive: {instr}"
        );
        assert!(
            !instr.contains("report progress honestly"),
            "should not encourage progress reporting: {instr}"
        );
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
            assert!(
                instr.contains("CONTINUATION"),
                "variant {variant} should have CONTINUATION in instruction"
            );
            assert!(
                instr.contains("If you must stop"),
                "variant {variant} should allow partial completion"
            );
            assert_eq!(
                step["continue"], true,
                "variant {variant} should have continue=true"
            );
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
    fn test_check_repository_not_in_factbase_scan_params() {
        // check op should be mentioned in the compact description
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(desc.contains("check"), "factbase should mention check op");
    }

    #[test]
    fn test_factbase_schema_mentions_embeddings_for_scan() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let desc = fb["description"].as_str().unwrap();
        assert!(
            desc.contains("embeddings"),
            "factbase description should mention embeddings"
        );
    }

    #[test]
    fn test_factbase_schema_has_force_reindex() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let props = fb["inputSchema"]["properties"].as_object().unwrap();
        assert!(
            props.contains_key("force_reindex"),
            "factbase should have force_reindex param"
        );
    }

    #[test]
    fn test_workflow_texts_mention_fact_pairs() {
        let (db, _tmp) = test_db();
        let update_cv = update_step(
            5,
            &serde_json::json!({"cross_validate": true}),
            &None,
            &wf(),
            &db,
        );
        let cv_instr = update_cv["instruction"].as_str().unwrap();
        assert!(
            cv_instr.contains("fact comparison") || cv_instr.contains("fact pairs"),
            "update.cross_validate should mention facts"
        );
    }

    #[test]
    fn test_workflow_texts_mention_time_budget_secs() {
        let (db, _tmp) = test_db();
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let update_scan = update_step(1, &serde_json::json!({}), &None, &wf(), &db);

        let setup_instr = setup["instruction"].as_str().unwrap();
        assert!(
            setup_instr.contains("time_budget_secs=120"),
            "setup.scan should specify time_budget_secs"
        );

        let scan_instr = update_scan["instruction"].as_str().unwrap();
        assert!(
            scan_instr.contains("time_budget_secs=120"),
            "update.scan should specify time_budget_secs"
        );
        assert!(
            scan_instr.contains("Do NOT stop early"),
            "update.scan should warn against stopping early"
        );
    }

    #[test]
    fn test_paging_instructions_use_mandatory_language() {
        let (db, _tmp) = test_db();
        // Only scan steps should have paging language
        let setup = setup_step(5, &serde_json::json!({}), &wf());
        let update_scan = update_step(1, &serde_json::json!({}), &None, &wf(), &db);

        for (name, instr) in [
            ("setup.scan", setup["instruction"].as_str().unwrap()),
            ("update.scan", update_scan["instruction"].as_str().unwrap()),
        ] {
            assert!(
                instr.contains("WILL return"),
                "{name} should say paging WILL happen"
            );
            assert!(
                instr.contains("MUST"),
                "{name} should use MUST language for continuation"
            );
        }
    }

    #[test]
    fn test_time_budget_progress_message_warns_incomplete() {
        let mut resp = serde_json::json!({"ok": true});
        crate::mcp::tools::helpers::apply_time_budget_progress(
            &mut resp,
            3,
            10,
            "check_repository",
            true,
            None,
        );
        let msg = resp["message"].as_str().unwrap();
        assert!(msg.contains("MANDATORY"));
        assert!(msg.contains("MUST"));
        assert!(msg.contains("Do NOT stop"));
        assert!(resp["when_done"].as_str().unwrap().contains("MANDATORY"));
        assert_eq!(resp["progress"]["percent_complete"], 30);
    }

    #[test]
    fn test_update_check_step() {
        let (db, _tmp) = test_db();
        let step = update_step(3, &serde_json::json!({}), &None, &wf(), &db);
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("factbase(op='check')"),
            "step 3 must instruct factbase(op='check')"
        );
    }

    #[test]
    fn test_update_cross_validate_step_when_enabled() {
        let (db, _tmp) = test_db();
        let step = update_step(
            5,
            &serde_json::json!({"cross_validate": true}),
            &None,
            &wf(),
            &db,
        );
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("factbase(op='fact_pairs')"),
            "step 5 with cross_validate=true must instruct factbase(op='fact_pairs')"
        );
    }

    #[test]
    fn test_update_cross_validate_step_skipped_when_disabled() {
        let (db, _tmp) = test_db();
        // Without cross_validate, step 5 is organize
        let step = update_step(5, &serde_json::json!({}), &None, &wf(), &db);
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("factbase(op='organize'"),
            "step 5 without cross_validate should be organize"
        );
    }

    #[test]
    fn test_update_links_step() {
        let (db, _tmp) = test_db();
        let step = update_step(4, &serde_json::json!({}), &None, &wf(), &db);
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("factbase(op='links')"),
            "step 4 must instruct factbase(op='links')"
        );
    }

    #[test]
    fn test_update_total_steps_with_cross_validate() {
        let (db, _tmp) = test_db();
        let step = update_step(
            1,
            &serde_json::json!({"cross_validate": true}),
            &None,
            &wf(),
            &db,
        );
        assert_eq!(step["total_steps"], 7);
    }

    #[test]
    fn test_update_total_steps_without_cross_validate() {
        let (db, _tmp) = test_db();
        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert_eq!(step["total_steps"], 6);
    }

    #[test]
    fn test_update_step1_full_rebuild_dimension_mismatch() {
        let (db, _tmp) = test_db();
        insert_test_doc(&db, "aaa111", "<!-- factbase:aaa111 -->\n# Test\n\n- Fact");
        // Store embedding info with a different dimension than default config
        // (no actual embedding needed — dimension check fires first)
        let config = crate::Config::load(None).unwrap_or_default();
        let wrong_dim = config.embedding.dimension + 100;
        db.set_embedding_info("some-model", wrong_dim).unwrap();

        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert_eq!(step["requires_confirmation"], true);
        assert_eq!(step["confirmation_reason"], "full embedding rebuild");
        let details = step["confirmation_details"].as_str().unwrap();
        assert!(details.contains("1 documents"), "should mention doc count");
        assert!(
            details.contains("dimension"),
            "should mention dimension change"
        );
    }

    #[test]
    fn test_update_step1_full_rebuild_model_change() {
        let (db, _tmp) = test_db();
        insert_test_doc(&db, "bbb222", "<!-- factbase:bbb222 -->\n# Test\n\n- Fact");
        let config = crate::Config::load(None).unwrap_or_default();
        // Matching dimension but different model
        db.set_embedding_info("old-model-that-doesnt-match", config.embedding.dimension)
            .unwrap();
        // Insert embedding with DB schema dimension (1024)
        db.upsert_embedding("bbb222", &vec![0.0f32; 1024]).unwrap();

        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert_eq!(step["requires_confirmation"], true);
        let details = step["confirmation_details"].as_str().unwrap();
        assert!(details.contains("model"), "should mention model change");
    }

    #[test]
    fn test_update_step1_full_rebuild_empty_embeddings() {
        let (db, _tmp) = test_db();
        insert_test_doc(&db, "ccc333", "<!-- factbase:ccc333 -->\n# Test\n\n- Fact");
        // No embeddings stored, no embedding info set

        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert_eq!(step["requires_confirmation"], true);
        let details = step["confirmation_details"].as_str().unwrap();
        assert!(
            details.contains("first-time"),
            "should mention first-time generation"
        );
    }

    #[test]
    fn test_update_step1_no_confirmation_for_incremental() {
        let (db, _tmp) = test_db();
        insert_test_doc(&db, "ddd444", "<!-- factbase:ddd444 -->\n# Test\n\n- Fact");
        let config = crate::Config::load(None).unwrap_or_default();
        db.set_embedding_info(&config.embedding.model, config.embedding.dimension)
            .unwrap();
        // Insert embedding with DB schema dimension (1024)
        db.upsert_embedding("ddd444", &vec![0.0f32; 1024]).unwrap();

        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert!(
            step.get("requires_confirmation").is_none(),
            "incremental update should not require confirmation"
        );
    }

    #[test]
    fn test_update_step1_no_confirmation_for_empty_repo() {
        let (db, _tmp) = test_db();
        // No documents at all
        let step = update_step(1, &serde_json::json!({}), &None, &wf(), &db);
        assert!(
            step.get("requires_confirmation").is_none(),
            "empty repo should not require confirmation"
        );
    }

    #[test]
    fn test_resolve_intro_requires_evidence() {
        let intro = DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION;
        assert!(
            intro.contains("EVIDENCE REQUIREMENT"),
            "intro must mention evidence requirement"
        );
        assert!(
            intro.contains("verified"),
            "intro must mention verified confidence"
        );
        assert!(
            intro.contains("believed"),
            "intro must mention believed confidence"
        );
        assert!(
            intro.contains("defer"),
            "intro must mention defer as valid action"
        );
        assert!(
            intro.contains("GOOD defer"),
            "intro must frame defer positively"
        );
    }

    #[test]
    fn test_resolve_answer_instruction_requires_research() {
        let instr = DEFAULT_RESOLVE_ANSWER_INSTRUCTION;
        assert!(
            instr.contains("ANSWER questions"),
            "answer instruction must prioritize answering"
        );
        assert!(
            instr.contains("confidence"),
            "answer instruction must mention confidence field"
        );
        assert!(
            instr.contains("Minimize research"),
            "answer instruction must discourage excessive research"
        );
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
        assert_eq!(
            questions.len(),
            1,
            "Only unanswered question should be in batch, got: {questions:?}"
        );
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
        assert_eq!(
            step["all_resolved"], true,
            "should be all_resolved when only believed remain"
        );
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
        db.update_document_content("cyc01", updated, "hash2")
            .unwrap();

        // Second batch: both are now believed, should be all_resolved
        let step2 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(
            step2["all_resolved"], true,
            "no infinite loop: believed answers not re-served"
        );
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
        db.update_document_content("dfc01", updated, "hash2")
            .unwrap();

        // Second batch: both are now deferred, should be all_resolved
        let step2 = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(
            step2["all_resolved"], true,
            "no infinite loop: deferred answers not re-served"
        );
        assert_eq!(step2["batch"]["resolved_deferred"], 2);
        assert_eq!(step2["batch"]["questions_remaining"], 0);
    }

    #[test]
    fn test_resolve_answer_instruction_prohibits_scan_check() {
        let instr = DEFAULT_RESOLVE_ANSWER_INSTRUCTION;
        assert!(
            instr.contains("Do NOT call factbase(op='scan')"),
            "answer instruction must prohibit scan"
        );
        assert!(
            instr.contains("factbase(op='check')"),
            "answer instruction must prohibit check"
        );
    }

    #[test]
    fn test_resolve_workflow_list_description_mentions_cleanup() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let resolve_wf = workflows.iter().find(|w| w["name"] == "resolve").unwrap();
        let desc = resolve_wf["description"].as_str().unwrap();
        assert!(
            desc.contains("clean"),
            "resolve description should mention cleanup"
        );
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
            assert!(
                q["evidence_guidance"].is_string(),
                "type_evidence variant should add evidence_guidance to each question"
            );
        }
        // Check type-specific guidance content
        let temporal_q = questions.iter().find(|q| q["type"] == "temporal").unwrap();
        assert!(temporal_q["evidence_guidance"]
            .as_str()
            .unwrap()
            .contains("specific event date"));
        let stale_q = questions.iter().find(|q| q["type"] == "stale").unwrap();
        assert!(stale_q["evidence_guidance"]
            .as_str()
            .unwrap()
            .contains("current year"));
        let ambiguous_q = questions.iter().find(|q| q["type"] == "ambiguous").unwrap();
        assert!(ambiguous_q["evidence_guidance"]
            .as_str()
            .unwrap()
            .contains("Check the KB first"));
    }

    #[test]
    fn test_resolve_step2_type_evidence_variant_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var003", &["temporal"]);
        let args = serde_json::json!({"variant": "type_evidence"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let intro = step["intro"].as_str().unwrap();
        assert!(
            intro.contains("varies by question type"),
            "type_evidence intro should mention type-specific evidence"
        );
        assert!(
            intro.contains("STALE:"),
            "type_evidence intro should have STALE section"
        );
        assert!(
            intro.contains("TEMPORAL:"),
            "type_evidence intro should have TEMPORAL section"
        );
        assert!(
            intro.contains("AMBIGUOUS:"),
            "type_evidence intro should have AMBIGUOUS section"
        );
        assert!(
            intro.contains("CONFLICT:"),
            "type_evidence intro should have CONFLICT section"
        );
        assert!(
            intro.contains("PRECISION:"),
            "type_evidence intro should have PRECISION section"
        );
    }

    #[test]
    fn test_resolve_step2_type_evidence_variant_answer_instruction() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "var004", &["temporal"]);
        let args = serde_json::json!({"variant": "type_evidence"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("evidence_guidance"),
            "type_evidence answer instruction should reference evidence_guidance field"
        );
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
        assert!(
            intro.contains("research-first"),
            "research_batch intro should mention research-first approach"
        );
        assert!(
            intro.contains("PHASE 1"),
            "research_batch intro should have Phase 1"
        );
        assert!(
            intro.contains("PHASE 2"),
            "research_batch intro should have Phase 2"
        );
        assert!(
            intro.contains("get_entity"),
            "research_batch intro should mention get_entity"
        );
    }

    #[test]
    fn test_resolve_step2_research_batch_variant_answer_instruction() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "rbat04", &["temporal"]);
        let args = serde_json::json!({"variant": "research_batch"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("research-first"),
            "research_batch answer instruction should mention research-first"
        );
        assert!(
            instr.contains("document group"),
            "research_batch answer instruction should mention document groups"
        );
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
            assert!(
                !guidance.is_empty(),
                "type_evidence_guidance should be non-empty for {:?}",
                qt
            );
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
        insert_doc_with_questions(
            &db,
            "cft001",
            &["temporal", "stale", "missing", "ambiguous"],
        );
        let args = serde_json::json!({"question_type": "stale,missing"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 2);
        let types: Vec<&str> = questions
            .iter()
            .filter_map(|q| q["type"].as_str())
            .collect();
        assert!(types.contains(&"stale"));
        assert!(types.contains(&"missing"));
        assert!(!types.contains(&"temporal"));
    }

    #[test]
    fn test_resolve_step2_type_distribution_not_in_response() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tdi001", &["temporal", "temporal", "stale", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // type_distribution moved to checkpoint file
        assert!(
            step.get("type_distribution").is_none(),
            "type_distribution should not be in response"
        );
    }

    #[test]
    fn test_resolve_step2_type_distribution_not_in_filtered_response() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tdf001", &["temporal", "stale", "missing"]);
        let args = serde_json::json!({"question_type": "stale"});
        let step = resolve_step(2, &args, &None, 0, &db, &wf());
        assert!(
            step.get("type_distribution").is_none(),
            "type_distribution should not be in response"
        );
        // But only stale questions in the batch
        assert_eq!(step["batch"]["questions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_resolve_step2_type_filter_in_response() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "tfr001", &["temporal", "stale"]);
        // With filter
        let step = resolve_step(
            2,
            &serde_json::json!({"question_type": "stale"}),
            &None,
            0,
            &db,
            &wf(),
        );
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
        let step = resolve_step(
            2,
            &serde_json::json!({"question_type": "conflict"}),
            &None,
            0,
            &db,
            &wf(),
        );
        assert_eq!(step["all_resolved"], true);
        assert_eq!(step["batch"]["questions_remaining"], 0);
        // type_distribution no longer in response (moved to checkpoint)
        assert!(step.get("type_distribution").is_none());
    }

    #[test]
    fn test_workflow_schema_has_variant_param() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let props = &wf_tool["inputSchema"]["properties"];
        assert!(
            props.get("variant").is_some(),
            "workflow schema should have variant param"
        );
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
        let glossary_content =
            "<!-- factbase:gls001 -->\n# Glossary\n\n- **HCLS**: Healthcare and Life Sciences\n";
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
        })
        .unwrap();

        // Insert a doc with an ambiguous acronym question about HCLS
        let doc_content = "<!-- factbase:acr001 -->\n# Project\n\n- Expanding HCLS practice\n\n<!-- factbase:review -->\n- [ ] `@q[ambiguous]` \"Expanding HCLS practice\" - what does \"HCLS\" mean in this context?\n";
        db.upsert_document(&Document {
            id: "acr001".to_string(),
            content: doc_content.to_string(),
            title: "Project".to_string(),
            file_path: "acr001.md".to_string(),
            ..Document::test_default()
        })
        .unwrap();

        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        // The HCLS question should be auto-resolved, not in the batch
        assert_eq!(
            batch["questions_remaining"], 0,
            "glossary-defined acronym question should be auto-resolved"
        );
        // resolved_so_far should include the auto-resolved question
        assert!(
            batch["resolved_so_far"].as_u64().unwrap() >= 1,
            "should count auto-resolved question"
        );
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
        })
        .unwrap();

        // Insert a doc with a non-acronym ambiguous question (location)
        let doc_content = "<!-- factbase:loc001 -->\n# Person\n\n- Lives in NYC\n\n<!-- factbase:review -->\n- [ ] `@q[ambiguous]` \"Lives in NYC\" - is this home, work, or another type of location?\n";
        db.upsert_document(&Document {
            id: "loc001".to_string(),
            content: doc_content.to_string(),
            title: "Person".to_string(),
            file_path: "loc001.md".to_string(),
            ..Document::test_default()
        })
        .unwrap();

        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let batch = &step["batch"];
        // Location question should NOT be auto-resolved
        assert_eq!(
            batch["questions_remaining"], 1,
            "non-acronym question should remain"
        );
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
            &None,
            &db,
            &wf_config,
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
        })
        .unwrap();

        let mut wf_config = WorkflowsConfig::default();
        wf_config
            .templates
            .insert("resolve.answer".into(), "CUSTOM INSTRUCTION {ctx}".into());

        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf_config);
        let instruction = step["instruction"].as_str().unwrap();
        assert!(
            instruction.starts_with("CUSTOM INSTRUCTION"),
            "should use custom prompt override"
        );
    }

    #[test]
    fn test_merge_repo_prompts_overrides_global() {
        let mut global = WorkflowsConfig::default();
        global
            .templates
            .insert("resolve.answer".into(), "global answer".into());
        global.resolve_variant = Some("baseline".into());

        let mut repo = WorkflowsConfig::default();
        repo.templates
            .insert("resolve.answer".into(), "repo answer".into());
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
        })
        .unwrap();

        // Disk file HAS review queue with weak-source questions
        let disk_content = "<!-- factbase:dsk001 -->\n# Disk Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Line 4: Citation needed\n";
        std::fs::write(repo_path.join("dsk001.md"), disk_content).unwrap();

        let docs = load_review_docs_from_disk(&db);
        assert_eq!(docs.len(), 1, "should find the doc via disk content");
        assert!(
            docs[0].content.contains("@q[weak-source]"),
            "should use disk content"
        );
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
        })
        .unwrap();

        // Disk file has weak-source questions
        let disk_content = "<!-- factbase:dsk002 -->\n# Disk Only\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Line 4: Vague citation\n- [ ] `@q[weak-source]` Line 5: Missing URL\n";
        std::fs::write(repo_path.join("dsk002.md"), disk_content).unwrap();

        // Type filter for weak-source should find the questions from disk
        // First call: triage pre-step fires (no triage_results yet)
        let triage_step = resolve_step(
            2,
            &serde_json::json!({"question_type": "weak-source"}),
            &None,
            0,
            &db,
            &wf(),
        );
        assert_eq!(triage_step["triage_pre_step"], true, "should return triage pre-step");
        let citations = triage_step["citations"].as_array().unwrap();
        assert_eq!(citations.len(), 2, "should find 2 weak-source questions from disk");

        // Second call with triage_results (all INVALID) → normal batch
        let triage_results = serde_json::json!([
            {"index": 1, "verdict": "INVALID", "suggestion": "add URL"},
            {"index": 2, "verdict": "INVALID", "suggestion": "add URL"}
        ]);
        let step = resolve_step(
            2,
            &serde_json::json!({"question_type": "weak-source", "triage_results": triage_results}),
            &None,
            0,
            &db,
            &wf(),
        );
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(
            questions.len(),
            2,
            "should find weak-source questions from disk"
        );
        assert_eq!(questions[0]["type"], "weak-source");
    }

    #[test]
    fn test_resolve_step2_doc_type_filter() {
        let (db, _tmp) = test_db();
        // Insert two docs with different doc_types, each with a review question
        let person_content = "<!-- factbase:per001 -->\n# Alice\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Missing date (line 4)\n";
        let service_content = "<!-- factbase:svc001 -->\n# S3\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Old source (line 4)\n";
        {
            use crate::database::tests::test_repo_in_db;
            use crate::models::Document;
            test_repo_in_db(&db, "test-repo", std::path::Path::new("/tmp/test"));
            db.upsert_document(&Document {
                id: "per001".to_string(),
                content: person_content.to_string(),
                title: "Alice".to_string(),
                file_path: "per001.md".to_string(),
                doc_type: Some("person".to_string()),
                ..Document::test_default()
            })
            .unwrap();
            db.upsert_document(&Document {
                id: "svc001".to_string(),
                content: service_content.to_string(),
                title: "S3".to_string(),
                file_path: "svc001.md".to_string(),
                doc_type: Some("service".to_string()),
                ..Document::test_default()
            })
            .unwrap();
        }

        // Without filter: both questions returned
        let step = resolve_step2_batch(&serde_json::json!({}), &None, &db, &wf());
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(
            questions.len(),
            2,
            "without filter should return all questions"
        );

        // With doc_type=person: only person question returned
        let step = resolve_step2_batch(
            &serde_json::json!({"doc_type": "person"}),
            &None,
            &db,
            &wf(),
        );
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(
            questions.len(),
            1,
            "doc_type=person should return only person questions"
        );
        assert_eq!(questions[0]["doc_id"], "per001");

        // With doc_type=service: only service question returned
        let step = resolve_step2_batch(
            &serde_json::json!({"doc_type": "service"}),
            &None,
            &db,
            &wf(),
        );
        let questions = step["batch"]["questions"].as_array().unwrap();
        assert_eq!(
            questions.len(),
            1,
            "doc_type=service should return only service questions"
        );
        assert_eq!(questions[0]["doc_id"], "svc001");

        // With doc_type=nonexistent: no questions
        let step = resolve_step2_batch(
            &serde_json::json!({"doc_type": "nonexistent"}),
            &None,
            &db,
            &wf(),
        );
        assert!(
            step["all_resolved"].as_bool().unwrap_or(false),
            "nonexistent doc_type should yield no questions"
        );
    }

    #[test]
    fn test_resolve_step1_deferred_does_not_block() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 713, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        // Must not contain language that causes agents to stop
        assert!(
            !instruction.contains("human attention"),
            "instruction must not say 'human attention'"
        );
        assert!(
            !instruction.contains("before proceeding"),
            "instruction must not say 'before proceeding'"
        );
        assert!(
            !instruction.contains("deferred"),
            "instruction must not mention deferred items"
        );
        // Deferred count still available as data
        assert_eq!(step["deferred_count"], 713);
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
        })
        .unwrap();

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
    fn test_continuation_guidance_small_queue_has_anti_early_stop() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 5);
        let result = build_continuation_guidance(5, 10, 50, &dist, &[]);
        // Small queues still get the anti-early-stopping directive
        let guidance = result.unwrap();
        assert!(
            guidance.contains("quit too early"),
            "should warn about early stopping even for small queues"
        );
        // But should NOT have the >100 or >500 directive
        assert!(
            !guidance.contains("DO NOT STOP"),
            "should not have strong directive for small queue"
        );
    }

    #[test]
    fn test_continuation_guidance_momentum_over_100() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::Temporal, 150);
        let result = build_continuation_guidance(150, 50, 50, &dist, &[]).unwrap();
        assert!(
            result.contains("DO NOT STOP"),
            "should have directive language"
        );
        assert!(result.contains("150"), "should mention remaining count");
        assert!(result.contains("cleared 50"), "should mention progress");
        assert!(
            result.contains("quit too early"),
            "should warn about early stopping"
        );
        // Should NOT have batch estimate (that's >500 only)
        assert!(
            !result.contains("batches)"),
            "should not have batch estimate under 500"
        );
    }

    #[test]
    fn test_continuation_guidance_batches_over_500() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::WeakSource, 4421);
        let filter = vec![QuestionType::WeakSource];
        let result = build_continuation_guidance(4421, 79, 50, &dist, &filter).unwrap();
        assert!(
            result.contains("DO NOT STOP"),
            "should have directive language"
        );
        assert!(result.contains("4421"), "should mention remaining count");
        assert!(result.contains("batches)"), "should have batch estimate");
        assert!(
            result.contains("question_type=weak-source"),
            "should include filter hint"
        );
        assert!(
            result.contains("quit too early"),
            "should warn about early stopping"
        );
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
        assert!(
            result.contains("temporal: 0 remaining"),
            "should note cleared type"
        );
        assert!(result.contains("ambiguous"), "should suggest next type");
        assert!(
            result.contains("25 remaining"),
            "should show next type count"
        );
    }

    #[test]
    fn test_continuation_guidance_only_weak_source_remains() {
        let mut dist = HashMap::new();
        dist.insert(QuestionType::WeakSource, 200);
        let result = build_continuation_guidance(200, 100, 50, &dist, &[]).unwrap();
        assert!(
            result.contains("Only weak-source remains"),
            "should note only weak-source left"
        );
        assert!(
            result.contains("repetitive patterns"),
            "should mention patterns"
        );
    }

    #[test]
    fn test_continuation_guidance_not_in_step2_response() {
        let (db, _tmp) = test_db();
        // Insert >100 questions
        let types_10: Vec<&str> = vec!["temporal"; 10];
        for i in 0..11 {
            insert_doc_with_questions(&db, &format!("cg{:03}", i), &types_10);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("continuation_guidance").is_none(),
            "continuation_guidance should not be in response (moved to checkpoint)"
        );
    }

    #[test]
    fn test_continuation_guidance_not_present_for_small_step2() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "sm001", &["temporal", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("continuation_guidance").is_none(),
            "continuation_guidance should not be in response"
        );
    }

    #[test]
    fn test_resolve_no_hardcoded_token_or_model_references() {
        let (db, _tmp) = test_db();
        // Insert enough questions to trigger all guidance paths
        let types_10: Vec<&str> = vec!["temporal"; 10];
        for i in 0..60 {
            insert_doc_with_questions(&db, &format!("tok{:03}", i), &types_10);
        }
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let json_str = serde_json::to_string(&step).unwrap().to_lowercase();
        // Must not reference specific token counts or model context sizes
        assert!(
            !json_str.contains("token"),
            "should not reference tokens: found in response"
        );
        assert!(
            !json_str.contains("context window"),
            "should not reference context window"
        );
        assert!(
            !json_str.contains("context size"),
            "should not reference context sizes"
        );
        assert!(
            !json_str.contains("128k"),
            "should not reference specific context sizes"
        );
        assert!(
            !json_str.contains("200k"),
            "should not reference specific context sizes"
        );
        assert!(
            !json_str.contains("gpt"),
            "should not reference specific models"
        );
        assert!(
            !json_str.contains("claude"),
            "should not reference specific models"
        );
        assert!(
            !json_str.contains("sonnet"),
            "should not reference specific models"
        );
    }

    // --- checkpoint file tests removed (checkpoint file no longer written) ---

    #[test]
    fn test_resolve_step2_response_has_no_removed_fields() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "nrf001", &["temporal", "stale", "missing"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        // These fields should NOT be in the response
        assert!(
            step.get("progress").is_none(),
            "progress should not be in response"
        );
        assert!(
            step.get("type_distribution").is_none(),
            "type_distribution should not be in response"
        );
        assert!(
            step.get("continuation_guidance").is_none(),
            "continuation_guidance should not be in response"
        );
        assert!(
            step.get("checkpoint_file").is_none(),
            "checkpoint_file should not be in response"
        );
        assert!(
            step.get("checkpoint_hint").is_none(),
            "checkpoint_hint should not be in response"
        );
        // These fields SHOULD be in the response
        assert!(step.get("batch").is_some(), "batch should be in response");
        assert!(
            step.get("instruction").is_some(),
            "instruction should be in response"
        );
        assert!(
            step.get("completion_gate").is_some(),
            "completion_gate should be in response"
        );
        assert!(
            step.get("when_done").is_some(),
            "when_done should be in response"
        );
        assert!(
            step.get("continue").is_some(),
            "continue should be in response"
        );
        assert!(
            step.get("variant").is_some(),
            "variant should be in response"
        );
        assert!(
            step.get("type_filter").is_some(),
            "type_filter should be in response"
        );
        assert!(
            step.get("checkpoint_file").is_none(),
            "checkpoint_file removed from response"
        );
        assert!(
            step.get("checkpoint_hint").is_none(),
            "checkpoint_hint removed from response"
        );
    }

    #[test]
    fn test_resolve_step2_response_smaller_without_progress_fields() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(
            &db,
            "rsz001",
            &["temporal", "stale", "missing", "conflict", "weak-source"],
        );
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let json_str = serde_json::to_string(&step).unwrap();
        // Response should not contain the removed field names
        assert!(
            !json_str.contains("\"progress\""),
            "response should not contain progress field"
        );
        assert!(
            !json_str.contains("\"continuation_guidance\""),
            "response should not contain continuation_guidance field"
        );
        // But should still have completion_gate with progress
        assert!(
            json_str.contains("completion_gate"),
            "response should have completion_gate"
        );
    }

    #[test]
    fn test_resolve_step2_first_batch_still_has_intro() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "fbi001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("intro").is_some(),
            "first batch should still have intro"
        );
    }

    #[test]
    fn test_resolve_step2_completion_gate_references_checkpoint() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "cg001", &["temporal"]);
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        let gate = step["completion_gate"].as_str().unwrap();
        assert!(
            gate.contains("resolved"),
            "completion_gate should show resolved count"
        );
        assert!(
            gate.contains("step=2"),
            "completion_gate should tell agent to call step=2"
        );
    }

    #[test]
    fn test_resolve_has_6_steps() {
        let (db, _tmp) = test_db();
        let step = resolve_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["total_steps"], 6);
    }

    #[test]
    fn test_resolve_step4_verify_not_complete() {
        let (db, _tmp) = test_db();
        let step = resolve_step(4, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(
            step.get("complete").is_none(),
            "step 4 should not be complete"
        );
        assert_eq!(step["next_tool"], "factbase");
        assert!(step["when_done"].as_str().unwrap().contains("step=5"));
    }

    #[test]
    fn test_resolve_step5_cleanup_scan() {
        let (db, _tmp) = test_db();
        let step = resolve_step(5, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["next_tool"], "factbase");
        let instr = step["instruction"].as_str().unwrap();
        assert!(instr.contains("scan"), "cleanup step should mention scan");
        assert!(
            !instr.contains("op='check'"),
            "cleanup step should NOT instruct running check"
        );
        assert!(
            step.get("complete").is_none(),
            "step 5 should not be complete"
        );
    }

    #[test]
    fn test_resolve_step6_reports_final_state() {
        let (db, _tmp) = test_db();
        let step = resolve_step(6, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["complete"], true);
        assert!(step.get("remaining_questions").is_some());
        assert!(step.get("deferred_questions").is_some());
    }

    #[test]
    fn test_resolve_step6_counts_remaining_questions() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "fin001", &["temporal", "stale"]);
        let step = resolve_step(6, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["remaining_questions"], 2);
        assert_eq!(step["deferred_questions"], 0);
    }

    // --- maintain workflow tests ---

    #[test]
    fn test_maintain_step1_scan() {
        let (db, _tmp) = test_db();
        let step = maintain_step(1, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 1);
        assert_eq!(step["total_steps"], 7);
        assert_eq!(step["next_tool"], "factbase");
    }

    #[test]
    fn test_maintain_step2_detect_links() {
        let (db, _tmp) = test_db();
        let step = maintain_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 2);
        assert_eq!(step["next_tool"], "factbase");
    }

    #[test]
    fn test_maintain_step3_check() {
        let (db, _tmp) = test_db();
        let step = maintain_step(3, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 3);
        assert_eq!(step["next_tool"], "factbase");
    }

    #[test]
    fn test_maintain_check_includes_temporal_filtering_guidance() {
        let (db, _tmp) = test_db();
        let step = maintain_step(3, &serde_json::json!({}), &None, 0, &db, &wf());
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("TEMPORAL QUESTION FILTERING"),
            "check instruction should include temporal filtering guidance"
        );
        assert!(
            instr.contains("confidence='low'"),
            "should mention low confidence"
        );
        assert!(
            instr.contains("dismiss"),
            "should tell agent to dismiss low-confidence questions"
        );
    }

    #[test]
    fn test_maintain_step4_links() {
        let (db, _tmp) = test_db();
        // Step 4 is now links (citation_review removed)
        let step = maintain_step(4, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 4);
        assert_eq!(step["next_tool"], "factbase");
        assert_eq!(step["suggested_op"], "links");
    }

    #[test]
    fn test_maintain_step5_organize() {
        let (db, _tmp) = test_db();
        // Step 5 is organize
        let step = maintain_step(5, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 5);
        assert_eq!(step["suggested_op"], "organize");
    }

    #[test]
    fn test_maintain_step6_resolve_delegates() {
        let (db, _tmp) = test_db();
        insert_doc_with_questions(&db, "mnt001", &["temporal", "stale"]);
        // Step 6 is resolve
        let step = maintain_step(6, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["step"], 6);
        assert_eq!(step["next_tool"], "workflow");
        assert_eq!(step["suggested_args"]["workflow"], "resolve");
        assert_eq!(step["total_unanswered"], 2);
        let instr = step["instruction"].as_str().unwrap();
        assert!(
            instr.contains("LOOP:"),
            "maintain resolve must include LOOP protocol: {instr}"
        );
        assert!(
            instr.contains("You do not decide when to stop"),
            "maintain resolve must forbid self-stopping"
        );
    }

    #[test]
    fn test_maintain_step6_skips_when_no_questions() {
        let (db, _tmp) = test_db();
        // Step 6 is resolve (skips when no questions)
        let step = maintain_step(6, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["total_unanswered"], 0);
        assert!(step["instruction"].as_str().unwrap().contains("clean"));
    }

    #[test]
    fn test_maintain_step7_report() {
        let (db, _tmp) = test_db();
        // Step 7 is the report (final step)
        let step = maintain_step(7, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "maintain");
        assert_eq!(step["complete"], true);
        assert!(step.get("remaining_questions").is_some());
        assert!(step.get("deferred_questions").is_some());
    }

    #[test]
    fn test_maintain_step7_obsidian_tip_when_obsidian_format() {
        let (db, _tmp) = test_db();
        let obsidian_perspective = Some(Perspective {
            format: Some(crate::models::format::FormatConfig {
                preset: Some("obsidian".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        // Step 7 is the final report step
        let step = maintain_step(
            7,
            &serde_json::json!({}),
            &obsidian_perspective,
            0,
            &db,
            &wf(),
        );
        let tip = step["tip"]
            .as_str()
            .expect("tip should be present for obsidian format");
        assert!(tip.contains("scan"), "tip should mention scan");
        assert!(tip.contains("renamed"), "tip should mention renaming");
    }

    #[test]
    fn test_maintain_step7_no_tip_without_obsidian_format() {
        let (db, _tmp) = test_db();
        // Step 7 is the final report step
        let step = maintain_step(7, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("tip").is_none(), "no tip without obsidian format");
    }

    #[test]
    fn test_resolve_weak_source_triage_instruction_has_key_content() {
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("VALID"));
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("INVALID"));
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("WEAK"));
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("Source authority"));
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("Specificity"));
        assert!(DEFAULT_RESOLVE_WEAK_SOURCE_TRIAGE_INSTRUCTION.contains("Fabrication risk"));
    }

    #[test]
    fn test_resolve_step2_weak_source_triage_pre_step() {
        let (db, _tmp) = test_db();
        // Insert a doc with a weak-source question
        let content = "<!-- factbase:ws001 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1] \"Phonetool lookup\" is not specific enough (line 4)\n";
        insert_test_doc(&db, "ws001", content);
        // Call with question_type=weak-source and no triage_results → triage pre-step
        let step = resolve_step(2, &serde_json::json!({"question_type": "weak-source"}), &None, 0, &db, &wf());
        assert_eq!(step["triage_pre_step"], true);
        assert!(step["citations"].is_array());
        assert!(step["triage_prompt"].as_str().unwrap().contains("VALID"));
        assert_eq!(step["continue"], true);
    }

    #[test]
    fn test_resolve_step2_triage_valid_dismisses_question() {
        let (db, _tmp) = test_db();
        let content = "<!-- factbase:ws002 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1] \"https://example.com\" is not specific (line 4)\n";
        insert_test_doc(&db, "ws002", content);
        // Apply triage with VALID verdict
        let triage_results = serde_json::json!([{"index": 1, "verdict": "VALID", "suggestion": ""}]);
        let step = resolve_step(2, &serde_json::json!({"question_type": "weak-source", "triage_results": triage_results}), &None, 0, &db, &wf());
        // After VALID dismissal, no more weak-source questions → normal batch (all_resolved)
        assert!(step.get("triage_pre_step").is_none());
    }

    #[test]
    fn test_resolve_step2_no_triage_without_weak_source_filter() {
        let (db, _tmp) = test_db();
        // Insert a doc with a weak-source question
        let content = "<!-- factbase:ws003 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1] \"Phonetool lookup\" is not specific (line 4)\n";
        insert_test_doc(&db, "ws003", content);
        // Without question_type=weak-source, no triage pre-step
        let step = resolve_step(2, &serde_json::json!({}), &None, 0, &db, &wf());
        assert!(step.get("triage_pre_step").is_none(), "triage should not fire without weak-source filter");
    }

    #[test]
    fn test_maintain_in_workflow_list() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let names: Vec<&str> = workflows
            .iter()
            .filter_map(|w| w["name"].as_str())
            .collect();
        assert!(names.contains(&"maintain"));
    }

    #[test]
    fn test_maintain_workflow_dispatches() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "maintain"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "maintain");
        assert_eq!(result["step"], 1);
    }

    #[test]
    fn test_resolve_cleanup_does_not_mention_check() {
        let (db, _tmp) = test_db();
        let step = resolve_step(5, &serde_json::json!({}), &None, 0, &db, &wf());
        let instruction = step["instruction"].as_str().unwrap();
        let when_done = step["when_done"].as_str().unwrap();
        // Cleanup should suggest scan only, not check
        assert_eq!(step["suggested_op"], "scan");
        assert!(
            !when_done.contains("check"),
            "when_done should not instruct running check: {when_done}"
        );
        assert!(
            !instruction.contains("op='check'"),
            "instruction should not tell agent to run check: {instruction}"
        );
    }

    #[test]
    fn test_resolve_cleanup_instruction_constant_no_check() {
        // The default instruction text must not reference running check
        assert!(
            !DEFAULT_RESOLVE_CLEANUP_INSTRUCTION.contains("op='check'"),
            "DEFAULT_RESOLVE_CLEANUP_INSTRUCTION should not reference op='check'"
        );
    }

    #[test]
    fn test_maintain_runs_check_only_at_step_3() {
        let (db, _tmp) = test_db();
        // Step 3 should suggest check
        let step3 = maintain_step(3, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step3["suggested_op"], "check");

        // No other maintain step should suggest check
        for s in [1, 2, 4, 5, 6, 7] {
            let step = maintain_step(s, &serde_json::json!({}), &None, 0, &db, &wf());
            let suggested = step
                .get("suggested_op")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert_ne!(
                suggested, "check",
                "maintain step {s} should not suggest check"
            );
        }
    }

    // --- New workflow design tests ---

    #[test]
    fn test_create_with_domain_step1_is_bootstrap() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "create", "domain": "mycology"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "create");
        assert_eq!(result["step"], 1);
        assert_eq!(result["total_steps"], 7);
        assert!(result["instruction"].is_string());
        assert!(result["instruction"].as_str().unwrap().contains("mycology"));
    }

    #[test]
    fn test_create_without_domain_step1_is_init() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "create", "path": "/tmp/test"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "create");
        assert_eq!(result["step"], 1);
        assert_eq!(result["total_steps"], 6);
        assert!(result["instruction"].as_str().unwrap().contains("init"));
    }

    #[test]
    fn test_create_complete_mentions_new_workflows() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "create", "step": 6, "path": "/tmp/test"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "create");
        assert_eq!(result["complete"], true);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("workflow='add'"),
            "should mention add workflow"
        );
        assert!(
            instr.contains("workflow='maintain'"),
            "should mention maintain workflow"
        );
        assert!(
            instr.contains("workflow='refresh'"),
            "should mention refresh workflow"
        );
    }

    #[test]
    fn test_add_with_topic_dispatches_to_ingest() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "add", "topic": "mushrooms"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "add");
        assert!(result["instruction"]
            .as_str()
            .unwrap()
            .contains("mushrooms"));
    }

    #[test]
    fn test_add_with_doc_id_dispatches_to_improve() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "add", "doc_id": "abc123"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "add");
        assert_eq!(result["doc_id"], "abc123");
    }

    #[test]
    fn test_add_bare_dispatches_to_enrich() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "add"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "add");
        assert!(
            result.get("entity_quality").is_some(),
            "enrich mode should include entity_quality"
        );
    }

    #[test]
    fn test_refresh_step1_scan() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "refresh"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "refresh");
        assert_eq!(result["step"], 1);
        assert_eq!(result["total_steps"], 6);
        assert_eq!(result["suggested_op"], "scan");
    }

    #[test]
    fn test_refresh_step3_research_includes_entity_quality() {
        let (db, _tmp) = test_db();
        let step = refresh_step(3, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "refresh");
        assert_eq!(step["step"], 3);
        assert!(step.get("entity_quality").is_some());
    }

    #[test]
    fn test_refresh_step6_report() {
        let (db, _tmp) = test_db();
        let step = refresh_step(6, &serde_json::json!({}), &None, 0, &db, &wf());
        assert_eq!(step["workflow"], "refresh");
        assert_eq!(step["complete"], true);
        assert!(step.get("remaining_questions").is_some());
    }

    // --- Legacy alias tests ---

    #[test]
    fn test_refresh_routing_schema_mentions_trigger_phrases() {
        // The workflow schema description should guide models to use refresh for "recent updates" queries
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf_tool["description"].as_str().unwrap();
        assert!(desc.contains("refresh"), "schema desc should mention refresh routing");
        assert!(
            desc.contains("recent") || desc.contains("what's new") || desc.contains("check for updates"),
            "schema desc should include refresh trigger phrases"
        );
        assert!(
            desc.contains("add=") || desc.contains("add vs refresh") || desc.contains("add=research"),
            "schema desc should distinguish add from refresh"
        );
    }

    #[test]
    fn test_refresh_workflow_param_description_has_trigger_phrases() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let workflow_param_desc = wf_tool["inputSchema"]["properties"]["workflow"]["description"]
            .as_str()
            .unwrap();
        assert!(
            workflow_param_desc.contains("refresh"),
            "workflow param should describe refresh"
        );
        assert!(
            workflow_param_desc.contains("recent") || workflow_param_desc.contains("what's new"),
            "workflow param should include refresh trigger phrases"
        );
        assert!(
            workflow_param_desc.contains("UPDATE") || workflow_param_desc.contains("update"),
            "workflow param should distinguish refresh (update) from add (create)"
        );
    }

    #[test]
    fn test_refresh_list_description_has_trigger_phrases() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let refresh = workflows.iter().find(|w| w["name"] == "refresh").unwrap();
        let desc = refresh["description"].as_str().unwrap();
        assert!(
            desc.contains("recent") || desc.contains("what's new") || desc.contains("check for updates"),
            "refresh list description should include trigger phrases"
        );
        assert!(
            desc.contains("UPDATE") || desc.contains("update"),
            "refresh list description should distinguish from add"
        );
    }

    // --- Legacy alias tests ---

    #[test]
    fn test_legacy_bootstrap_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "bootstrap", "domain": "mycology"});
        let result = workflow(&db, &args).unwrap();
        // Legacy alias returns with old name for backward compat
        assert_eq!(result["workflow"], "bootstrap");
        assert!(result["instruction"].is_string());
    }

    #[test]
    fn test_legacy_setup_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "setup", "path": "/tmp/test"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "setup");
    }

    #[test]
    fn test_legacy_update_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "update"});
        let result = workflow(&db, &args).unwrap();
        // update is aliased to maintain but rebranded back to "update"
        assert_eq!(result["workflow"], "update");
        assert_eq!(result["step"], 1);
    }

    #[test]
    fn test_legacy_ingest_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "ingest", "topic": "test"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "ingest");
    }

    #[test]
    fn test_legacy_enrich_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "enrich"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "enrich");
    }

    #[test]
    fn test_legacy_improve_alias() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "improve", "doc_id": "abc123"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "improve");
    }

    #[test]
    fn test_workflow_list_has_4_primary_plus_resolve() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let names: Vec<&str> = workflows
            .iter()
            .filter_map(|w| w["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "create",
                "add",
                "maintain",
                "refresh",
                "correct",
                "transition",
                "resolve"
            ]
        );
        assert!(result["aliases"].is_object());
    }

    #[test]
    fn test_rebrand_step_patches_workflow_and_when_done() {
        let val = serde_json::json!({
            "workflow": "ingest",
            "instruction": "Call workflow(workflow='ingest', step=2)",
            "when_done": "Call workflow(workflow='ingest', step=3)"
        });
        let rebranded = rebrand_step(val, "ingest", "add");
        assert_eq!(rebranded["workflow"], "add");
        assert!(rebranded["instruction"]
            .as_str()
            .unwrap()
            .contains("workflow='add'"));
        assert!(rebranded["when_done"]
            .as_str()
            .unwrap()
            .contains("workflow='add'"));
    }

    // --- Correct workflow: parse dimension tests ---

    #[test]
    fn test_correct_parse_extracts_temporal_context() {
        let wf = WorkflowsConfig::default();
        let args =
            serde_json::json!({"correction": "The migration completed in January, not March"});
        let result = correct_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("Temporal context"),
            "parse should ask for temporal context"
        );
        assert!(
            instr.contains("dates, date ranges, or temporal markers"),
            "parse should explain what temporal context means"
        );
        assert!(
            instr.contains("\"none\""),
            "parse should handle no-temporal case"
        );
    }

    #[test]
    fn test_correct_parse_identifies_correction_types() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        for ty in &[
            "relational",
            "temporal",
            "factual",
            "status",
            "identity",
            "classification",
        ] {
            assert!(
                instr.contains(ty),
                "parse should list correction type: {ty}"
            );
        }
        assert!(
            instr.contains("Correction type:"),
            "parse output should include Correction type field"
        );
    }

    #[test]
    fn test_correct_parse_has_old_new_value_and_scope() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("Old value:"),
            "parse output should include Old value"
        );
        assert!(
            instr.contains("New value:"),
            "parse output should include New value"
        );
        assert!(
            instr.contains("Scope:"),
            "parse output should include Scope"
        );
        assert!(instr.contains("single"), "scope should mention single");
        assert!(instr.contains("systemic"), "scope should mention systemic");
    }

    #[test]
    fn test_correct_parse_still_has_entities_and_search_terms() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("Entities:"),
            "parse should still extract entities"
        );
        assert!(
            instr.contains("False claim:"),
            "parse should still extract false claim"
        );
        assert!(
            instr.contains("True fact:"),
            "parse should still extract true fact"
        );
        assert!(
            instr.contains("Search terms:"),
            "parse should still extract search terms"
        );
    }

    #[test]
    fn test_correct_fix_uses_temporal_context_over_today() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("correction date"),
            "fix should prefer correction date for @t tag"
        );
        assert!(
            instr.contains("temporal context"),
            "fix should reference temporal context from step 1"
        );
        assert!(
            instr.contains("fall back to today"),
            "fix should fall back to today when no temporal context"
        );
    }

    #[test]
    fn test_correct_fix_fallback_contains_today_date() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert!(
            instr.contains(&today),
            "fix fallback should contain today's date"
        );
    }

    #[test]
    fn test_correct_parse_domain_agnostic() {
        // Mushroom test: instruction should not reference any specific domain
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap().to_lowercase();
        for term in &[
            "employee",
            "career",
            "company",
            "person",
            "people",
            "hire",
            "promotion",
        ] {
            assert!(
                !instr.contains(term),
                "parse instruction should not contain domain term: {term}"
            );
        }
    }

    #[test]
    fn test_correct_fix_rephrases_in_document_context() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("IN THE CONTEXT OF THIS DOCUMENT"),
            "fix should instruct context-aware rephrasing"
        );
        assert!(
            instr.contains("AS IF THE FALSE CLAIM NEVER EXISTED"),
            "fix should instruct writing as if false claim never existed"
        );
        assert!(
            instr.contains("read naturally"),
            "fix should require natural reading"
        );
    }

    #[test]
    fn test_correct_fix_distinguishes_entity_vs_overview_docs() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("only mention what is relevant to THAT entity"),
            "fix should scope entity docs to relevant facts"
        );
        assert!(
            instr.contains("overview/hub"),
            "fix should identify overview docs as the place for full explanation"
        );
        assert!(
            instr.contains("cross-references"),
            "fix should handle cross-reference docs"
        );
    }

    #[test]
    fn test_correct_fix_forbids_disclaimers() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        // Must explicitly forbid disclaimer patterns
        assert!(
            instr.contains("FORBIDDEN patterns"),
            "fix should list forbidden patterns"
        );
        assert!(
            instr.contains("Do NOT add notes, disclaimers, parentheticals"),
            "fix should forbid disclaimers"
        );
        assert!(
            instr.contains("deny the old claim"),
            "fix should forbid sentences that deny the old claim"
        );
    }

    #[test]
    fn test_correct_fix_shows_before_after_examples() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        // Must show correct approach with before/after examples
        assert!(
            instr.contains("CORRECT approach"),
            "fix should show correct approach"
        );
        assert!(instr.contains("OLD:"), "fix should show OLD example");
        assert!(instr.contains("WRITE:"), "fix should show WRITE example");
    }

    #[test]
    fn test_correct_fix_still_requires_source_footnote() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test", "source": "CEO memo 2026-03-01"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("source footnote"),
            "fix should still require source footnote on every fixed doc"
        );
        assert!(
            instr.contains("CEO memo 2026-03-01"),
            "fix should include the provided source"
        );
    }

    #[test]
    fn test_correct_fix_domain_agnostic() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"correction": "test"});
        let result = correct_step(3, &args, &wf);
        let instr = result["instruction"].as_str().unwrap().to_lowercase();
        for term in &[
            "employee",
            "career",
            "company",
            "person",
            "people",
            "hire",
            "promotion",
        ] {
            assert!(
                !instr.contains(term),
                "fix instruction should not contain domain term: {term}"
            );
        }
    }

    // --- Transition workflow tests ---

    #[test]
    fn test_transition_step1_parses_change() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "Acme Corp renamed to NewCo"});
        let result = transition_step(1, &args, &wf);
        assert_eq!(result["workflow"], "transition");
        assert_eq!(result["step"], 1);
        assert_eq!(result["total_steps"], 7);
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("Acme Corp renamed to NewCo"));
        assert!(instr.contains("Change type:"));
        assert!(instr.contains("rename"));
        assert!(instr.contains("merger"));
        assert!(instr.contains("role_change"));
        assert!(instr.contains("Old value:"));
        assert!(instr.contains("New value:"));
        assert!(instr.contains("Effective date:"));
        assert!(instr.contains("Search terms:"));
    }

    #[test]
    fn test_transition_step1_includes_effective_date_default() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(1, &args, &wf);
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains(&today),
            "parse should include today as fallback date"
        );
    }

    #[test]
    fn test_transition_step1_preserves_source() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test", "source": "CEO announcement 2026-03-11"});
        let result = transition_step(1, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("CEO announcement 2026-03-11"));
    }

    #[test]
    fn test_transition_step2_without_nomenclature_asks_question() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(2, &args, &wf);
        assert_eq!(result["step"], 2);
        assert_eq!(result["awaiting_input"], true);
        assert_eq!(result["input_param"], "nomenclature");
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("Replace with context"));
        assert!(instr.contains("Replace clean"));
        assert!(instr.contains("Keep old in history"));
        assert!(instr.contains("Custom"));
    }

    #[test]
    fn test_transition_step2_with_nomenclature_proceeds_to_search() {
        let wf = WorkflowsConfig::default();
        let args =
            serde_json::json!({"change": "test", "nomenclature": "NewCo (formerly Acme Corp)"});
        let result = transition_step(2, &args, &wf);
        // Should jump to step 3 (search)
        assert_eq!(result["step"], 3);
        assert_eq!(result["nomenclature"], "NewCo (formerly Acme Corp)");
        assert!(result.get("awaiting_input").is_none());
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("NewCo (formerly Acme Corp)"));
        assert!(instr.contains("search"));
    }

    #[test]
    fn test_transition_step4_apply_includes_temporal_boundaries() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({
            "change": "test rename",
            "effective_date": "2026-03-11",
            "nomenclature": "New Name (formerly Old Name)"
        });
        let result = transition_step(4, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("@t[..2026-03-11]"),
            "apply should add end-date on old value"
        );
        assert!(
            instr.contains("@t[2026-03-11..]"),
            "apply should add start-date on new value"
        );
    }

    #[test]
    fn test_transition_step4_apply_with_source_footnote() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({
            "change": "test",
            "source": "Board memo 2026-03-11",
            "nomenclature": "new"
        });
        let result = transition_step(4, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("Board memo 2026-03-11"),
            "apply should include source footnote"
        );
    }

    #[test]
    fn test_transition_step4_distinguishes_doc_types() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test", "nomenclature": "new"});
        let result = transition_step(4, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("Entity overview doc"),
            "apply should handle entity overview docs"
        );
        assert!(
            instr.contains("Current reference docs"),
            "apply should handle current reference docs"
        );
        assert!(
            instr.contains("Historical reference docs"),
            "apply should handle historical docs"
        );
    }

    #[test]
    fn test_transition_step4_preserves_historical_references() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test", "nomenclature": "new"});
        let result = transition_step(4, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("historical footnotes"),
            "apply should preserve historical references"
        );
        assert!(
            instr.contains("correct at the time"),
            "apply should acknowledge old info was valid"
        );
    }

    #[test]
    fn test_transition_step5_executes_org_suggestions() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(5, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("execute_suggestions"),
            "step 5 should execute org suggestions"
        );
    }

    #[test]
    fn test_transition_step6_runs_maintain() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(6, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("scan"), "step 6 should scan");
        assert!(instr.contains("check"), "step 6 should check");
    }

    #[test]
    fn test_transition_step7_report_complete() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({
            "change": "Acme Corp renamed to NewCo",
            "effective_date": "2026-03-11",
            "source": "CEO memo",
            "nomenclature": "NewCo (formerly Acme Corp)"
        });
        let result = transition_step(7, &args, &wf);
        assert_eq!(result["complete"], true);
        let instr = result["instruction"].as_str().unwrap();
        assert!(instr.contains("Acme Corp renamed to NewCo"));
        assert!(instr.contains("2026-03-11"));
        assert!(instr.contains("CEO memo"));
        assert!(instr.contains("NewCo (formerly Acme Corp)"));
    }

    #[test]
    fn test_transition_effective_date_defaults_to_today() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(1, &args, &wf);
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(result["effective_date"].as_str().unwrap(), today);
    }

    #[test]
    fn test_transition_effective_date_uses_provided() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test", "effective_date": "2025-06-15"});
        let result = transition_step(1, &args, &wf);
        assert_eq!(result["effective_date"].as_str().unwrap(), "2025-06-15");
    }

    #[test]
    fn test_transition_domain_agnostic() {
        // Mushroom test: instructions should not reference any specific domain
        let wf = WorkflowsConfig::default();
        for step in 1..=7 {
            let args = serde_json::json!({"change": "test", "nomenclature": "new"});
            let result = transition_step(step, &args, &wf);
            let instr = result["instruction"].as_str().unwrap().to_lowercase();
            for term in &[
                "employee",
                "career",
                "company",
                "person",
                "people",
                "hire",
                "promotion",
            ] {
                assert!(
                    !instr.contains(term),
                    "step {step} instruction should not contain domain term: {term}"
                );
            }
        }
    }

    #[test]
    fn test_transition_dispatches_from_workflow() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "transition", "change": "test rename"});
        let result = workflow(&db, &args).unwrap();
        assert_eq!(result["workflow"], "transition");
        assert_eq!(result["step"], 1);
    }

    #[test]
    fn test_transition_in_workflow_list() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"workflow": "list"});
        let result = workflow(&db, &args).unwrap();
        let workflows = result["workflows"].as_array().unwrap();
        let transition = workflows
            .iter()
            .find(|w| w["name"] == "transition")
            .unwrap();
        let desc = transition["description"].as_str().unwrap();
        assert!(desc.contains("temporal entity changes"));
        assert!(desc.contains("rename"));
        assert!(desc.contains("merger"));
    }

    #[test]
    fn test_transition_nomenclature_options_are_generic() {
        let wf = WorkflowsConfig::default();
        let args = serde_json::json!({"change": "test"});
        let result = transition_step(2, &args, &wf);
        let instr = result["instruction"].as_str().unwrap();
        // Options should use generic placeholders, not domain-specific examples
        assert!(
            instr.contains("<new value>")
                || instr.contains("<old value>")
                || instr.contains("new value"),
            "nomenclature options should use generic placeholders"
        );
    }

    #[test]
    fn test_workflow_tool_description_includes_override_language() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let desc = wf_tool["description"].as_str().unwrap();
        assert!(
            desc.contains("explicitly names a workflow"),
            "workflow description should tell agent to respect explicit user choice"
        );
        assert!(
            desc.contains("ALWAYS use that workflow"),
            "workflow description should use ALWAYS language"
        );
        assert!(
            desc.contains("do NOT override"),
            "workflow description should forbid overriding user choice"
        );
    }

    #[test]
    fn test_correct_workflow_description_includes_was_never_true() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let workflow_param_desc = wf_tool["inputSchema"]["properties"]["workflow"]["description"]
            .as_str()
            .unwrap();
        assert!(
            workflow_param_desc.contains("ALWAYS WRONG")
                || workflow_param_desc.contains("never true"),
            "correct description should clarify it applies to facts that were never true"
        );
    }

    #[test]
    fn test_transition_workflow_description_includes_was_true_until() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let wf_tool = tools_arr.iter().find(|t| t["name"] == "workflow").unwrap();
        let workflow_param_desc = wf_tool["inputSchema"]["properties"]["workflow"]["description"]
            .as_str()
            .unwrap();
        assert!(
            workflow_param_desc.contains("WAS TRUE")
                || workflow_param_desc.contains("was true until"),
            "transition description should clarify it applies to facts that were true until a date"
        );
    }

    // --- citation_patterns in workflow instructions ---

    #[test]
    fn test_bootstrap_prompt_suggests_citation_patterns_for_jazz() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("jazz music", None, &prompts, None);
        assert!(
            prompt.contains("citation_patterns"),
            "bootstrap prompt should mention citation_patterns"
        );
        assert!(
            prompt.contains("catalog_number"),
            "bootstrap prompt should give catalog_number as a jazz example"
        );
        assert!(
            prompt.contains("CL 1355") || prompt.contains("SD 1361"),
            "bootstrap prompt should show jazz catalog number examples"
        );
    }

    #[test]
    fn test_bootstrap_prompt_suggests_citation_patterns_for_bible() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("biblical studies", None, &prompts, None);
        assert!(
            prompt.contains("citation_patterns"),
            "bootstrap prompt should mention citation_patterns"
        );
        assert!(
            prompt.contains("verse_reference"),
            "bootstrap prompt should give verse_reference as a bible example"
        );
        assert!(
            prompt.contains("Genesis 1:1") || prompt.contains("John 3:16"),
            "bootstrap prompt should show verse reference examples"
        );
    }

    #[test]
    fn test_bootstrap_prompt_omit_guidance_for_web_only_domains() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("mushroom foraging", None, &prompts, None);
        assert!(
            prompt.contains("omit citation_patterns") || prompt.contains("omit"),
            "bootstrap prompt should tell agent to omit citation_patterns when not needed"
        );
    }

    #[test]
    fn test_bootstrap_prompt_includes_good_vs_bad_pattern_guidance() {
        let prompts = crate::config::PromptsConfig::default();
        let prompt = build_bootstrap_prompt("legal research", None, &prompts, None);
        assert!(
            prompt.contains("too broad") || prompt.contains("Bad citation_patterns"),
            "bootstrap prompt should warn about overly broad patterns"
        );
    }

    #[test]
    fn test_setup_perspective_instruction_mentions_citation_patterns() {
        assert!(
            DEFAULT_SETUP_PERSPECTIVE_INSTRUCTION.contains("citation_patterns"),
            "perspective instruction should mention citation_patterns"
        );
    }

    #[test]
    fn test_maintain_report_suggests_citation_pattern_for_false_positives() {
        assert!(
            DEFAULT_MAINTAIN_REPORT_INSTRUCTION.contains("citation_pattern"),
            "maintain report should suggest adding citation_pattern for false positives"
        );
        assert!(
            DEFAULT_MAINTAIN_REPORT_INSTRUCTION.contains("weak-source"),
            "maintain report should mention weak-source questions as the trigger"
        );
        assert!(
            DEFAULT_MAINTAIN_REPORT_INSTRUCTION.contains("perspective.yaml"),
            "maintain report should tell agent to add pattern to perspective.yaml"
        );
    }

    #[test]
    fn test_resolve_answer_intro_suggests_citation_pattern_for_valid_formats() {
        assert!(
            DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION.contains("citation_pattern"),
            "resolve intro should suggest adding citation_pattern for valid domain formats"
        );
        assert!(
            DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION.contains("Suggest citation_pattern"),
            "resolve intro should show the exact suggestion format"
        );
        assert!(
            DEFAULT_RESOLVE_ANSWER_INTRO_INSTRUCTION.contains("name=X, pattern=Y"),
            "resolve intro should show the pattern format"
        );
    }
}
