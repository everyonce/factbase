use super::create_repository;
use clap::Parser;
use factbase::config::Config;
use factbase::output::format_json;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
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
    let db_path = factbase_dir.join("factbase.db");

    // Tolerate pre-existing .factbase dir (e.g. user created config.yaml).
    // Only error if a repo is already registered in the database.
    if db_path.exists() {
        let config = Config::default();
        if let Ok(db) = config.open_database(&db_path) {
            if let Ok(repos) = db.list_repositories() {
                if !repos.is_empty() {
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
            }
        }
    }
    fs::create_dir_all(&factbase_dir)?;

    let perspective_path = path.join("perspective.yaml");
    if !perspective_path.exists() {
        fs::write(&perspective_path, factbase::models::PERSPECTIVE_TEMPLATE)?;
    }

    factbase::ensure_gitignore(&path)?;

    let config = Config::default();
    let db = config.open_database(&db_path)?;

    let repo_id = args
        .id
        .unwrap_or_else(|| factbase::DEFAULT_REPO_ID.into());
    let repo_name = args.name.unwrap_or_else(|| {
        path.file_name()
            .map_or_else(|| factbase::DEFAULT_REPO_ID.into(), |s| s.to_string_lossy().to_string())
    });
    let repo = create_repository(&repo_id, &repo_name, &path);
    db.upsert_repository(&repo)?;

    // Optionally generate starter config
    let mut config_created = false;
    if args.config {
        let config_path = factbase_dir.join("config.yaml");
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
    use factbase::database::Database;
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
        assert_eq!(repos[0].id, factbase::DEFAULT_REPO_ID);
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

    #[test]
    fn test_init_tolerates_preexisting_factbase_dir_with_config() {
        let tmp = TempDir::new().unwrap();
        let factbase_dir = tmp.path().join(".factbase");
        fs::create_dir_all(&factbase_dir).unwrap();
        fs::write(
            factbase_dir.join("config.yaml"),
            "embedding:\n  provider: bedrock\n",
        )
        .unwrap();

        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();

        // DB created and repo registered
        let db = Database::new(&factbase_dir.join("factbase.db")).unwrap();
        let repos = db.list_repositories().unwrap();
        assert_eq!(repos.len(), 1);

        // Pre-existing config.yaml preserved
        assert!(factbase_dir.join("config.yaml").exists());
    }

    #[test]
    fn test_init_tolerates_preexisting_empty_factbase_dir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".factbase")).unwrap();

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
    }

    #[test]
    fn test_init_creates_gitignore() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".factbase/"));
        assert!(content.contains(".fastembed_cache/"));
    }

    #[test]
    fn test_init_appends_to_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "node_modules/\n").unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".factbase/"));
        assert!(content.contains(".fastembed_cache/"));
    }

    #[test]
    fn test_init_skips_existing_gitignore_entries() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), ".factbase/\n").unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        // .factbase/ should appear only once
        assert_eq!(content.matches(".factbase/").count(), 1);
        // .fastembed_cache/ should be appended
        assert!(content.contains(".fastembed_cache/"));
    }

    #[test]
    fn test_init_perspective_template_is_domain_neutral() {
        let tmp = TempDir::new().unwrap();
        let args = InitArgs {
            path: tmp.path().to_path_buf(),
            name: None,
            id: None,
            json: false,
            config: false,
        };
        cmd_init(args).unwrap();
        let content = fs::read_to_string(tmp.path().join("perspective.yaml")).unwrap();

        // Must show multiple domain examples, not just business
        assert!(content.contains("Mycology") || content.contains("biology"));
        assert!(content.contains("civilization") || content.contains("history"));

        // Must not hardcode a single organization
        assert!(!content.contains("organization: Acme"));

        // All domain-specific lines should be commented out
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            assert!(
                trimmed.starts_with('#'),
                "Non-comment line in template: {trimmed}"
            );
        }
    }
}
