use super::setup_database;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    version,
    about = "Generate shell completions",
    after_help = "\
EXAMPLES:
    # Bash
    factbase completions bash > ~/.local/share/bash-completion/completions/factbase

    # Zsh
    factbase completions zsh > ~/.zfunc/_factbase

    # Fish
    factbase completions fish > ~/.config/fish/completions/factbase.fish

    # Include repository IDs (regenerate after adding/removing repos)
    factbase completions bash --with-repos > ~/.local/share/bash-completion/completions/factbase
"
)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: Shell,

    /// Include current repository IDs in completions (regenerate after adding/removing repos)
    #[arg(long)]
    pub with_repos: bool,
}

pub fn cmd_completions(args: CompletionsArgs) {
    let mut cmd = crate::Cli::command();

    if args.with_repos {
        // Generate completions with dynamic repo IDs
        let repo_ids = get_repo_ids();
        let mut output = Vec::new();
        generate(args.shell, &mut cmd, "factbase", &mut output);
        let script = String::from_utf8_lossy(&output);
        let enhanced = inject_repo_completions(&script, &repo_ids, args.shell);
        io::stdout()
            .write_all(enhanced.as_bytes())
            .expect("Failed to write completions");
    } else {
        generate(args.shell, &mut cmd, "factbase", &mut io::stdout());
    }
}

fn get_repo_ids() -> Vec<String> {
    match setup_database() {
        Ok((_, db)) => db
            .list_repositories()
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.id)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn inject_repo_completions(script: &str, repo_ids: &[String], shell: Shell) -> String {
    if repo_ids.is_empty() {
        return script.to_string();
    }

    match shell {
        Shell::Bash => inject_bash_repos(script, repo_ids),
        Shell::Zsh => inject_zsh_repos(script, repo_ids),
        Shell::Fish => inject_fish_repos(script, repo_ids),
        _ => script.to_string(), // PowerShell/Elvish: return unchanged
    }
}

fn inject_bash_repos(script: &str, repo_ids: &[String]) -> String {
    let repos_str = repo_ids.join(" ");
    // Replace file completion for --repo with repo ID completion
    script.replace(
        "--repo)\n                    COMPREPLY=($(compgen -f \"${cur}\"))",
        &format!(
            "--repo)\n                    COMPREPLY=($(compgen -W \"{}\" -- \"${{cur}}\"))",
            repos_str
        ),
    )
}

fn inject_zsh_repos(script: &str, repo_ids: &[String]) -> String {
    let repos_list = repo_ids
        .iter()
        .map(|id| format!("'{}'", id))
        .collect::<Vec<_>>()
        .join(" ");
    // Replace ':REPO:_default' with ':REPO:(repo1 repo2 ...)'
    script
        .replace(":REPO:_default'", &format!(":REPO:({})' ", repos_list))
        .replace(":repo:_default'", &format!(":repo:({})' ", repos_list))
}

fn inject_fish_repos(script: &str, repo_ids: &[String]) -> String {
    let repos_str = repo_ids.join(" ");
    // Replace '-l repo -r' with '-l repo -r -f -a "repo1 repo2"'
    // The -f flag prevents file completion, -a adds the repo IDs as completions
    script
        .replace(
            "-l repo -r\n",
            &format!("-l repo -r -f -a \"{}\"\n", repos_str),
        )
        .replace(
            "-s r -l repo -r\n",
            &format!("-s r -l repo -r -f -a \"{}\"\n", repos_str),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completions_args_shell_parsing() {
        // Test that shell argument is required and parsed correctly
        let args = CompletionsArgs::try_parse_from(["completions", "bash"]).unwrap();
        assert_eq!(args.shell, Shell::Bash);
        assert!(!args.with_repos);

        let args = CompletionsArgs::try_parse_from(["completions", "zsh"]).unwrap();
        assert_eq!(args.shell, Shell::Zsh);

        let args = CompletionsArgs::try_parse_from(["completions", "fish"]).unwrap();
        assert_eq!(args.shell, Shell::Fish);
    }

    #[test]
    fn test_completions_args_with_repos_flag() {
        let args =
            CompletionsArgs::try_parse_from(["completions", "bash", "--with-repos"]).unwrap();
        assert_eq!(args.shell, Shell::Bash);
        assert!(args.with_repos);
    }

    #[test]
    fn test_completions_args_invalid_shell() {
        let result = CompletionsArgs::try_parse_from(["completions", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_inject_repo_completions_empty_repos() {
        let script = "some completion script";
        let result = inject_repo_completions(script, &[], Shell::Bash);
        assert_eq!(result, script);
    }

    #[test]
    fn test_inject_bash_repos() {
        let script = "--repo)\n                    COMPREPLY=($(compgen -f \"${cur}\"))";
        let repos = vec!["main".to_string(), "notes".to_string()];
        let result = inject_bash_repos(script, &repos);
        assert!(result.contains("compgen -W \"main notes\""));
        assert!(!result.contains("compgen -f"));
    }

    #[test]
    fn test_inject_zsh_repos() {
        let script = "':REPO:_default'";
        let repos = vec!["main".to_string(), "docs".to_string()];
        let result = inject_zsh_repos(script, &repos);
        assert!(result.contains("'main'"));
        assert!(result.contains("'docs'"));
        assert!(!result.contains("_default"));
    }

    #[test]
    fn test_inject_fish_repos() {
        let script = "-l repo -r\n";
        let repos = vec!["repo1".to_string(), "repo2".to_string()];
        let result = inject_fish_repos(script, &repos);
        assert!(result.contains("-f -a \"repo1 repo2\""));
    }
}
