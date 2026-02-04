//! NoString Server — headless daemon for 24/7 inheritance monitoring
//!
//! Reuses all NoString library crates (watch, notify, electrum, inherit)
//! without the Tauri desktop UI. Designed for Docker / server deployment.
//!
//! # Usage
//!
//! ```bash
//! nostring-server --config /path/to/nostring-server.toml
//! nostring-server --check   # Run one check cycle and exit
//! nostring-server --validate # Validate config and exit
//! ```

mod config;
mod daemon;

use anyhow::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    // Security hardening: disable core dumps to prevent seed material leaking to disk
    nostring_core::memory::disable_core_dumps();

    // Initialize rustls CryptoProvider before any Nostr/TLS operations.
    // Without this, WebSocket connections via nostr-sdk will panic.
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // Parse CLI args (minimal — no clap dependency needed)
    let args: Vec<String> = std::env::args().collect();

    let mut config_path = PathBuf::from("/config/nostring-server.toml");
    let mut one_shot = false;
    let mut validate_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" | "-c" => {
                i += 1;
                if i < args.len() {
                    config_path = PathBuf::from(&args[i]);
                } else {
                    anyhow::bail!("--config requires a path argument");
                }
            }
            "--check" | "--once" => {
                one_shot = true;
            }
            "--validate" => {
                validate_only = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--version" | "-V" => {
                println!("nostring-server {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => {
                anyhow::bail!("Unknown argument: {}", other);
            }
        }
        i += 1;
    }

    // Load config
    let mut server_config = config::ServerConfig::from_file(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path.display()))?;

    // Apply env overrides
    server_config.apply_env_overrides();

    // Validate
    server_config
        .validate()
        .context("Configuration validation failed")?;

    // Init logger
    std::env::set_var("RUST_LOG", &server_config.server.log_level);
    env_logger::init();

    if validate_only {
        println!("✅ Configuration is valid.");
        println!("  Network:       {}", server_config.bitcoin.network);
        println!("  Electrum:      {}", server_config.bitcoin.electrum_url);
        println!(
            "  Descriptor:    {}…",
            &server_config.policy.descriptor[..server_config.policy.descriptor.len().min(60)]
        );
        println!(
            "  Timelock:      {} blocks",
            server_config.policy.timelock_blocks
        );
        println!(
            "  Check interval: {} secs",
            server_config.server.check_interval_secs
        );
        println!(
            "  Nostr notify:  {}",
            server_config.notifications.nostr.is_some()
        );
        println!(
            "  Email notify:  {}",
            server_config.notifications.email.is_some()
        );
        println!(
            "  Heirs:         {}",
            server_config.notifications.heirs.len()
        );
        return Ok(());
    }

    // Build tokio runtime
    let rt = tokio::runtime::Runtime::new().context("Failed to create Tokio runtime")?;

    if one_shot {
        log::info!("Running single check cycle…");
        rt.block_on(daemon::run_check_cycle(&server_config))?;
        log::info!("Done.");
    } else {
        // Install Ctrl-C handler for graceful shutdown
        let shutdown = rt.block_on(async {
            tokio::select! {
                result = daemon::run(server_config) => result,
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Received shutdown signal. Exiting…");
                    Ok(())
                }
            }
        });

        if let Err(e) = shutdown {
            log::error!("Server error: {:#}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        r#"NoString Server — headless inheritance monitoring daemon

USAGE:
    nostring-server [OPTIONS]

OPTIONS:
    -c, --config <PATH>   Config file path (default: /config/nostring-server.toml)
    --check, --once       Run a single check cycle and exit
    --validate            Validate config file and exit
    -h, --help            Show this help message
    -V, --version         Show version

ENVIRONMENT VARIABLES (override config file):
    NOSTRING_DATA_DIR         Data directory path
    NOSTRING_CHECK_INTERVAL   Check interval in seconds
    NOSTRING_LOG_LEVEL        Log level (error/warn/info/debug/trace)
    NOSTRING_NETWORK          Bitcoin network (bitcoin/testnet/signet/regtest)
    NOSTRING_ELECTRUM_URL     Electrum server URL
    NOSTRING_DESCRIPTOR       Inheritance descriptor
    NOSTRING_TIMELOCK_BLOCKS  Timelock in blocks
    NOSTRING_SERVICE_KEY      Nostr service key (nsec or hex)
    NOSTRING_OWNER_NPUB       Owner's Nostr public key

EXAMPLES:
    # Run as daemon with config file
    nostring-server --config /path/to/config.toml

    # Single check (useful for cron jobs)
    nostring-server --config config.toml --check

    # Validate configuration
    nostring-server --config config.toml --validate
"#
    );
}
