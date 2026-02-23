//! Search command arguments.

use clap::Parser;

#[derive(Parser)]
#[command(
    about = "Semantic search across documents",
    after_help = "\
EXAMPLES:
    factbase search \"project deadlines\"
    factbase search -c \"API\" | head -5
    factbase search --as-of 2021-06 \"CTO\"
    factbase search --filter \"type:person\" --exclude \"has:temporal\" \"engineer\"
    factbase search --exclude-type draft --exclude-type archived \"meeting\"
    factbase search --count \"API\"
"
)]
pub struct SearchArgs {
    pub query: String,
    #[arg(long, short = 't', help = "Filter to specific document type")]
    pub doc_type: Option<String>,
    #[arg(
        long = "exclude-type",
        short = 'T',
        help = "Exclude documents of this type (can be repeated)"
    )]
    pub exclude_type: Option<Vec<String>>,
    #[arg(long, short = 'r')]
    pub repo: Option<String>,
    #[arg(long, short = 'l', default_value = "10")]
    pub limit: usize,
    #[arg(long, short = 'j')]
    pub json: bool,
    #[arg(
        long,
        short = 'q',
        help = "Suppress non-essential output (useful for scripting)"
    )]
    pub quiet: bool,
    #[arg(long, help = "Search by title instead of semantic search")]
    pub title: bool,
    #[arg(
        long,
        help = "Offline mode: use title search without Ollama (implies --title)"
    )]
    pub offline: bool,
    #[arg(
        long,
        help = "Filter to facts valid at specific date (YYYY, YYYY-MM, or YYYY-MM-DD)"
    )]
    pub as_of: Option<String>,
    #[arg(
        long,
        help = "Filter to facts valid during date range (YYYY..YYYY or YYYY-MM..YYYY-MM)"
    )]
    pub during: Option<String>,
    #[arg(
        long,
        help = "Exclude facts with unknown temporal context (@t[?] or no temporal tags)"
    )]
    pub exclude_unknown: bool,
    #[arg(
        long,
        help = "Boost ranking of facts with recent @t[~...] (LastSeen) dates"
    )]
    pub boost_recent: bool,
    #[arg(
        long,
        short = 'f',
        help = "Filter results by metadata (type:X, has:temporal, has:sources, links:>N)"
    )]
    pub filter: Option<Vec<String>>,
    #[arg(
        long,
        short = 'x',
        help = "Exclude results matching metadata (same syntax as --filter)"
    )]
    pub exclude: Option<Vec<String>>,
    #[arg(
        long,
        short = 'c',
        help = "Compact output: one line per result ([score] id: title)"
    )]
    pub compact: bool,
    #[arg(
        long,
        short = 's',
        help = "Show summary statistics instead of individual results"
    )]
    pub summary: bool,
    #[arg(
        long,
        help = "Timeout in seconds for Ollama API calls (default: from config, max: 300)"
    )]
    pub timeout: Option<u64>,
    #[arg(
        long,
        value_parser = ["relevance", "date", "title", "type"],
        default_value = "relevance",
        help = "Sort results by: relevance (default), date (newest first), title (alphabetical), type (grouped by type)"
    )]
    pub sort: String,
    #[arg(
        long,
        short = 'w',
        help = "Watch for file changes and re-run search (live updating results)"
    )]
    pub watch: bool,
    #[arg(long, help = "Output only the count of matching results")]
    pub count: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_args_count_flag() {
        let args = SearchArgs {
            query: "test".to_string(),
            doc_type: None,
            exclude_type: None,
            repo: None,
            limit: 10,
            json: false,
            quiet: false,
            title: false,
            offline: false,
            as_of: None,
            during: None,
            exclude_unknown: false,
            boost_recent: false,
            filter: None,
            exclude: None,
            compact: false,
            summary: false,
            timeout: None,
            sort: "relevance".to_string(),
            watch: false,
            count: true,
        };
        assert!(args.count);
        assert!(!args.json);
    }

    #[test]
    fn test_search_args_count_with_json() {
        let args = SearchArgs {
            query: "test".to_string(),
            doc_type: None,
            exclude_type: None,
            repo: None,
            limit: 10,
            json: true,
            quiet: false,
            title: false,
            offline: false,
            as_of: None,
            during: None,
            exclude_unknown: false,
            boost_recent: false,
            filter: None,
            exclude: None,
            compact: false,
            summary: false,
            timeout: None,
            sort: "relevance".to_string(),
            watch: false,
            count: true,
        };
        assert!(args.count);
        assert!(args.json);
    }

    #[test]
    fn test_search_args_offline_flag() {
        let args = SearchArgs {
            query: "test".to_string(),
            doc_type: None,
            exclude_type: None,
            repo: None,
            limit: 10,
            json: false,
            quiet: false,
            title: false,
            offline: true,
            as_of: None,
            during: None,
            exclude_unknown: false,
            boost_recent: false,
            filter: None,
            exclude: None,
            compact: false,
            summary: false,
            timeout: None,
            sort: "relevance".to_string(),
            watch: false,
            count: false,
        };
        assert!(args.offline);
        assert!(!args.title); // offline implies title, but doesn't set it
    }
}
