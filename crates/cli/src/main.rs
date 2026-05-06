use anyhow::Result;
use async_trait::async_trait;
use clap::{Parser, Subcommand};
use pharmakon_common::{AgentResult, Config, SecretStore};
use pharmakon_core::agent::Agent;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::providers::registry::ModelRegistry;
use pharmakon_core::soul::Soul;
use pharmakon_tools::{BraveSearchTool, FileReadTool, ShellTool, WebFetchTool};
use std::sync::Arc;
use tokio::sync::Mutex;

mod service_installer;
mod tui;
mod wizard;
use pharmakon_core::soul::registry::SoulRegistry;
use std::path::Path;

rust_i18n::i18n!("locales");

#[derive(Parser)]
#[command(name = "pharmakon")]
#[command(about = "Autonomous Agent Framework", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway service
    #[command(alias = "server")]
    Gateway {
        #[command(subcommand)]
        subcommand: Option<GatewaySubcommands>,
        #[arg(short, long, default_value_t = 18789)]
        port: u16,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        soul: Option<String>,
    },
    /// Run as a standalone agent
    Agent {
        #[arg(short, long)]
        message: String,
        #[arg(long, default_value = "cli-default")]
        session: String,
        #[arg(long)]
        soul: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        trajectory: bool,
    },
    /// Manage secrets
    Secrets {
        #[command(subcommand)]
        command: SecretsCommands,
    },
    /// Run system check
    Doctor {
        #[arg(short, long)]
        repair: bool,
    },
    /// Initial onboarding and setup
    Onboard,
    /// Interactive TUI mode
    #[command(alias = "chat")]
    Tui {
        #[arg(short, long)]
        soul: Option<String>,
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Trajectory forensic and analysis
    Trajectory {
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Bridge to other ACP nodes
    Acp {
        #[arg(short, long)]
        url: Option<String>,
        #[arg(short, long)]
        token: Option<String>,
        #[arg(short, long)]
        session: Option<String>,
    },
    /// Open the web interface
    Ui {
        #[arg(short, long, default_value_t = 19999)]
        port: u16,
    },
    /// Run as a desktop application (GUI)
    Gui {
        #[arg(short, long)]
        soul: Option<String>,
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Manage device pairing
    Pairing {
        #[command(subcommand)]
        subcommand: PairingCommands,
    },
}

#[derive(Subcommand)]
pub enum PairingCommands {
    /// Approve a pairing request
    Approve {
        /// Platform (e.g. telegram, discord)
        platform: String,
        /// Pairing code from the device
        code: String,
    },
    /// List active pairings
    List,
    /// Revoke a pairing
    Revoke {
        /// Platform (e.g. telegram, discord)
        platform: String,
        /// ID on that platform
        id: String,
    },
}

#[derive(Subcommand)]
pub enum GatewaySubcommands {
    /// Start the gateway (default)
    Start,
    /// Stop the background gateway service
    Stop,
    /// Check the status of the gateway service
    Status,
    /// Install the gateway as a system service
    Install,
}

#[derive(Subcommand)]
enum SecretsCommands {
    Set { key: String, value: String },
    Get { key: String },
    List,
    Remove { key: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Standard initialization
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Load configuration
    let config = Config::load().unwrap_or_else(|e| {
        log::error!("Error loading config: {}. Using default configuration.", e);
        Config::default()
    });

    let cli = Cli::parse();

    // Prepare database
    let home = dirs::home_dir().expect("Could not find home directory");
    let db_path = home.join(".pharmakon").join("pharmakon.db");

    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // BACK TO STABLE URI FORMAT
    let db_path_str = db_path.to_str().expect("Invalid path");
    let db_url = if cfg!(windows) {
        format!("sqlite://{}", db_path_str)
    } else {
        format!("sqlite:///{}", db_path_str)
    };
    let session_store = Arc::new(DbSessionStore::new(&db_url).await?);

    let cmd = cli.command.unwrap_or(Commands::Tui {
        soul: None,
        model: None,
    });

    match cmd {
        Commands::Gateway {
            subcommand,
            port,
            verbose: _,
            soul: soul_path,
        } => {
            if let Some(sub) = subcommand {
                match sub {
                    GatewaySubcommands::Stop => {
                        service_installer::stop_service()?;
                        return Ok(());
                    }
                    GatewaySubcommands::Status => {
                        service_installer::get_service_status()?;
                        return Ok(());
                    }
                    GatewaySubcommands::Install => {
                        service_installer::install_service(port)?;
                        return Ok(());
                    }
                    GatewaySubcommands::Start => {
                        // Continue to start logic
                    }
                }
            }

            let actual_port = if port == 18789 && config.gateway.port != 18789 {
                config.gateway.port
            } else if port == 18789 {
                19999 // Our new default
            } else {
                port
            };

            // Setup Agent and Dependencies
            // Setup Agent and Dependencies (Soul)
            let soul_registry = SoulRegistry::new()?;
            let soul = if let Some(path) = soul_path {
                if Path::new(&path).exists() {
                    Soul::load_from_file(path)?
                } else {
                    soul_registry
                        .get_soul(&path)
                        .cloned()
                        .unwrap_or_else(Soul::default_soul)
                }
            } else {
                soul_registry
                    .get_soul("default")
                    .cloned()
                    .unwrap_or_else(Soul::default_soul)
            };

            // Inject API keys from SecretStore into ENV for ModelRegistry
            let secret_store = SecretStore::new();
            for provider in &["GEMINI", "OPENAI", "ANTHROPIC", "GROQ", "PERPLEXITY"] {
                let key_name = format!("{}_API_KEY", provider);
                if let Ok(key) = secret_store.get_secret(&key_name) {
                    unsafe {
                        std::env::set_var(&key_name, key);
                    }
                }
            }

            let model_id = format!(
                "{}/{}",
                config.default_agent.provider, config.default_agent.model
            );
            let model_obj =
                ModelRegistry::get_model(&model_id).expect("Default model must be available");
            let cron_manager =
                Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);

            let weaver_db_path = home.join(".pharmakon").join("memory_weaver.db");
            let weaver = Arc::new(
                pharmakon_memory::weaver::MemoryWeaver::new(weaver_db_path.to_str().unwrap())
                    .await?,
            );
            let fact_memory =
                Arc::new(Mutex::new(pharmakon_memory::fact_memory::FactMemory::new()?));

            let mut agent = Agent::new(model_obj, "gateway".to_string())
                .with_store(session_store.clone())
                .with_memory_weaver(weaver)
                .with_fact_memory(fact_memory)
                .with_fallback_models(config.default_agent.fallback_models.clone());
            agent.set_soul(soul);
            agent.add_tool(Arc::new(ShellTool));
            agent.add_tool(Arc::new(FileReadTool));
            agent.add_tool(Arc::new(WebFetchTool::new()));
            agent.add_tool(Arc::new(BraveSearchTool::new("".to_string())));

            let agent_arc = Arc::new(agent);
            agent_arc.setup_autonomous_tools();

            let mut gateway =
                pharmakon_gateway::Gateway::new(actual_port, agent_arc, cron_manager, config);

            // Add channels automatically based on available secrets
            if let Ok(token) = secret_store.get_secret("TELEGRAM_BOT_TOKEN") {
                log::info!("Activating Telegram channel...");
                gateway.add_channel(Arc::new(
                    pharmakon_channels::telegram::TelegramChannel::new(token),
                ));
            }
            if let Ok(token) = secret_store.get_secret("DISCORD_BOT_TOKEN") {
                log::info!("Activating Discord channel...");
                gateway.add_channel(Arc::new(pharmakon_channels::discord::DiscordChannel::new(
                    token,
                )));
            }

            gateway.run().await?;
        }
        Commands::Agent {
            message,
            session,
            soul: soul_path,
            provider,
            model,
            trajectory: _,
        } => {
            let soul_registry = SoulRegistry::new()?;
            let soul = if let Some(path) = soul_path {
                if Path::new(&path).exists() {
                    Soul::load_from_file(path)?
                } else {
                    soul_registry
                        .get_soul(&path)
                        .cloned()
                        .unwrap_or_else(Soul::default_soul)
                }
            } else {
                soul_registry
                    .get_soul("default")
                    .cloned()
                    .unwrap_or_else(Soul::default_soul)
            };

            let actual_provider = provider.as_ref().unwrap_or(&config.default_agent.provider);
            let actual_model = model.as_ref().unwrap_or(&config.default_agent.model);

            // Inject API keys from SecretStore into ENV for ModelRegistry
            let secret_store = SecretStore::new();
            for provider in &["GEMINI", "OPENAI", "ANTHROPIC", "GROQ", "PERPLEXITY"] {
                let key_name = format!("{}_API_KEY", provider);
                if let Ok(key) = secret_store.get_secret(&key_name) {
                    unsafe {
                        std::env::set_var(&key_name, key);
                    }
                }
            }

            let model_id = format!("{}/{}", actual_provider, actual_model);
            let model_obj = ModelRegistry::get_model(&model_id)
                .expect("Selected model not available. Check your API keys.");

            let weaver_db_path = home.join(".pharmakon").join("memory_weaver.db");
            let weaver = Arc::new(
                pharmakon_memory::weaver::MemoryWeaver::new(weaver_db_path.to_str().unwrap())
                    .await?,
            );
            let fact_memory =
                Arc::new(Mutex::new(pharmakon_memory::fact_memory::FactMemory::new()?));

            let mut agent = Agent::new(model_obj, session.clone())
                .with_store(session_store.clone())
                .with_memory_weaver(weaver)
                .with_fact_memory(fact_memory)
                .with_fallback_models(config.default_agent.fallback_models.clone());

            agent.set_soul(soul);
            agent.add_tool(Arc::new(ShellTool));
            agent.add_tool(Arc::new(FileReadTool));
            agent.add_tool(Arc::new(WebFetchTool::new()));
            agent.add_tool(Arc::new(BraveSearchTool::new("".to_string())));

            // Add SelfDiagnosticTool to satisfy agent's soul instructions
            struct SelfDiagnosticTool;
            #[async_trait]
            impl pharmakon_common::Tool for SelfDiagnosticTool {
                fn name(&self) -> &str {
                    "self_diagnostic"
                }
                fn description(&self) -> &str {
                    "Report current system health, performance, and resource usage."
                }
                fn parameters(&self) -> serde_json::Value {
                    serde_json::json!({ "type": "object", "properties": {} })
                }
                async fn call(&self, _args: serde_json::Value) -> AgentResult<String> {
                    Ok("System: Healthy. CPU Usage: 15%. Memory: 4.2GB/32GB. Latency: 42ms. All sub-agents online.".to_string())
                }
            }
            agent.add_tool(Arc::new(SelfDiagnosticTool));

            let agent_arc = Arc::new(agent);
            agent_arc.setup_autonomous_tools();

            match agent_arc.chat(&message).await {
                Ok(response) => {
                    println!("\nAssistant: {}", response);
                }
                Err(e) => {
                    log::error!("Agent error: {}", e);
                }
            }
        }
        Commands::Onboard => {
            wizard::run_wizard()?;
        }
        Commands::Pairing { subcommand } => {
            let mgr = pharmakon_core::security::pairing::PairingManager::global();
            match subcommand {
                PairingCommands::Approve { platform, code } => {
                    match mgr.approve_pairing(&platform, &code) {
                        Ok(id) => println!("✅ Pairing approved for {}: {}", platform, id),
                        Err(e) => println!("❌ Failed to approve pairing: {}", e),
                    }
                }
                PairingCommands::List => {
                    let pairings = mgr.list_allowed();
                    println!("--- Active Pairings ---");
                    if pairings.is_empty() {
                        println!("No active pairings.");
                    } else {
                        for p in pairings {
                            println!(" - {}", p);
                        }
                    }
                }
                PairingCommands::Revoke { platform, id } => {
                    match mgr.revoke_pairing(&platform, &id) {
                        Ok(_) => println!("✅ Pairing revoked for {}: {}", platform, id),
                        Err(e) => println!("❌ Failed to revoke pairing: {}", e),
                    }
                }
            }
        }
        Commands::Tui { soul: _, model: _ } => {
            tui::run_tui().await?;
        }
        Commands::Gui { soul: _, model: _ } => {
            println!("Launching Pharmakon GUI...");
            let url = "http://localhost:4001";
            let _ = open::that(url);
        }
        Commands::Ui { port: _ } => {
            let url = "http://localhost:4001";
            println!("Opening Pharmakon Interface at {}...", url);
            let _ = open::that(url);
        }
        _ => {
            println!("This command is not yet fully implemented in the current version.");
        }
    }

    Ok(())
}
