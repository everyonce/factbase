use clap::Parser;
use factbase::config::Config;
use factbase::output::format_json;

#[derive(Parser)]
#[command(
    about = "Show version and configuration info",
    after_help = "\
EXAMPLES:
    factbase version
    factbase version --json
"
)]
pub struct VersionArgs {
    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
}

pub fn cmd_version(args: VersionArgs) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let build_date = env!("BUILD_DATE");
    let rustc_version = env!("RUSTC_VERSION");

    // Try to load config for embedding model info
    let config = Config::load(None).ok();
    let embedding_model = config
        .as_ref()
        .map_or("not configured", |c| c.embedding.model.as_str());

    if args.json {
        let json = serde_json::json!({
            "version": version,
            "build_date": build_date,
            "rustc_version": rustc_version,
            "embedding_model": embedding_model,
        });
        println!("{}", format_json(&json)?);
    } else {
        println!("factbase {version}");
        println!("  Built:     {build_date}");
        println!("  Rustc:     {rustc_version}");
        println!("  Embedding: {embedding_model}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_args_default() {
        let args = VersionArgs { json: false };
        assert!(!args.json);
    }

    #[test]
    fn test_version_args_json() {
        let args = VersionArgs { json: true };
        assert!(args.json);
    }
}
