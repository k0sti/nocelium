use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use nocelium_channels::stdio::StdioChannel;
use nocelium_core::{Config, Identity};

#[derive(Parser)]
#[command(name = "nocelium", about = "Nostr-native AI agent runtime")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Generate a new identity and exit
    #[arg(long)]
    gen_identity: bool,

    /// Show identity info and exit
    #[arg(long)]
    show_identity: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nocelium=info".parse()?)
        )
        .init();

    let cli = Cli::parse();

    // Load config
    let config = match &cli.config {
        Some(path) => Config::load(path)?,
        None => Config::load_default()?,
    };

    // Load or generate identity
    let identity = Identity::load_or_generate(&config.identity)?;

    if cli.gen_identity {
        println!("Identity generated: {}", identity.npub());
        return Ok(());
    }

    if cli.show_identity {
        println!("npub: {}", identity.npub());
        return Ok(());
    }

    // Build the agent
    println!("Nocelium v{}", env!("CARGO_PKG_VERSION"));
    println!("Identity: {}", identity.npub());
    println!("Provider: {} ({})", config.provider.provider_type, config.provider.model);
    println!("Streaming: {}", config.agent.streaming);
    println!("Type /quit to exit\n");

    let agent = nocelium_core::agent::build_agent(&config, &identity)?;

    // Run with stdio channel
    let mut channel = StdioChannel::new();
    nocelium_core::agent::run_loop(&agent, &mut channel, config.agent.streaming).await?;

    Ok(())
}
