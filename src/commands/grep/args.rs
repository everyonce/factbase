use super::OutputFormat;
use clap::Parser;

#[derive(Parser)]
#[command(
    about = "Search document content for text patterns",
    after_help = "\
EXAMPLES:
    factbase grep \"TODO\"
    factbase grep -C 2 \"pattern\"
    factbase grep --highlight \"API\" | less -R
    factbase grep -r myrepo -t person \"email\"
    factbase grep --exclude-type draft \"TODO\"
    factbase grep --exclude-type draft --exclude-type archived \"FIXME\"
    factbase grep --dry-run \"complex.*regex\"
    factbase grep --stats \"TODO\"
    factbase grep --count \"FIXME\"
    factbase grep --since 1d \"FIXME\"
    factbase grep --since 1w \"TODO\"
    factbase grep -w \"TODO\"
"
)]
pub struct GrepArgs {
    /// Pattern to search for (case-insensitive)
    pub pattern: String,
    #[arg(long, short = 't')]
    pub doc_type: Option<String>,
    #[arg(long, short = 'r')]
    pub repo: Option<String>,
    #[arg(long, short = 'l', default_value = "10")]
    pub limit: usize,
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
    #[arg(
        long,
        short = 'f',
        value_enum,
        default_value = "table",
        help = "Output format"
    )]
    pub format: OutputFormat,
    #[arg(
        long,
        short = 'H',
        help = "Highlight matched text (default: auto-detect terminal)"
    )]
    pub highlight: Option<bool>,
    #[arg(
        long,
        short = 'C',
        default_value = "0",
        help = "Show N lines of context before and after each match"
    )]
    pub context: usize,
    #[arg(
        long,
        help = "Validate pattern and show search scope without searching"
    )]
    pub dry_run: bool,
    #[arg(long, help = "Show match statistics instead of full results")]
    pub stats: bool,
    #[arg(
        long,
        help = "Only search files modified since date (ISO 8601 or relative: 1h, 1d, 1w)"
    )]
    pub since: Option<String>,
    #[arg(long, help = "Output only the count of matching results")]
    pub count: bool,
    #[arg(
        long = "exclude-type",
        short = 'T',
        help = "Exclude documents of this type (can be repeated)"
    )]
    pub exclude_type: Option<Vec<String>>,
    #[arg(
        long,
        short = 'w',
        help = "Watch for file changes and re-run search (live updating results)"
    )]
    pub watch: bool,
}
