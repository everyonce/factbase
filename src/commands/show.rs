use super::{print_output, setup_database_only, OutputFormat};
use clap::Parser;
use factbase::Database;
use serde::Serialize;

#[derive(Parser)]
#[command(
    version,
    about = "Show document details",
    after_help = "\
EXAMPLES:
    factbase show abc123
    factbase show abc123 --json
    factbase show abc123 --format yaml
"
)]
pub struct ShowArgs {
    /// Document ID (6-character hex)
    pub id: String,

    /// Output as JSON (shorthand for --format json)
    #[arg(short, long)]
    pub json: bool,

    /// Output format (table, json, yaml)
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Serialize)]
struct ShowOutput {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_type: Option<String>,
    file_path: String,
    repo_id: String,
    indexed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_modified_at: Option<String>,
    links_to: Vec<LinkInfo>,
    linked_from: Vec<LinkInfo>,
}

#[derive(Serialize)]
struct LinkInfo {
    id: String,
    title: String,
}

pub fn cmd_show(args: ShowArgs) -> anyhow::Result<()> {
    let db = setup_database_only()?;
    let format = OutputFormat::resolve(args.json, args.format);

    let output = get_document_details(&db, &args.id)?;

    print_output(format, &output, || {
        println!("Document: {}", output.id);
        println!("Title:    {}", output.title);
        if let Some(ref t) = output.doc_type {
            println!("Type:     {t}");
        }
        println!("File:     {}", output.file_path);
        println!("Repo:     {}", output.repo_id);
        println!("Indexed:  {}", output.indexed_at);
        if let Some(ref m) = output.file_modified_at {
            println!("Modified: {m}");
        }

        if !output.links_to.is_empty() {
            println!("\nLinks to:");
            for link in &output.links_to {
                println!("  → {} ({})", link.id, link.title);
            }
        }

        if !output.linked_from.is_empty() {
            println!("\nLinked from:");
            for link in &output.linked_from {
                println!("  ← {} ({})", link.id, link.title);
            }
        }
    })?;

    Ok(())
}

fn get_document_details(db: &Database, id: &str) -> anyhow::Result<ShowOutput> {
    let doc = db.require_document(id)?;

    let links_from = db.get_links_from(id)?;
    let links_to_db = db.get_links_to(id)?;

    // Get titles for linked documents
    let links_to: Vec<LinkInfo> = links_from
        .iter()
        .filter_map(|link| {
            db.get_document(&link.target_id)
                .ok()
                .flatten()
                .map(|d| LinkInfo {
                    id: link.target_id.clone(),
                    title: d.title,
                })
        })
        .collect();

    let linked_from: Vec<LinkInfo> = links_to_db
        .iter()
        .filter_map(|link| {
            db.get_document(&link.source_id)
                .ok()
                .flatten()
                .map(|d| LinkInfo {
                    id: link.source_id.clone(),
                    title: d.title,
                })
        })
        .collect();

    Ok(ShowOutput {
        id: doc.id,
        title: doc.title,
        doc_type: doc.doc_type,
        file_path: doc.file_path,
        repo_id: doc.repo_id,
        indexed_at: doc.indexed_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        file_modified_at: doc
            .file_modified_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string()),
        links_to,
        linked_from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_args_default() {
        let args = ShowArgs {
            id: "abc123".to_string(),
            json: false,
            format: OutputFormat::Table,
        };
        assert_eq!(args.id, "abc123");
        assert!(!args.json);
    }

    #[test]
    fn test_show_args_json_flag() {
        let args = ShowArgs {
            id: "abc123".to_string(),
            json: true,
            format: OutputFormat::Table,
        };
        assert!(args.json);
    }
}
