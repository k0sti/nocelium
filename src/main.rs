use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

use nocelium_channels::Channel;
use nocelium_channels::stdio::StdioChannel;
use nocelium_core::{Config, Dispatcher, Identity};

#[derive(Parser)]
#[command(name = "nocelium", about = "Nostr-native AI agent runtime")]
struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the agent interactively
    Agent,

    /// Manage systemd user service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Manage Nostr identity
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install systemd user service
    Install,
    /// Start the service
    Start,
    /// Stop the service
    Stop,
    /// Show service status
    Status,
    /// Follow service logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
    /// Uninstall the service
    Uninstall,
}

#[derive(Subcommand)]
enum IdentityAction {
    /// Show identity (npub)
    Show,
    /// Generate a new identity
    Generate,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show resolved configuration
    Show,
    /// Migrate TOML config to Nomen
    Migrate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // No subcommand → show help
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            return Ok(());
        }
    };

    // Init logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nocelium=info".parse()?)
                .add_directive("nocelium_core=info".parse()?)
                .add_directive("nocelium_tools=info".parse()?)
                .add_directive("nocelium_memory=info".parse()?)
                .add_directive("nocelium_channels=info".parse()?)
        )
        .init();

    match command {
        Command::Agent => run_agent(&cli.config).await,
        Command::Service { action } => handle_service(&action, &cli.config).await,
        Command::Identity { action } => handle_identity(&action, &cli.config),
        Command::Config { action } => handle_config(&action, &cli.config).await,
    }
}

async fn run_agent(config_path: &Option<PathBuf>) -> Result<()> {
    let config = load_config(config_path)?;
    let identity = Identity::load_or_generate(&config.identity)?;

    println!("Nocelium v{}", env!("CARGO_PKG_VERSION"));
    println!("Identity: {}", identity.npub());
    println!("Provider: {} ({})", config.provider.provider_type, config.provider.model);
    println!();

    let memory = if config.memory.enabled {
        let client = nocelium_memory::MemoryClient::new(&config.memory.socket_path, 3);
        println!("Memory: enabled ({})", config.memory.socket_path);
        Some(Arc::new(client))
    } else {
        println!("Memory: disabled");
        None
    };

    let agent = nocelium_core::agent::build_agent(&config, &identity, memory.clone())?;

    // Event queue
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Build channel map
    let mut channels: HashMap<String, Arc<dyn Channel>> = HashMap::new();

    // Stdio channel (only when interactive)
    let is_interactive = std::io::stdin().is_terminal();
    if is_interactive {
        let stdio: Arc<dyn Channel> = Arc::new(StdioChannel::new());
        channels.insert("stdio".into(), Arc::clone(&stdio));
        println!("Channel: stdio (interactive)");
        println!("Type /quit to exit\n");
    }

    // Telegram channel (if enabled)
    #[cfg(feature = "telegram")]
    if let Some(ref tg_config) = config.channels.telegram {
        if tg_config.enabled {
            let token = tg_config
                .token
                .clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Telegram enabled but no token. Set TELEGRAM_BOT_TOKEN or channels.telegram.token"
                    )
                })?;

            let tg_channel: Arc<dyn Channel> =
                Arc::new(nocelium_channels::telegram::TelegramChannel::new(&token));
            channels.insert("telegram".into(), Arc::clone(&tg_channel));

            let tg_tx = tx.clone();
            let tg = Arc::clone(&tg_channel);
            tokio::spawn(async move {
                if let Err(e) = tg.listen(tg_tx).await {
                    tracing::error!(error = %e, "Telegram listener failed");
                }
            });
            println!("Channel: telegram");
        }
    }

    // Check we have at least one channel
    if channels.is_empty() {
        anyhow::bail!("No channels available. Enable Telegram or run interactively.");
    }

    // Spawn listeners for all channels
    for (name, channel) in &channels {
        if name == "telegram" {
            continue; // already spawned above
        }
        let ch_tx = tx.clone();
        let ch = Arc::clone(channel);
        tokio::spawn(async move {
            if let Err(e) = ch.listen(ch_tx).await {
                tracing::error!(error = %e, "Channel listener failed");
            }
        });
    }

    drop(tx);

    let dispatcher = Dispatcher::default_agent_turn();

    nocelium_core::agent::run_loop(
        &agent,
        &mut rx,
        &channels,
        &dispatcher,
        memory.as_deref(),
    )
    .await?;

    Ok(())
}

fn handle_identity(action: &IdentityAction, config_path: &Option<PathBuf>) -> Result<()> {
    let config = load_config(config_path)?;

    match action {
        IdentityAction::Show => {
            let identity = Identity::load_or_generate(&config.identity)?;
            println!("npub: {}", identity.npub());
        }
        IdentityAction::Generate => {
            let identity = Identity::load_or_generate(&config.identity)?;
            println!("Identity generated: {}", identity.npub());
        }
    }

    Ok(())
}

async fn handle_config(action: &ConfigAction, config_path: &Option<PathBuf>) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let config = load_config(config_path)?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigAction::Migrate => {
            println!("Config migration to Nomen is not yet implemented.");
            println!("This will read your TOML config and store it as Nomen memories.");
        }
    }

    Ok(())
}

fn load_config(config_path: &Option<PathBuf>) -> Result<Config> {
    match config_path {
        Some(path) => Config::load(path),
        None => Config::load_default(),
    }
}

// --- Service management ---

const SERVICE_NAME: &str = "nocelium";

async fn handle_service(action: &ServiceAction, config_path: &Option<PathBuf>) -> Result<()> {
    let service_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?
        .join("systemd/user");
    let service_file = service_dir.join(format!("{SERVICE_NAME}.service"));

    match action {
        ServiceAction::Install => {
            let exe = std::env::current_exe()?;
            let exe_path = exe.display();

            let config_arg = if let Some(p) = config_path {
                format!(" --config {}", p.canonicalize()?.display())
            } else {
                let default_paths = [
                    PathBuf::from("./nocelium.toml"),
                    dirs::config_dir()
                        .map(|d| d.join("nocelium/config.toml"))
                        .unwrap_or_default(),
                    dirs::home_dir()
                        .map(|d| d.join(".config/nocelium/config.toml"))
                        .unwrap_or_default(),
                ];
                match default_paths.iter().find(|p| p.exists()) {
                    Some(p) => format!(" --config {}", p.canonicalize()?.display()),
                    None => String::new(),
                }
            };

            let unit = format!(
                r#"[Unit]
Description=Nocelium AI Agent
After=network.target nomen.service

[Service]
Type=simple
ExecStart={exe_path}{config_arg} agent
Restart=always
RestartSec=5
Environment=RUST_LOG=nocelium=info

[Install]
WantedBy=default.target
"#
            );

            std::fs::create_dir_all(&service_dir)?;
            std::fs::write(&service_file, unit)?;
            run_systemctl(&["daemon-reload"])?;

            println!("Service installed: {}", service_file.display());
            println!();
            println!("Set environment variables:");
            println!("  systemctl --user edit {SERVICE_NAME}");
            println!("  # Add under [Service]:");
            println!("  # Environment=OPENROUTER_API_KEY=sk-...");
            println!("  # Environment=TELEGRAM_BOT_TOKEN=...");
            println!();
            println!("Then: nocelium service start");
        }

        ServiceAction::Start => {
            run_systemctl(&["enable", "--now", SERVICE_NAME])?;
            println!("Service started");
        }

        ServiceAction::Stop => {
            run_systemctl(&["stop", SERVICE_NAME])?;
            println!("Service stopped");
        }

        ServiceAction::Status => {
            let _ = run_systemctl(&["status", SERVICE_NAME]);
        }

        ServiceAction::Logs { follow } => {
            let mut args = vec!["--user", "-u", SERVICE_NAME];
            if *follow {
                args.push("-f");
            }
            let status = std::process::Command::new("journalctl")
                .args(&args)
                .status()?;
            if !status.success() {
                anyhow::bail!("journalctl failed");
            }
        }

        ServiceAction::Uninstall => {
            let _ = run_systemctl(&["disable", "--now", SERVICE_NAME]);
            if service_file.exists() {
                std::fs::remove_file(&service_file)?;
                run_systemctl(&["daemon-reload"])?;
                println!("Service uninstalled");
            } else {
                println!("Service not installed");
            }
        }
    }

    Ok(())
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if !status.success() {
        anyhow::bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}
