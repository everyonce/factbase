use anyhow::Context;

use super::{print_output, resolve_repos, OutputFormat};
use crate::commands::setup::Setup;
use clap::Parser;
use factbase::database::Database;
use serde::Serialize;

#[derive(Parser)]
#[command(
    about = "Explore document link relationships",
    after_help = "\
EXAMPLES:
    factbase links abc123              # Show outgoing links
    factbase links abc123 --reverse    # Show incoming links
    factbase links --orphans           # List unlinked documents
    factbase links --top 10            # Most connected documents
    factbase links --top 5 --json      # JSON output
"
)]
pub struct LinksArgs {
    /// Document ID (optional, required unless --orphans or --top)
    pub id: Option<String>,

    /// Show incoming links instead of outgoing
    #[arg(long)]
    pub reverse: bool,

    /// List documents with no links (orphans)
    #[arg(long)]
    pub orphans: bool,

    /// Show N most connected documents
    #[arg(long, value_name = "N")]
    pub top: Option<usize>,

    /// Filter by repository
    #[arg(short = 'r', long)]
    pub repo: Option<String>,

    /// Output as JSON (shorthand for --format json)
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Serialize)]
struct LinkOutput {
    id: String,
    title: String,
    direction: String,
    links: Vec<LinkInfo>,
}

#[derive(Serialize)]
struct LinkInfo {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Serialize)]
struct OrphanOutput {
    orphans: Vec<OrphanDoc>,
    count: usize,
}

#[derive(Serialize)]
struct OrphanDoc {
    id: String,
    title: String,
}

#[derive(Serialize)]
struct TopConnectedOutput {
    documents: Vec<ConnectedDoc>,
}

#[derive(Serialize)]
struct ConnectedDoc {
    id: String,
    title: String,
    link_count: usize,
}

pub fn cmd_links(args: LinksArgs) -> anyhow::Result<()> {
    let db = Setup::new().build()?.db;
    let format = OutputFormat::resolve(args.json, args.format);

    if args.orphans {
        return show_orphans(&db, args.repo.as_deref(), format);
    }

    if let Some(n) = args.top {
        return show_top_connected(&db, args.repo.as_deref(), n, format);
    }

    let id = args
        .id
        .context("Document ID required unless using --orphans or --top")?;

    show_document_links(&db, &id, args.reverse, format)
}

fn show_document_links(
    db: &Database,
    id: &str,
    reverse: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let doc = db.require_document(id)?;

    let (links, direction) = if reverse {
        (db.get_links_to(id)?, "incoming")
    } else {
        (db.get_links_from(id)?, "outgoing")
    };

    let link_infos: Vec<LinkInfo> = links
        .iter()
        .filter_map(|link| {
            let target_id = if reverse {
                &link.source_id
            } else {
                &link.target_id
            };
            db.get_document(target_id).ok().flatten().map(|d| LinkInfo {
                id: target_id.clone(),
                title: d.title,
                context: link.context.clone(),
            })
        })
        .collect();

    let output = LinkOutput {
        id: doc.id.clone(),
        title: doc.title.clone(),
        direction: direction.to_string(),
        links: link_infos,
    };

    print_output(format, &output, || {
        let arrow = if reverse { "←" } else { "→" };
        println!("{} ({}) {} links:", doc.id, doc.title, direction);
        if output.links.is_empty() {
            println!("  (none)");
        } else {
            for link in &output.links {
                println!("  {} {} ({})", arrow, link.id, link.title);
            }
        }
    })?;

    Ok(())
}

fn show_orphans(
    db: &Database,
    repo_filter: Option<&str>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let repos = db.list_repositories()?;
    let repos = resolve_repos(repos, repo_filter)?;

    let mut all_orphans = Vec::with_capacity(repos.len() * 8); // Estimate ~8 orphans per repo
    for repo in &repos {
        let stats = db.get_detailed_stats(&repo.id, None)?;
        for (id, title) in stats.orphans {
            all_orphans.push(OrphanDoc { id, title });
        }
    }

    let output = OrphanOutput {
        count: all_orphans.len(),
        orphans: all_orphans,
    };

    print_output(format, &output, || {
        println!("Orphan documents (no links): {}", output.count);
        if output.orphans.is_empty() {
            println!("  (none)");
        } else {
            for doc in &output.orphans {
                println!("  {} ({})", doc.id, doc.title);
            }
        }
    })?;

    Ok(())
}

fn show_top_connected(
    db: &Database,
    repo_filter: Option<&str>,
    n: usize,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let repos = db.list_repositories()?;
    let repos = resolve_repos(repos, repo_filter)?;

    let mut all_docs: Vec<ConnectedDoc> = Vec::with_capacity(repos.len() * 10); // Estimate ~10 linked docs per repo
    for repo in &repos {
        let stats = db.get_detailed_stats(&repo.id, None)?;
        for (id, title, count) in stats.most_linked {
            all_docs.push(ConnectedDoc {
                id,
                title,
                link_count: count,
            });
        }
    }

    // Sort by link count descending and take top N
    all_docs.sort_by(|a, b| b.link_count.cmp(&a.link_count));
    all_docs.truncate(n);

    let output = TopConnectedOutput {
        documents: all_docs,
    };

    print_output(format, &output, || {
        println!("Most connected documents:");
        if output.documents.is_empty() {
            println!("  (none)");
        } else {
            for (i, doc) in output.documents.iter().enumerate() {
                println!(
                    "  {}. {} ({}) - {} links",
                    i + 1,
                    doc.id,
                    doc.title,
                    doc.link_count
                );
            }
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_links_args_default() {
        let args = LinksArgs {
            id: Some("abc123".to_string()),
            reverse: false,
            orphans: false,
            top: None,
            repo: None,
            json: false,
            format: OutputFormat::Table,
        };
        assert_eq!(args.id, Some("abc123".to_string()));
        assert!(!args.reverse);
        assert!(!args.orphans);
    }

    #[test]
    fn test_links_args_reverse() {
        let args = LinksArgs {
            id: Some("abc123".to_string()),
            reverse: true,
            orphans: false,
            top: None,
            repo: None,
            json: false,
            format: OutputFormat::Table,
        };
        assert!(args.reverse);
    }

    #[test]
    fn test_links_args_orphans() {
        let args = LinksArgs {
            id: None,
            reverse: false,
            orphans: true,
            top: None,
            repo: None,
            json: false,
            format: OutputFormat::Table,
        };
        assert!(args.orphans);
        assert!(args.id.is_none());
    }

    #[test]
    fn test_links_args_top() {
        let args = LinksArgs {
            id: None,
            reverse: false,
            orphans: false,
            top: Some(10),
            repo: None,
            json: false,
            format: OutputFormat::Table,
        };
        assert_eq!(args.top, Some(10));
    }
}
