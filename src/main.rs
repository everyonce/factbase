mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use commands::{
    cmd_completions, cmd_db_vacuum, cmd_doctor, cmd_embeddings,
    cmd_repair, cmd_scan, cmd_status,
    cmd_version,
};
#[cfg(feature = "mcp")]
use commands::{cmd_mcp, cmd_serve};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

/// Log level for tracing output
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum LogLevel {
    Off,
    Error,
    #[default]
    Warn,
    Info,
    Debug,
    Trace,
}

/// Log format for tracing output
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

#[derive(Parser)]
#[command(
    name = "factbase",
    about = "Filesystem-based knowledge management — one KB per directory",
    after_long_help = "\
Quick start: factbase scan && factbase mcp
Full guide:  https://gitea.home.everyonce.com/daniel/factbase/src/branch/main/docs/quickstart.md

Hidden commands: db, completions, version (use 'factbase <cmd> --help')",
    version,
    long_version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (built ",
        env!("BUILD_DATE"),
        ", rustc ",
        env!("RUSTC_VERSION"),
        ")"
    )
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, global = true)]
    verbose: bool,
    /// Log level (overrides -v flag)
    #[arg(long, global = true, value_enum, default_value = "warn")]
    log_level: LogLevel,
    /// Log format
    #[arg(long, global = true, value_enum, default_value = "text")]
    log_format: LogFormat,
    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Index documents (re-scan current directory)
    Scan(commands::scan::ScanArgs),
    /// Start MCP server and file watcher
    #[cfg(feature = "mcp")]
    Serve(commands::serve::ServeArgs),
    /// Run MCP stdio transport (for agent integration)
    #[cfg(feature = "mcp")]
    Mcp,
    /// Show repository status and statistics
    Status(commands::status::StatusArgs),
    /// Auto-fix document corruption
    Repair(commands::repair::RepairArgs),
    /// Manage vector embeddings (export, import, status)
    #[command(subcommand)]
    Embeddings(commands::embeddings::EmbeddingsCommands),
    /// Check connectivity and model availability
    Doctor(commands::doctor::DoctorArgs),
    /// Database operations
    #[command(subcommand, hide = true)]
    Db(DbCommands),
    /// Generate shell completions
    #[command(hide = true)]
    Completions(commands::completions::CompletionsArgs),
    /// Show version and configuration info
    #[command(hide = true)]
    Version(commands::version::VersionArgs),
}

#[derive(Subcommand)]
enum DbCommands {
    Vacuum(commands::db::DbVacuumArgs),
    Stats(commands::db::DbStatsArgs),
    #[command(name = "backfill-word-counts")]
    BackfillWordCounts(commands::db::DbBackfillWordCountsArgs),
}

fn main() -> anyhow::Result<()> {
    // Spawn the main logic on a thread with 8MB stack (Windows default is 2MB,
    // which overflows with large async state machines).
    let builder = std::thread::Builder::new().stack_size(8 * 1024 * 1024);
    let handler = builder.spawn(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(8 * 1024 * 1024)
            .build()
            .expect("Failed to build tokio runtime")
            .block_on(async_main())
    })?;
    handler.join().unwrap_or_else(|e| {
        eprintln!("Fatal error: {e:?}");
        std::process::exit(1);
    })
}

async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize graceful shutdown handler for Ctrl+C
    factbase::shutdown::init_shutdown_handler();

    // Set global no-color flag if --no-color was passed
    if cli.no_color {
        factbase::output::set_no_color(true);
    }

    // Determine log level: --log-level takes precedence, then -v flags, then default
    let log_level = if cli.verbose {
        "debug"
    } else {
        match cli.log_level {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    // Configure tracing subscriber based on format (logs go to stderr)
    match cli.log_format {
        LogFormat::Text => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(std::io::stderr)
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(std::io::stderr)
                .json()
                .init();
        }
    }

    match cli.command {
        Commands::Scan(args) => cmd_scan(args).await?,
        Commands::Status(args) => cmd_status(args)?,
        #[cfg(feature = "mcp")]
        Commands::Serve(args) => cmd_serve(args).await?,
        #[cfg(feature = "mcp")]
        Commands::Mcp => cmd_mcp().await?,
        Commands::Db(cmd) => match cmd {
            DbCommands::Vacuum(_) => cmd_db_vacuum()?,
            DbCommands::Stats(args) => commands::db::cmd_db_stats(args)?,
            DbCommands::BackfillWordCounts(_) => commands::db::cmd_db_backfill_word_counts()?,
        },
        Commands::Repair(args) => cmd_repair(args)?,
        Commands::Completions(args) => cmd_completions(args),
        Commands::Embeddings(cmd) => cmd_embeddings(commands::embeddings::EmbeddingsArgs { command: cmd })?,
        Commands::Doctor(args) => cmd_doctor(args).await?,
        Commands::Version(args) => cmd_version(args)?,
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_scan_default() {
        let cli = Cli::try_parse_from(["factbase", "scan"]).unwrap();
        assert!(matches!(cli.command, Commands::Scan(_)));
        assert!(!cli.verbose);
    }

    #[test]
    fn parse_status_default() {
        let cli = Cli::try_parse_from(["factbase", "status"]).unwrap();
        assert!(matches!(cli.command, Commands::Status(_)));
    }

    #[test]
    fn parse_global_verbose() {
        let cli = Cli::try_parse_from(["factbase", "-v", "scan"]).unwrap();
        assert!(matches!(cli.command, Commands::Scan(_)));
        assert!(cli.verbose);
    }

    #[test]
    fn parse_global_verbose_after_subcommand() {
        let cli = Cli::try_parse_from(["factbase", "scan", "-v"]).unwrap();
        assert!(matches!(cli.command, Commands::Scan(_)));
        assert!(cli.verbose);
    }

    /// Ensure every subcommand parses without TypeId mismatch panics.
    #[test]
    fn parse_all_subcommands_with_global_verbose() {
        let cases: &[&[&str]] = &[
            &["factbase", "-v", "scan", "--dry-run"],
            &["factbase", "-v", "status"],
            &["factbase", "-v", "doctor"],
            &["factbase", "-v", "repair", "--dry-run"],
        ];
        for args in cases {
            Cli::try_parse_from(*args).unwrap_or_else(|e| {
                panic!("Failed to parse {:?}: {e}", args);
            });
        }
    }
}
