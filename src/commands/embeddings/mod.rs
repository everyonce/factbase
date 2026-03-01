//! Embedding import/export CLI commands.

mod args;

pub use args::{EmbeddingsArgs, EmbeddingsCommands};

use super::setup::setup_database_only;
use factbase::Config;

pub fn cmd_embeddings(args: EmbeddingsArgs) -> anyhow::Result<()> {
    match args.command {
        EmbeddingsCommands::Export(a) => cmd_export(a),
        EmbeddingsCommands::Import(a) => cmd_import(a),
        EmbeddingsCommands::Status(a) => cmd_status(a),
    }
}

fn cmd_export(args: args::ExportArgs) -> anyhow::Result<()> {
    let config = Config::load(None)?;
    let db = setup_database_only()?;
    let model = config.embedding.model.clone();

    let (chunk_count, fact_count) = factbase::export_embeddings_to_file(
        &db,
        args.repo.as_deref(),
        &model,
        &args.output,
    )?;

    eprintln!(
        "Exported {chunk_count} embedding chunks and {fact_count} fact embeddings to {}",
        args.output.display()
    );
    Ok(())
}

fn cmd_import(args: args::ImportArgs) -> anyhow::Result<()> {
    let db = setup_database_only()?;

    let result = factbase::import_embeddings_from_file(&db, &args.input, args.force)?;

    eprintln!(
        "Imported {} chunks ({} skipped), {} fact embeddings ({} skipped) — model: {}, dimension: {}",
        result.imported_chunks, result.skipped_chunks,
        result.imported_facts, result.skipped_facts,
        result.model, result.dimension
    );
    Ok(())
}

fn cmd_status(args: args::StatusArgs) -> anyhow::Result<()> {
    let config = Config::load(None)?;
    let db = setup_database_only()?;
    let model = config.embedding.model.clone();

    let info = factbase::embeddings_status(&db, args.repo.as_deref(), &model)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Embedding model:    {}", info.model);
        println!("Dimension:          {}", info.dimension.map_or("none".into(), |d| d.to_string()));
        println!("Documents indexed:  {}", info.total_documents);
        println!("Total chunks:       {}", info.total_chunks);
        if info.documents_without_embeddings > 0 {
            println!("Missing embeddings: {}", info.documents_without_embeddings);
        }
        if info.orphaned_chunks > 0 {
            println!("Orphaned chunks:    {}", info.orphaned_chunks);
        }
        println!();
        println!("Fact Embeddings:");
        println!("  Total fact embeddings: {}", info.total_fact_embeddings);
        let total_docs = info.documents_with_fact_embeddings + info.documents_without_fact_embeddings;
        println!("  Documents with facts:  {} / {}", info.documents_with_fact_embeddings, total_docs);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::args::*;
    use clap::Parser;

    #[test]
    fn test_parse_export_args() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "export", "--output", "out.jsonl"]).unwrap();
        assert!(matches!(args.command, EmbeddingsCommands::Export(_)));
    }

    #[test]
    fn test_parse_import_args() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "import", "--input", "in.jsonl"]).unwrap();
        assert!(matches!(args.command, EmbeddingsCommands::Import(_)));
    }

    #[test]
    fn test_parse_status_args() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "status"]).unwrap();
        assert!(matches!(args.command, EmbeddingsCommands::Status(_)));
    }

    #[test]
    fn test_parse_export_with_repo() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "export", "--output", "out.jsonl", "--repo", "myrepo"]).unwrap();
        if let EmbeddingsCommands::Export(a) = args.command {
            assert_eq!(a.repo, Some("myrepo".into()));
        }
    }

    #[test]
    fn test_parse_import_with_force() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "import", "--input", "in.jsonl", "--force"]).unwrap();
        if let EmbeddingsCommands::Import(a) = args.command {
            assert!(a.force);
        }
    }

    #[test]
    fn test_parse_status_with_json() {
        let args = EmbeddingsArgs::try_parse_from(["embeddings", "status", "--json"]).unwrap();
        if let EmbeddingsCommands::Status(a) = args.command {
            assert!(a.json);
        }
    }
}
