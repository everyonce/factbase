//! Doctor command for checking inference backend connectivity and models.
//!
//! This module provides the `factbase doctor` command which checks:
//! - Database connectivity
//! - Inference backend availability (Bedrock or Ollama)
//! - Embedding model availability
//!
//! # Module Organization
//!
//! - `args` - Command argument parsing (`DoctorArgs`)
//! - `checks` - Health check functions and output structs
//! - `fix` - Auto-fix logic (pull models, create config)

mod args;
mod checks;
mod fix;

pub use args::DoctorArgs;

use checks::{
    check_database, check_ollama_server, fetch_available_models, model_available, CheckStatus,
    DoctorOutput,
};
use fix::{create_default_config, pull_ollama_model};

use anyhow::bail;
use factbase::{format_json, Config};
use std::time::Duration;

pub async fn cmd_doctor(args: DoctorArgs) -> anyhow::Result<()> {
    let quiet = args.quiet || args.json;

    macro_rules! qprintln {
        ($($arg:tt)*) => {
            if !quiet {
                println!($($arg)*);
            }
        };
    }

    // Load or create config
    let config = match Config::load(None) {
        Ok(c) => c,
        Err(e) => {
            if args.fix || args.dry_run {
                let config_path = Config::default_path();
                if args.dry_run {
                    qprintln!("Would create default config at: {}", config_path.display());
                    return Ok(());
                } else {
                    qprintln!("Creating default config at: {}", config_path.display());
                    let config = create_default_config()?;
                    qprintln!("✓ Created default config");
                    config
                }
            } else {
                let err_msg = format!("Config error: {e}");
                if args.json {
                    let output = DoctorOutput {
                        database: CheckStatus::err(&err_msg),
                        ollama_server: CheckStatus::err("Config not loaded"),
                        embedding_model: CheckStatus::err("Config not loaded"),
                        overall_healthy: false,
                    };
                    println!("{}", format_json(&output)?);
                    bail!("health check failed");
                }
                if !args.quiet {
                    bail!("{err_msg}. Run with --fix to create default config.");
                }
                bail!("health check failed");
            }
        }
    };

    let timeout_secs = args.timeout.unwrap_or(config.embedding.timeout_secs);
    let client = factbase::create_http_client(Duration::from_secs(timeout_secs));

    qprintln!("Checking system health...\n");

    // Check database
    let (db_ok, db_status, db_info) = check_database(&config);
    if db_ok {
        qprintln!("✓ Database: {}", db_info);
    } else {
        qprintln!(
            "✗ Database: {} ({})",
            db_info,
            db_status.error.as_deref().unwrap_or("unknown")
        );
        qprintln!("  Fix: Check file permissions or run 'factbase init'");
    }

    qprintln!();

    let is_bedrock = config.embedding.provider == "bedrock";
    let is_local = config.embedding.provider == "local";

    let (embed_ok, embed_status, server_ok, server_status) = if is_local {
        // Local provider: check that fastembed can initialize
        qprintln!("Checking local embedding provider...");
        qprintln!();

        #[cfg(feature = "local-embedding")]
        {
            match factbase::LocalEmbeddingProvider::new(false) {
                Ok(_) => {
                    qprintln!("✓ Local embedding: BGE-small-en-v1.5 (384-dim, CPU)");
                    (true, CheckStatus::ok(), true, CheckStatus::ok())
                }
                Err(e) => {
                    qprintln!("✗ Local embedding: {}", e);
                    qprintln!("  Fix: Check disk space and network (model downloads on first use)");
                    (
                        false,
                        CheckStatus::err(format!("{e}")),
                        true,
                        CheckStatus::ok(),
                    )
                }
            }
        }
        #[cfg(not(feature = "local-embedding"))]
        {
            qprintln!("✗ Local embedding: not available (binary built without local-embedding feature)");
            (
                false,
                CheckStatus::err("local-embedding feature not enabled"),
                true,
                CheckStatus::ok(),
            )
        }
    } else if is_bedrock {
        // Bedrock provider: check model configuration, not Ollama server
        qprintln!("Checking Bedrock configuration...\n");

        let region = config.embedding.effective_base_url();
        qprintln!(
            "✓ Embedding model: {} (Bedrock, region: {})",
            config.embedding.model,
            region
        );

        qprintln!();
        qprintln!("Tip: If you get AccessDeniedException during scan, enable model access at:");
        qprintln!("  https://console.aws.amazon.com/bedrock/home#/modelaccess");
        qprintln!("  Required permissions: bedrock:InvokeModel");

        (
            true,
            CheckStatus::ok(),
            true, // no server to check for Bedrock
            CheckStatus::ok(),
        )
    } else {
        // Ollama provider: check server and models
        qprintln!("Checking Ollama connectivity...\n");

        let base_url = config.embedding.effective_base_url();
        let (server_ok, server_status) = check_ollama_server(&client, base_url).await;
        if server_ok {
            qprintln!("✓ Ollama server: {} (running)", base_url);
        } else {
            qprintln!(
                "✗ Ollama server: {} ({})",
                base_url,
                server_status.error.as_deref().unwrap_or("unknown")
            );
            qprintln!("\n  Fix: Start Ollama with 'ollama serve'");
        }

        if server_ok {
            let models = fetch_available_models(&client, base_url).await;
            let (embed_ok, embed_status) =
                check_and_fix_model(&args, &models, &config.embedding.model, "Embedding", quiet);
            (
                embed_ok,
                embed_status,
                server_ok,
                CheckStatus::ok(),
            )
        } else {
            (
                false,
                CheckStatus::err("Ollama server not available"),
                false,
                server_status,
            )
        }
    };

    // Summary
    let all_ok = db_ok && server_ok && embed_ok;

    if args.json {
        let output = DoctorOutput {
            database: db_status,
            ollama_server: server_status,
            embedding_model: embed_status,
            overall_healthy: all_ok,
        };
        println!("{}", format_json(&output)?);
    }

    qprintln!();
    if all_ok {
        qprintln!("All checks passed. Ready to scan.");
        Ok(())
    } else {
        qprintln!("Some checks failed. Fix issues above before scanning.");
        bail!("health check failed")
    }
}

/// Check if a model is available and optionally fix by pulling it.
fn check_and_fix_model(
    args: &DoctorArgs,
    models: &[String],
    model_name: &str,
    label: &str,
    quiet: bool,
) -> (bool, CheckStatus) {
    macro_rules! qprintln {
        ($($arg:tt)*) => {
            if !quiet {
                println!($($arg)*);
            }
        };
    }

    if model_available(models, model_name) {
        qprintln!("✓ {} model: {} (available)", label, model_name);
        return (true, CheckStatus::ok());
    }

    qprintln!("✗ {} model: {} (not found)", label, model_name);

    if args.dry_run {
        qprintln!("  Would run: ollama pull {}", model_name);
        return (false, CheckStatus::err("not found"));
    }

    if args.fix {
        qprintln!("  Pulling model: ollama pull {}", model_name);
        match pull_ollama_model(model_name) {
            Ok(()) => {
                qprintln!("  ✓ Successfully pulled {}", model_name);
                return (true, CheckStatus::ok());
            }
            Err(e) => {
                qprintln!("  ✗ Failed to pull {}: {}", model_name, e);
                return (false, CheckStatus::err(format!("failed to pull: {e}")));
            }
        }
    }

    qprintln!("  Fix: ollama pull {}", model_name);
    (false, CheckStatus::err("not found"))
}
