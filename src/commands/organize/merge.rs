//! Merge command implementation.
//!
//! Merges two similar documents into one with fact-level accounting.

use super::MergeArgs;
use crate::commands::{
    confirm_prompt, find_repo_with_config, print_output, setup_llm_with_timeout, OutputFormat,
};
use factbase::{execute_merge, plan_merge, verify_merge, MergePlan, MergeResult};
use serde::Serialize;

/// Output for merge command (dry-run or execution).
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum MergeOutput {
    Plan(MergePlanOutput),
    Result(MergeResultOutput),
}

/// Dry-run output showing the merge plan.
#[derive(Debug, Serialize)]
pub struct MergePlanOutput {
    pub keep_id: String,
    pub keep_title: String,
    pub merge_id: String,
    pub merge_title: String,
    pub fact_count: usize,
    pub duplicate_count: usize,
    pub orphan_count: usize,
    pub dry_run: bool,
}

/// Execution result output.
#[derive(Debug, Clone, Serialize)]
pub struct MergeResultOutput {
    pub kept_id: String,
    pub merged_ids: Vec<String>,
    pub fact_count: usize,
    pub duplicate_count: usize,
    pub orphan_count: usize,
    pub orphan_path: Option<String>,
    pub links_redirected: usize,
}

impl From<MergeResult> for MergeResultOutput {
    fn from(r: MergeResult) -> Self {
        Self {
            kept_id: r.kept_id,
            merged_ids: r.merged_ids,
            fact_count: r.fact_count,
            duplicate_count: r.duplicate_count,
            orphan_count: r.orphan_count,
            orphan_path: r.orphan_path.map(|p| p.display().to_string()),
            links_redirected: r.links_redirected,
        }
    }
}

/// Run the merge command.
pub async fn run(args: MergeArgs) -> anyhow::Result<()> {
    let (config, db, repo) = find_repo_with_config(None)?;
    let format = OutputFormat::resolve(args.json, args.format);

    // Validate document IDs exist
    let doc1 = db.require_document(&args.doc1)?;
    let doc2 = db.require_document(&args.doc2)?;

    // Determine which document to keep
    let keep_id = args.into.as_deref().unwrap_or_else(|| {
        // Auto-select: prefer document with more content or links
        let links1 = db.get_links_from(&args.doc1).unwrap_or_default().len();
        let links2 = db.get_links_from(&args.doc2).unwrap_or_default().len();
        let score1 = doc1.content.len() + links1 * 100;
        let score2 = doc2.content.len() + links2 * 100;
        if score1 >= score2 {
            &args.doc1
        } else {
            &args.doc2
        }
    });

    // Validate --into is one of the two documents
    if let Some(ref into) = args.into {
        if into != &args.doc1 && into != &args.doc2 {
            anyhow::bail!(
                "--into must be one of the documents being merged ({} or {})",
                args.doc1,
                args.doc2
            );
        }
    }

    let merge_id = if keep_id == args.doc1 {
        &args.doc2
    } else {
        &args.doc1
    };

    // Get titles for display
    let keep_title = if keep_id == args.doc1 {
        &doc1.title
    } else {
        &doc2.title
    };
    let merge_title = if *merge_id == args.doc1 {
        &doc1.title
    } else {
        &doc2.title
    };

    // Create merge plan using LLM
    let llm = setup_llm_with_timeout(&config, args.timeout).await;
    let plan = plan_merge(keep_id, &[merge_id], &db, &llm).await?;

    if args.dry_run {
        let output = MergePlanOutput {
            keep_id: keep_id.to_string(),
            keep_title: keep_title.clone(),
            merge_id: merge_id.to_string(),
            merge_title: merge_title.clone(),
            fact_count: plan.ledger.source_facts.len(),
            duplicate_count: plan.duplicate_count(),
            orphan_count: plan.orphan_count(),
            dry_run: true,
        };
        print_output(format, &MergeOutput::Plan(output), || {
            print_plan(&plan, keep_title, merge_title)
        })?;
        return Ok(());
    }

    // Show plan and prompt for confirmation
    if !args.yes {
        print_plan(&plan, keep_title, merge_title);
        if !confirm_prompt("Proceed with merge?")? {
            println!("Merge cancelled.");
            return Ok(());
        }
    }

    // Execute merge with snapshot and verification
    let doc_ids: Vec<&str> = vec![keep_id, merge_id];
    let result = super::execute_with_snapshot(
        &doc_ids,
        &db,
        &repo.path,
        "Merge",
        || execute_merge(&plan, &db, &repo.path),
        |r| verify_merge(&plan, r, &db, &repo.path),
    )?;

    let output = MergeResultOutput::from(result);

    print_output(format, &MergeOutput::Result(output.clone()), || {
        print_result(&output)
    })?;

    Ok(())
}

/// Print merge plan in table format.
fn print_plan(plan: &MergePlan, keep_title: &str, merge_title: &str) {
    println!("Merge Plan");
    println!("{}", "=".repeat(40));
    println!("Keep:  {} [{}]", keep_title, plan.keep_id);
    println!(
        "Merge: {} [{}]",
        merge_title,
        plan.merge_ids.first().unwrap_or(&String::new())
    );
    println!();
    println!("Fact Summary:");
    println!("  Total facts:    {}", plan.ledger.source_facts.len());
    println!(
        "  To keep:        {}",
        plan.ledger.source_facts.len() - plan.duplicate_count() - plan.orphan_count()
    );
    println!("  Duplicates:     {}", plan.duplicate_count());
    println!("  Orphans:        {}", plan.orphan_count());

    if plan.orphan_count() > 0 {
        println!(
            "\n⚠ {} fact(s) will be sent to _orphans.md",
            plan.orphan_count()
        );
    }
}

/// Print merge result in table format.
fn print_result(result: &MergeResultOutput) {
    println!("Merge Complete");
    println!("{}", "=".repeat(40));
    println!("Kept document:    {}", result.kept_id);
    println!("Merged documents: {}", result.merged_ids.join(", "));
    println!();
    println!("Results:");
    println!("  Facts kept:       {}", result.fact_count);
    println!("  Duplicates:       {}", result.duplicate_count);
    println!("  Orphans:          {}", result.orphan_count);
    println!("  Links redirected: {}", result.links_redirected);

    if let Some(ref path) = result.orphan_path {
        println!("\nOrphans written to: {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_plan_output_struct() {
        let output = MergePlanOutput {
            keep_id: "abc123".to_string(),
            keep_title: "Doc A".to_string(),
            merge_id: "def456".to_string(),
            merge_title: "Doc B".to_string(),
            fact_count: 10,
            duplicate_count: 2,
            orphan_count: 1,
            dry_run: true,
        };
        assert_eq!(output.keep_id, "abc123");
        assert!(output.dry_run);
    }

    #[test]
    fn test_merge_result_output_from() {
        use std::path::PathBuf;

        let result = MergeResult {
            kept_id: "abc123".to_string(),
            merged_ids: vec!["def456".to_string()],
            fact_count: 5,
            duplicate_count: 2,
            orphan_count: 1,
            orphan_path: Some(PathBuf::from("_orphans.md")),
            links_redirected: 3,
        };

        let output = MergeResultOutput::from(result);
        assert_eq!(output.kept_id, "abc123");
        assert_eq!(output.orphan_path, Some("_orphans.md".to_string()));
    }

    #[test]
    fn test_merge_result_output_no_orphan_path() {
        let result = MergeResult {
            kept_id: "abc123".to_string(),
            merged_ids: vec!["def456".to_string()],
            fact_count: 5,
            duplicate_count: 0,
            orphan_count: 0,
            orphan_path: None,
            links_redirected: 0,
        };

        let output = MergeResultOutput::from(result);
        assert!(output.orphan_path.is_none());
    }
}
