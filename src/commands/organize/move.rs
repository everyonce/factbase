//! Move command implementation.
//!
//! Moves a document to a different folder, updating its type based on
//! the destination folder.

use std::path::Path;

use anyhow::Context;

use super::MoveArgs;
use crate::commands::{confirm_prompt, find_repo_with_config, print_output, OutputFormat};
use factbase::{execute_move, MoveResult};
use serde::Serialize;

/// Output for move command (dry-run or execution).
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum MoveOutput {
    Plan(MovePlanOutput),
    Result(MoveResultOutput),
}

/// Dry-run output showing the move plan.
#[derive(Debug, Clone, Serialize)]
pub struct MovePlanOutput {
    pub doc_id: String,
    pub doc_title: String,
    pub old_path: String,
    pub new_path: String,
    pub old_type: Option<String>,
    pub new_type: String,
    pub dry_run: bool,
}

/// Execution result output.
#[derive(Debug, Clone, Serialize)]
pub struct MoveResultOutput {
    pub doc_id: String,
    pub old_path: String,
    pub new_path: String,
    pub old_type: Option<String>,
    pub new_type: String,
}

impl From<MoveResult> for MoveResultOutput {
    fn from(r: MoveResult) -> Self {
        Self {
            doc_id: r.doc_id,
            old_path: r.old_path,
            new_path: r.new_path,
            old_type: r.old_type,
            new_type: r.new_type,
        }
    }
}

/// Run the move command.
pub fn run(args: MoveArgs) -> anyhow::Result<()> {
    let (_config, db, repo) = find_repo_with_config(None)?;
    let format = OutputFormat::resolve(args.json, args.format);

    // Validate document ID exists
    let doc = db.require_document(&args.doc_id)?;

    // Build new path: destination folder + original filename
    let old_path = Path::new(&doc.file_path);
    let filename = old_path
        .file_name()
        .with_context(|| format!("Invalid file path: {}", doc.file_path))?;

    // Normalize destination: ensure it ends with the filename
    let dest = args.to.trim_end_matches('/');
    let new_path = if dest.ends_with(".md") {
        // User provided full path including filename
        dest.to_string()
    } else {
        // User provided folder, append filename
        format!("{}/{}", dest, filename.to_string_lossy())
    };

    // Derive new type from destination folder
    let processor = factbase::processor::DocumentProcessor::new();
    let new_type = processor.derive_type(Path::new(&new_path), Path::new(""));

    if args.dry_run {
        let output = MovePlanOutput {
            doc_id: args.doc_id.clone(),
            doc_title: doc.title.clone(),
            old_path: doc.file_path.clone(),
            new_path: new_path.clone(),
            old_type: doc.doc_type.clone(),
            new_type,
            dry_run: true,
        };
        print_output(format, &MoveOutput::Plan(output.clone()), || {
            print_plan(&output)
        })?;
        return Ok(());
    }

    // Show plan and prompt for confirmation
    if !args.yes {
        let plan = MovePlanOutput {
            doc_id: args.doc_id.clone(),
            doc_title: doc.title.clone(),
            old_path: doc.file_path.clone(),
            new_path: new_path.clone(),
            old_type: doc.doc_type.clone(),
            new_type: new_type.clone(),
            dry_run: false,
        };
        print_plan(&plan);
        if !confirm_prompt("Proceed with move?")? {
            println!("Move cancelled.");
            return Ok(());
        }
    }

    // Execute move
    let result = execute_move(&args.doc_id, Path::new(&new_path), &db, &repo.path)?;
    let output = MoveResultOutput::from(result);

    print_output(format, &MoveOutput::Result(output.clone()), || {
        print_result(&output)
    })?;

    Ok(())
}

/// Print move plan in table format.
fn print_plan(plan: &MovePlanOutput) {
    println!("Move Plan");
    println!("{}", "=".repeat(40));
    println!("Document: {} [{}]", plan.doc_title, plan.doc_id);
    println!();
    println!("Current:");
    println!("  Path: {}", plan.old_path);
    println!("  Type: {}", plan.old_type.as_deref().unwrap_or("(none)"));
    println!();
    println!("After move:");
    println!("  Path: {}", plan.new_path);
    println!("  Type: {}", plan.new_type);
}

/// Print move result in table format.
fn print_result(result: &MoveResultOutput) {
    println!("Move Complete");
    println!("{}", "=".repeat(40));
    println!("Document: {}", result.doc_id);
    println!();
    println!("Moved:");
    println!("  From: {}", result.old_path);
    println!("  To:   {}", result.new_path);
    println!();
    println!(
        "Type changed: {} → {}",
        result.old_type.as_deref().unwrap_or("(none)"),
        result.new_type
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_plan_output_struct() {
        let output = MovePlanOutput {
            doc_id: "abc123".to_string(),
            doc_title: "Test Doc".to_string(),
            old_path: "people/test.md".to_string(),
            new_path: "projects/test.md".to_string(),
            old_type: Some("person".to_string()),
            new_type: "project".to_string(),
            dry_run: true,
        };
        assert_eq!(output.doc_id, "abc123");
        assert!(output.dry_run);
    }

    #[test]
    fn test_move_result_output_from() {
        let result = MoveResult {
            doc_id: "abc123".to_string(),
            old_path: "people/test.md".to_string(),
            new_path: "projects/test.md".to_string(),
            old_type: Some("person".to_string()),
            new_type: "project".to_string(),
        };

        let output = MoveResultOutput::from(result);
        assert_eq!(output.doc_id, "abc123");
        assert_eq!(output.new_type, "project");
    }

    #[test]
    fn test_move_result_output_no_old_type() {
        let result = MoveResult {
            doc_id: "abc123".to_string(),
            old_path: "test.md".to_string(),
            new_path: "people/test.md".to_string(),
            old_type: None,
            new_type: "person".to_string(),
        };

        let output = MoveResultOutput::from(result);
        assert!(output.old_type.is_none());
    }
}
