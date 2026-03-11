use super::{create_repository, validate_directory_path};
use crate::commands::setup::Setup;
use clap::Parser;
use factbase::format_json;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    about = "Add a repository",
    after_help = "\
EXAMPLES:
    factbase repo add myrepo ~/notes
    factbase repo add work ~/work/docs --name \"Work Docs\"
"
)]
pub struct RepoAddArgs {
    pub id: String,
    pub path: PathBuf,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Parser)]
#[command(
    about = "Remove a repository",
    after_help = "\
EXAMPLES:
    factbase repo remove myrepo
    factbase repo remove myrepo --force
    factbase repo remove myrepo --dry-run
"
)]
pub struct RepoRemoveArgs {
    pub id: String,
    #[arg(long)]
    pub force: bool,
    /// Preview what would be deleted without making changes
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Parser)]
#[command(
    about = "List all repositories",
    after_help = "\
EXAMPLES:
    factbase repo list
    factbase repo list -j
    factbase repo list -q | xargs -I{} factbase scan --repo {}
"
)]
pub struct RepoListArgs {
    #[arg(long, short = 'j')]
    pub json: bool,
    #[arg(long, short = 'q', help = "Output only repo IDs, one per line")]
    pub quiet: bool,
}

/// Validate a repository ID for allowed characters
pub fn validate_repo_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty() {
        anyhow::bail!("Repository ID cannot be empty");
    }
    if id.len() > 32 {
        anyhow::bail!("Repository ID cannot exceed 32 characters");
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Repository ID can only contain alphanumeric characters, hyphens, and underscores"
        );
    }
    if id.starts_with('-') || id.starts_with('_') {
        anyhow::bail!("Repository ID cannot start with a hyphen or underscore");
    }
    Ok(())
}

/// Generate a default repository name from the path
pub fn default_repo_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string()
}

pub fn cmd_repo_add(args: RepoAddArgs) -> anyhow::Result<()> {
    // Validate repository ID
    validate_repo_id(&args.id)?;

    let path = factbase::organize::clean_canonicalize(&args.path);

    validate_directory_path(&path)?;

    let db = Setup::new().build()?.db;

    let repos = db.list_repositories()?;
    if repos.iter().any(|r| r.id == args.id) {
        anyhow::bail!("Repository ID '{}' already exists", args.id);
    }
    if repos.iter().any(|r| r.path == path) {
        anyhow::bail!("Path already registered: {}", path.display());
    }

    let name = args.name.unwrap_or_else(|| default_repo_name(&path));
    let repo = create_repository(&args.id, &name, &path);
    db.add_repository(&repo)?;

    println!("Added repository: {}", args.id);
    println!("Path: {}", path.display());
    println!("Run 'factbase scan --repo {}' to index documents", args.id);
    Ok(())
}

pub fn cmd_repo_remove(args: RepoRemoveArgs) -> anyhow::Result<()> {
    let db = Setup::new().build()?.db;

    let repos = db.list_repositories()?;
    let _repo = repos
        .iter()
        .find(|r| r.id == args.id)
        .ok_or_else(|| factbase::repo_not_found(&args.id))?;

    let doc_count = db.get_documents_for_repo(&args.id)?.len();

    // Dry-run mode: show what would be deleted and exit
    if args.dry_run {
        println!("Would remove repository: {}", args.id);
        println!("Would delete {doc_count} documents");
        return Ok(());
    }

    if !args.force && doc_count > 0 {
        print!(
            "Remove repository '{}' with {} documents? [y/N] ",
            args.id, doc_count
        );
        Write::flush(&mut io::stdout())?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled");
            return Ok(());
        }
    }

    let deleted = db.remove_repository(&args.id)?;
    println!("Removed repository: {}", args.id);
    println!("Deleted {deleted} documents");
    Ok(())
}

pub fn cmd_repo_list(args: RepoListArgs) -> anyhow::Result<()> {
    let db = Setup::new().build()?.db;

    let repos = db.list_repositories_with_stats()?;

    if args.json {
        let output: Vec<_> = repos
            .iter()
            .map(|(repo, count)| {
                serde_json::json!({
                    "id": repo.id,
                    "name": repo.name,
                    "path": repo.path,
                    "documents": count,
                })
            })
            .collect();
        println!("{}", format_json(&output)?);
        return Ok(());
    }

    if args.quiet {
        for (repo, _) in repos {
            println!("{}", repo.id);
        }
        return Ok(());
    }

    if repos.is_empty() {
        println!("No repositories registered");
        return Ok(());
    }

    println!("{:<12} {:<20} {:<8} PATH", "ID", "NAME", "DOCS");
    println!("{}", "-".repeat(60));
    for (repo, count) in repos {
        println!(
            "{:<12} {:<20} {:<8} {}",
            repo.id,
            repo.name,
            count,
            repo.path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_repo_id_valid() {
        assert!(validate_repo_id("myrepo").is_ok());
        assert!(validate_repo_id("my-repo").is_ok());
        assert!(validate_repo_id("my_repo").is_ok());
        assert!(validate_repo_id("repo123").is_ok());
        assert!(validate_repo_id("a").is_ok());
    }

    #[test]
    fn test_validate_repo_id_empty() {
        let err = validate_repo_id("").unwrap_err().to_string();
        assert!(err.contains("cannot be empty"));
    }

    #[test]
    fn test_validate_repo_id_too_long() {
        let long_id = "a".repeat(33);
        let err = validate_repo_id(&long_id).unwrap_err().to_string();
        assert!(err.contains("cannot exceed 32"));
    }

    #[test]
    fn test_validate_repo_id_invalid_chars() {
        assert!(validate_repo_id("my repo").is_err());
        assert!(validate_repo_id("my.repo").is_err());
        assert!(validate_repo_id("my/repo").is_err());
    }

    #[test]
    fn test_validate_repo_id_invalid_start() {
        assert!(validate_repo_id("-myrepo").is_err());
        assert!(validate_repo_id("_myrepo").is_err());
    }

    #[test]
    fn test_default_repo_name() {
        assert_eq!(
            default_repo_name(std::path::Path::new("/home/user/notes")),
            "notes"
        );
        assert_eq!(
            default_repo_name(std::path::Path::new("/home/user/my-docs")),
            "my-docs"
        );
    }

    #[test]
    fn test_default_repo_name_root() {
        // Root path should return "unnamed" or similar
        let name = default_repo_name(std::path::Path::new("/"));
        assert!(!name.is_empty());
    }

    #[test]
    fn test_repo_list_args_quiet_flag() {
        // Verify quiet flag exists and defaults to false
        let args = RepoListArgs {
            json: false,
            quiet: false,
        };
        assert!(!args.quiet);
        assert!(!args.json);

        let args_quiet = RepoListArgs {
            json: false,
            quiet: true,
        };
        assert!(args_quiet.quiet);
    }
}
