//! Command argument parsing for doctor command.

use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "Check inference backend connectivity and models",
    after_help = "\
EXAMPLES:
    factbase doctor
    factbase doctor --fix
    factbase doctor --dry-run
    factbase doctor --json
    factbase doctor -q && factbase scan
    factbase doctor --timeout 5      # Quick check
    factbase doctor --timeout 120    # Slow network
"
)]
pub struct DoctorArgs {
    /// Auto-fix common issues (pull missing models, create config)
    #[arg(long)]
    pub fix: bool,
    /// Show what would be fixed without making changes
    #[arg(long)]
    pub dry_run: bool,
    /// Suppress output on success (exit 0 if healthy, 1 if not)
    #[arg(short, long)]
    pub quiet: bool,
    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
    /// HTTP timeout in seconds (default: from config, typically 30)
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_args_default() {
        let args = DoctorArgs {
            fix: false,
            dry_run: false,
            quiet: false,
            json: false,
            timeout: None,
        };
        assert!(!args.fix);
        assert!(!args.dry_run);
        assert!(!args.quiet);
        assert!(!args.json);
        assert!(args.timeout.is_none());
    }

    #[test]
    fn test_doctor_args_with_fix() {
        let args = DoctorArgs {
            fix: true,
            dry_run: false,
            quiet: false,
            json: false,
            timeout: None,
        };
        assert!(args.fix);
    }

    #[test]
    fn test_doctor_args_with_timeout() {
        let args = DoctorArgs {
            fix: false,
            dry_run: false,
            quiet: false,
            json: false,
            timeout: Some(60),
        };
        assert_eq!(args.timeout, Some(60));
    }

    #[test]
    fn test_doctor_args_quiet_mode() {
        let args = DoctorArgs {
            fix: false,
            dry_run: false,
            quiet: true,
            json: false,
            timeout: None,
        };
        assert!(args.quiet);
    }

    #[test]
    fn test_doctor_args_json_mode() {
        let args = DoctorArgs {
            fix: false,
            dry_run: false,
            quiet: false,
            json: true,
            timeout: None,
        };
        assert!(args.json);
    }
}
