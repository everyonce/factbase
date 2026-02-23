use super::create_repository;
use clap::Parser;
use factbase::{format_json, Config};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Initialize a new repository",
    after_help = "\
EXAMPLES:
    # Initialize current directory
    factbase init .

    # Initialize with custom name
    factbase init ~/notes --name \"My Notes\"

    # Initialize with custom ID
    factbase init ~/docs --id docs --name \"Documentation\"

    # Output as JSON for scripting
    factbase init . --json
"
)]
pub struct InitArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub id: Option<String>,
    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
    /// Also generate a starter config file at ~/.config/factbase/config.yaml
    #[arg(long)]
    pub config: bool,
}

pub fn cmd_init(args: InitArgs) -> anyhow::Result<()> {
    let path = args.path.canonicalize().unwrap_or_else(|_| {
        fs::create_dir_all(&args.path).ok();
        args.path.clone()
    });
    let factbase_dir = path.join(".factbase");
    if factbase_dir.exists() {
        if args.json {
            let json = serde_json::json!({
                "config_path": factbase_dir.display().to_string(),
                "created": false,
                "message": format!("Already initialized: {}", path.display())
            });
            println!("{}", format_json(&json)?);
        }
        anyhow::bail!("Already initialized: {}", path.display());
    }
    fs::create_dir_all(&factbase_dir)?;

    let perspective_path = path.join("perspective.yaml");
    if !perspective_path.exists() {
        fs::write(&perspective_path, "# Factbase perspective — tells agents what this knowledge base is about\n\n# Your organization name (helps agents understand context)\n# organization: Acme Corp\n\n# What this knowledge base focuses on\n# focus: Customer relationship intelligence for AWS solutions architects\n\n# Allowed document types (derived from folder names)\n# allowed_types:\n#   - person\n#   - company\n#   - project\n\n# Review quality settings\n# review:\n#   stale_days: 180\n#   required_fields:\n#     person: [current_role, location, company]\n#     company: [industry, headquarters]\n")?;
    }

    let db_path = factbase_dir.join("factbase.db");
    let config = Config::default();
    let db = config.open_database(&db_path)?;

    let repo_id = args.id.unwrap_or_else(|| "main".into());
    let repo_name = args.name.unwrap_or_else(|| {
        path.file_name()
            .map_or_else(|| "main".into(), |s| s.to_string_lossy().to_string())
    });
    let repo = create_repository(&repo_id, &repo_name, &path);
    db.upsert_repository(&repo)?;

    // Optionally generate starter config
    let mut config_created = false;
    if args.config {
        let config_path = Config::default_path();
        if config_path.exists() {
            if !args.json {
                println!("Config already exists at {}", config_path.display());
            }
        } else {
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let default_config = Config::default();
            let yaml = serde_yaml_ng::to_string(&default_config)?;
            fs::write(&config_path, yaml)?;
            config_created = true;
            if !args.json {
                println!("Created config at {}", config_path.display());
            }
        }
    }

    if args.json {
        let json = serde_json::json!({
            "config_path": factbase_dir.display().to_string(),
            "created": true,
            "config_created": config_created,
            "message": format!("Initialized factbase at {}", path.display())
        });
        println!("{}", format_json(&json)?);
    } else {
        println!("Initialized factbase at {}", path.display());
        println!("Next: Add markdown files and run `factbase scan`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use factbase::Database;
    use tempfile::TempDir;

    #[test]
    fn test_init_args_default() {
        let args = InitArgs {
            path: PathBuf::from("."),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        assert!(!args.json);
        assert!(!args.config);
        assert!(args.name.is_none());
        assert!(args.id.is_none());
    }

    #[test]
    fn test_init_args_json() {
        let args = InitArgs {
            path: PathBuf::from("/tmp/test"),
            name: Some("Test".into()),
            id: Some("test".into()),
            json: true,
            config: false,
        };
        assert!(args.json);
        assert_eq!(args.name, Some("Test".into()));
        assert_eq!(args.id, Some("test".into()));
    }

    #[test]
    fn test_init_creates_factbase_dir() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        assert!(tmp.path().join(".factbase").exists());
        assert!(tmp.path().join(".factbase/factbase.db").exists());
    }

    #[test]
    fn test_init_default_repo_id() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let db = Database::new(&tmp.path().join(".factbase/factbase.db")).unwrap();
        let repos = db.list_repositories().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].id, "main");
    }

    #[test]
    fn test_init_custom_repo_id_and_name() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: Some("My Notes".into()),
            id: Some("notes".into()),
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let db = Database::new(&tmp.path().join(".factbase/factbase.db")).unwrap();
        let repos = db.list_repositories().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].id, "notes");
        assert_eq!(repos[0].name, "My Notes");
    }

    #[test]
    fn test_init_already_initialized_fails() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let args2 = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        let result = cmd_init(args2);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Already initialized"));
    }
}
