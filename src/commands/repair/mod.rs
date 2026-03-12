//! Repair command for auto-fixing document corruption.

use clap::Args;

#[derive(Args)]
pub struct RepairArgs {
    /// Repair a single document by ID
    #[arg(long)]
    pub doc: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Suppress non-essential output
    #[arg(short, long)]
    pub quiet: bool,
}

use crate::commands::setup::Setup;
use factbase::processor::content_hash;
use factbase::processor::repair::repair_document;
use std::path::Path;

fn db_repos(
    db: &factbase::database::Database,
) -> anyhow::Result<Vec<factbase::models::Repository>> {
    let repos = db.list_repositories()?;
    if repos.is_empty() {
        anyhow::bail!("No repository found");
    }
    Ok(repos)
}

pub fn cmd_repair(args: RepairArgs) -> anyhow::Result<()> {
    let ctx = Setup::new().require_repo(None).build()?;
    let repos = db_repos(&ctx.db)?;

    let mut total_fixed = 0usize;
    let mut total_docs = 0usize;

    for repo in &repos {
        let docs = if let Some(ref doc_id) = args.doc {
            match ctx.db.get_document(doc_id)? {
                Some(doc) if doc.repo_id == repo.id => vec![doc],
                _ => continue,
            }
        } else {
            ctx.db.list_documents(None, Some(&repo.id), None, 10000)?
        };

        for doc in &docs {
            let disk_content = std::fs::read_to_string(&doc.file_path).ok();
            let content = disk_content.as_deref().unwrap_or(&doc.content);

            let result = repair_document(content);
            if result.fixes == 0 {
                continue;
            }

            total_docs += 1;
            total_fixed += result.fixes;

            if !args.quiet {
                println!("{} [{}]: {} fix(es)", doc.title, doc.id, result.fixes);
                for desc in &result.descriptions {
                    println!("  - {desc}");
                }
            }

            if !args.dry_run {
                if let Some(ref repaired) = result.content {
                    let path = Path::new(&doc.file_path);
                    if path.exists() {
                        std::fs::write(path, repaired)?;
                        let new_hash = content_hash(repaired);
                        ctx.db
                            .update_document_content(&doc.id, repaired, &new_hash)?;
                        if !args.quiet {
                            println!("  ✓ Written to disk");
                        }
                    }
                }
            } else if !args.quiet {
                println!("  (dry run — no changes written)");
            }
        }
    }

    if !args.quiet {
        if total_fixed == 0 {
            println!("No corruption detected.");
        } else {
            let action = if args.dry_run { "would fix" } else { "fixed" };
            println!("\n{total_fixed} issue(s) {action} across {total_docs} document(s).");
        }
    }

    Ok(())
}
