//! Split command implementation.
//!
//! Splits a multi-topic document into separate documents with fact-level accounting.

use super::SplitArgs;
use crate::commands::{
    confirm_prompt, find_repo_with_config, print_output, setup_llm_with_timeout, OutputFormat,
};
use factbase::{
    execute_split, extract_sections, plan_split, verify_split, SplitPlan, SplitResult, SplitSection,
};
use serde::Serialize;

/// Output for split command (dry-run or execution).
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum SplitOutput {
    Plan(SplitPlanOutput),
    Result(SplitResultOutput),
}

/// Dry-run output showing the split plan.
#[derive(Debug, Serialize)]
pub struct SplitPlanOutput {
    pub source_id: String,
    pub source_title: String,
    pub sections: Vec<SectionInfo>,
    pub fact_count: usize,
    pub orphan_count: usize,
    pub dry_run: bool,
}

/// Section info for output.
#[derive(Debug, Serialize)]
pub struct SectionInfo {
    pub title: String,
    pub proposed_title: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Execution result output.
#[derive(Debug, Clone, Serialize)]
pub struct SplitResultOutput {
    pub source_id: String,
    pub new_doc_ids: Vec<String>,
    pub fact_count: usize,
    pub orphan_count: usize,
    pub orphan_path: Option<String>,
}

impl From<SplitResult> for SplitResultOutput {
    fn from(r: SplitResult) -> Self {
        Self {
            source_id: r.source_id,
            new_doc_ids: r.new_doc_ids,
            fact_count: r.fact_count,
            orphan_count: r.orphan_count,
            orphan_path: r.orphan_path.map(|p| p.display().to_string()),
        }
    }
}

/// Run the split command.
pub async fn run(args: SplitArgs) -> anyhow::Result<()> {
    let (config, db, repo) = find_repo_with_config(None)?;
    let format = OutputFormat::resolve(args.json, args.format);

    // Validate document ID exists
    let doc = db.require_document(&args.doc_id)?;

    // Extract sections from document
    let mut sections = extract_sections(&doc.content);

    // Filter to sections with meaningful content
    sections.retain(|s| s.content.len() >= 50);

    // If --at specified, filter to that section and its siblings
    if let Some(ref at_title) = args.at {
        let matching: Vec<SplitSection> = sections
            .iter()
            .filter(|s| s.title.to_lowercase().contains(&at_title.to_lowercase()))
            .cloned()
            .collect();

        if matching.is_empty() {
            anyhow::bail!(
                "No section matching '{}' found. Available sections: {}",
                at_title,
                sections
                    .iter()
                    .map(|s| s.title.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        sections = matching;
    }

    // Need at least 2 sections to split
    if sections.len() < 2 {
        anyhow::bail!(
            "Document has {} section(s) with content. Need at least 2 sections to split.",
            sections.len()
        );
    }

    // Create split plan using LLM
    let llm = setup_llm_with_timeout(&config, args.timeout).await;
    let plan = plan_split(&args.doc_id, &sections, &db, &llm).await?;

    if args.dry_run {
        let output = build_plan_output(&doc.title, &plan, &sections);
        print_output(format, &SplitOutput::Plan(output), || {
            print_plan(&plan, &doc.title, &sections)
        })?;
        return Ok(());
    }

    // Show plan and prompt for confirmation
    if !args.yes {
        print_plan(&plan, &doc.title, &sections);
        if !confirm_prompt("Proceed with split?")? {
            println!("Split cancelled.");
            return Ok(());
        }
    }

    // Execute split with snapshot and verification
    let doc_ids: Vec<&str> = vec![&args.doc_id];
    let result = super::execute_with_snapshot(
        &doc_ids,
        &db,
        &repo.path,
        "Split",
        || execute_split(&plan, &db, &repo.path),
        |r| verify_split(&plan, r, &db, &repo.path),
    )?;

    let output = SplitResultOutput::from(result);

    print_output(format, &SplitOutput::Result(output.clone()), || {
        print_result(&output)
    })?;

    Ok(())
}

/// Build plan output struct.
fn build_plan_output(
    source_title: &str,
    plan: &SplitPlan,
    sections: &[SplitSection],
) -> SplitPlanOutput {
    let section_infos: Vec<SectionInfo> = plan
        .new_documents
        .iter()
        .zip(sections.iter())
        .map(|(doc, section)| SectionInfo {
            title: section.title.clone(),
            proposed_title: doc.title.clone(),
            start_line: section.start_line,
            end_line: section.end_line,
        })
        .collect();

    SplitPlanOutput {
        source_id: plan.source_id.clone(),
        source_title: source_title.to_string(),
        sections: section_infos,
        fact_count: plan.ledger.source_facts.len(),
        orphan_count: plan.orphan_count(),
        dry_run: true,
    }
}

/// Print split plan in table format.
fn print_plan(plan: &SplitPlan, source_title: &str, sections: &[SplitSection]) {
    println!("Split Plan");
    println!("{}", "=".repeat(50));
    println!("Source: {} [{}]", source_title, plan.source_id);
    println!();
    println!("Proposed Documents ({}):", plan.document_count());

    for (i, doc) in plan.new_documents.iter().enumerate() {
        let section = sections.get(i);
        let lines = section
            .map(|s| format!("lines {}-{}", s.start_line, s.end_line))
            .unwrap_or_default();
        println!(
            "  {}. {} (from: {}, {})",
            i + 1,
            doc.title,
            doc.section_title,
            lines
        );
    }

    println!();
    println!("Fact Summary:");
    println!("  Total facts:  {}", plan.ledger.source_facts.len());
    println!(
        "  Distributed:  {}",
        plan.ledger.source_facts.len() - plan.orphan_count()
    );
    println!("  Orphans:      {}", plan.orphan_count());

    if plan.orphan_count() > 0 {
        println!(
            "\n⚠ {} fact(s) will be sent to _orphans.md",
            plan.orphan_count()
        );
    }
}

/// Print split result in table format.
fn print_result(result: &SplitResultOutput) {
    println!("Split Complete");
    println!("{}", "=".repeat(50));
    println!("Source document: {} (deleted)", result.source_id);
    println!();
    println!("New Documents ({}):", result.new_doc_ids.len());
    for (i, id) in result.new_doc_ids.iter().enumerate() {
        println!("  {}. {}", i + 1, id);
    }
    println!();
    println!("Results:");
    println!("  Facts distributed: {}", result.fact_count);
    println!("  Orphans:           {}", result.orphan_count);

    if let Some(ref path) = result.orphan_path {
        println!("\nOrphans written to: {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_plan_output_struct() {
        let output = SplitPlanOutput {
            source_id: "abc123".to_string(),
            source_title: "Multi-Topic Doc".to_string(),
            sections: vec![
                SectionInfo {
                    title: "Career".to_string(),
                    proposed_title: "Career History".to_string(),
                    start_line: 3,
                    end_line: 10,
                },
                SectionInfo {
                    title: "Education".to_string(),
                    proposed_title: "Academic Background".to_string(),
                    start_line: 12,
                    end_line: 20,
                },
            ],
            fact_count: 8,
            orphan_count: 1,
            dry_run: true,
        };
        assert_eq!(output.source_id, "abc123");
        assert_eq!(output.sections.len(), 2);
        assert!(output.dry_run);
    }

    #[test]
    fn test_split_result_output_from() {
        use std::path::PathBuf;

        let result = SplitResult {
            source_id: "abc123".to_string(),
            new_doc_ids: vec!["def456".to_string(), "ghi789".to_string()],
            fact_count: 5,
            orphan_count: 1,
            orphan_path: Some(PathBuf::from("_orphans.md")),
        };

        let output = SplitResultOutput::from(result);
        assert_eq!(output.source_id, "abc123");
        assert_eq!(output.new_doc_ids.len(), 2);
        assert_eq!(output.orphan_path, Some("_orphans.md".to_string()));
    }

    #[test]
    fn test_split_result_output_no_orphan_path() {
        let result = SplitResult {
            source_id: "abc123".to_string(),
            new_doc_ids: vec!["def456".to_string()],
            fact_count: 3,
            orphan_count: 0,
            orphan_path: None,
        };

        let output = SplitResultOutput::from(result);
        assert!(output.orphan_path.is_none());
    }
}
