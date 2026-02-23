mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use commands::{
    cmd_completions, cmd_db_vacuum, cmd_doctor, cmd_export, cmd_grep, cmd_import, cmd_init,
    cmd_links, cmd_check, cmd_organize, cmd_repo_add, cmd_repo_list, cmd_repo_remove, cmd_review,
    cmd_scan, cmd_search, cmd_show, cmd_stats, cmd_status, cmd_version, StatsArgs,
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
    about = "Filesystem-based knowledge management",
    after_long_help = "\
Quick start: factbase init . && factbase scan && factbase search \"your query\"
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
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
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
    /// Initialize a new repository
    Init(commands::init::InitArgs),
    /// Index documents in repositories
    Scan(commands::scan::ScanArgs),
    /// Semantic search across documents
    Search(commands::search::SearchArgs),
    /// Start MCP server and file watcher
    #[cfg(feature = "mcp")]
    Serve(commands::serve::ServeArgs),
    /// Run MCP stdio transport (for agent integration)
    #[cfg(feature = "mcp")]
    Mcp,

    /// Search document content for text patterns
    Grep(commands::grep::GrepArgs),
    /// Show document details
    Show(commands::show::ShowArgs),
    /// Explore document link relationships
    Links(commands::links::LinksArgs),
    /// Show repository statistics
    Status(commands::status::StatusArgs),
    /// Show quick aggregate statistics
    Stats(StatsArgs),

    /// Check knowledge base quality
    Check(commands::check::CheckArgs),
    /// Process review questions
    Review(commands::review::ReviewArgs),
    /// Reorganize knowledge base
    #[command(subcommand)]
    Organize(commands::organize::OrganizeCommands),

    /// Manage repositories
    #[command(subcommand)]
    Repo(RepoCommands),
    /// Export documents from a repository
    Export(commands::export::ExportArgs),
    /// Import documents into a repository
    Import(commands::import::ImportArgs),
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
enum RepoCommands {
    Add(commands::repo::RepoAddArgs),
    Remove(commands::repo::RepoRemoveArgs),
    List(commands::repo::RepoListArgs),
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
    factbase::init_shutdown_handler();

    // Set global no-color flag if --no-color was passed
    if cli.no_color {
        factbase::set_no_color(true);
    }

    // Determine log level: --log-level takes precedence, then -v flags, then default
    let log_level = if cli.verbose > 0 {
        match cli.verbose {
            1 => "debug",
            _ => "trace",
        }
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
        Commands::Init(args) => cmd_init(args)?,
        Commands::Scan(args) => cmd_scan(args).await?,
        Commands::Status(args) => cmd_status(args)?,
        Commands::Stats(args) => cmd_stats(args)?,
        Commands::Search(args) => cmd_search(args).await?,
        Commands::Grep(args) => cmd_grep(args)?,
        #[cfg(feature = "mcp")]
        Commands::Serve(args) => cmd_serve(args).await?,
        #[cfg(feature = "mcp")]
        Commands::Mcp => cmd_mcp().await?,
        Commands::Repo(cmd) => match cmd {
            RepoCommands::Add(args) => cmd_repo_add(args)?,
            RepoCommands::Remove(args) => cmd_repo_remove(args)?,
            RepoCommands::List(args) => cmd_repo_list(args)?,
        },
        Commands::Db(cmd) => match cmd {
            DbCommands::Vacuum(_) => cmd_db_vacuum()?,
            DbCommands::Stats(args) => commands::db::cmd_db_stats(args)?,
            DbCommands::BackfillWordCounts(_) => commands::db::cmd_db_backfill_word_counts()?,
        },
        Commands::Organize(cmd) => {
            cmd_organize(commands::organize::OrganizeArgs { command: cmd }).await?
        }
        Commands::Completions(args) => cmd_completions(args),
        Commands::Export(args) => cmd_export(args)?,
        Commands::Import(args) => cmd_import(args)?,
        Commands::Doctor(args) => cmd_doctor(args).await?,
        Commands::Check(args) => cmd_check(args).await?,
        Commands::Review(args) => cmd_review(args).await?,
        Commands::Show(args) => cmd_show(args)?,
        Commands::Links(args) => cmd_links(args)?,
        Commands::Version(args) => cmd_version(args)?,
    }
    Ok(())
}
