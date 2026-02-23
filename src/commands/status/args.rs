use super::OutputFormat;
use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "Show repository statistics (offline, no Ollama required)",
    after_help = "\
EXAMPLES:
    factbase status
    factbase status -r myrepo -d
    factbase status --format json
    factbase status --offline    # Explicit offline mode (same behavior)

NOTE: This command reads only from the local database and never contacts Ollama.
      The --offline flag is accepted for documentation purposes but has no effect.
"
)]
pub struct StatusArgs {
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long, short = 'd')]
    pub detailed: bool,
    #[arg(
        long,
        short = 'j',
        help = "Output as JSON (shorthand for --format json)"
    )]
    pub json: bool,
    #[arg(
        long,
        short = 'q',
        help = "Suppress non-essential output (useful for scripting)"
    )]
    pub quiet: bool,
    #[arg(long, short = 'f', value_enum, default_value = "table")]
    pub format: OutputFormat,
    #[arg(
        long,
        help = "Run in offline mode (no-op: status never contacts Ollama)"
    )]
    pub offline: bool,
    #[arg(
        long,
        help = "Only include documents modified since date (ISO 8601 or relative: 1h, 1d, 1w)"
    )]
    pub since: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_args_since_flag() {
        let args = StatusArgs::try_parse_from(["status", "--since", "1d"]).unwrap();
        assert_eq!(args.since, Some("1d".to_string()));
    }

    #[test]
    fn test_status_args_since_with_repo() {
        let args =
            StatusArgs::try_parse_from(["status", "--repo", "main", "--since", "1w"]).unwrap();
        assert_eq!(args.repo, Some("main".to_string()));
        assert_eq!(args.since, Some("1w".to_string()));
    }

    #[test]
    fn test_status_args_since_with_detailed() {
        let args = StatusArgs::try_parse_from(["status", "-d", "--since", "2024-01-01"]).unwrap();
        assert!(args.detailed);
        assert_eq!(args.since, Some("2024-01-01".to_string()));
    }

    #[test]
    fn test_status_args_since_with_json() {
        let args = StatusArgs::try_parse_from(["status", "--json", "--since", "1h"]).unwrap();
        assert!(args.json);
        assert_eq!(args.since, Some("1h".to_string()));
    }
}
