use anyhow::Result;
use clap::{Parser, Subcommand};
use pharmakon_common::{Config, SecretStore};
use pharmakon_core::agent::Agent;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::providers::registry::ModelRegistry;
use pharmakon_core::soul::Soul;
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
#[command(about = "💊 Pharmakon — Personal AI Engineering OS", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Message to send directly (positional shortcut for `pharmakon agent --message`)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    message: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive TUI chat (default command)
    #[command(alias = "chat")]
    Tui {
        #[arg(short, long)]
        soul: Option<String>,
        #[arg(short, long)]
        model: Option<String>,
        #[arg(short, long)]
        message: Option<String>,
    },
    /// One-shot agent query
    Agent {
        #[arg(short, long)]
        message: Option<String>,
        #[arg(long, default_value = "")]
        session: String,
        #[arg(long)]
        soul: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Start the gateway service
    #[command(alias = "server")]
    Gateway {
        #[command(subcommand)]
        subcommand: Option<GatewaySubcommands>,
        #[arg(short, long, default_value_t = 19999)]
        port: u16,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        soul: Option<String>,
    },
    /// Manage secrets
    Secrets {
        #[command(subcommand)]
        command: SecretsCommands,
    },
    /// Run system diagnostics
    Doctor {
        #[arg(short, long)]
        repair: bool,
    },
    /// Initial onboarding and setup
    Onboard,
    /// View trajectory / session history
    Trajectory {
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(short, long)]
        output: Option<String>,
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
    /// Manage background service
    Service {
        #[command(subcommand)]
        subcommand: ServiceCommands,
    },
    /// Run Ollama background distillation based on successful trajectories
    Distill {
        /// Base model to distill from (default: llama3.2)
        #[arg(long, default_value = "llama3.2")]
        base_model: String,

        /// Name of the compiled local model (default: pharmakon-distilled)
        #[arg(long, default_value = "pharmakon-distilled")]
        target_model: String,
    },
}

#[derive(Subcommand)]
pub enum PairingCommands {
    /// Approve a pairing request
    Approve { platform: String, code: String },
    /// List active pairings
    List,
    /// Revoke a pairing
    Revoke { platform: String, id: String },
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
pub enum ServiceCommands {
    /// Install as a background service
    Install { #[arg(short, long, default_value_t = 19999)] port: u16 },
    /// Stop the background service
    Stop,
    /// Check service status
    Status,
}

#[derive(Subcommand)]
enum SecretsCommands {
    Set { key: String, value: String },
    Get { key: String },
    List,
    Remove { key: String },
}

/// Build a fully-initialized Agent with soul, nexus, fact memory, and all tools.
async fn build_agent(
    config: &Config,
    session_store: Arc<DbSessionStore>,
    session_id: &str,
    soul_path: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<Agent> {
    let home = dirs::home_dir().expect("Could not find home directory");

    // Inject API keys
    let secret_store = SecretStore::new();
    for p in &["GEMINI", "OPENAI", "ANTHROPIC", "GROQ", "PERPLEXITY"] {
        let key_name = format!("{}_API_KEY", p);
        if let Ok(key) = secret_store.get_secret(&key_name) {
            unsafe { std::env::set_var(&key_name, key); }
        }
    }

    // Soul
    let soul_registry = SoulRegistry::new()?;
    let soul = if let Some(path) = soul_path {
        if Path::new(path).exists() {
            Soul::load_from_file(path)?
        } else {
            soul_registry.get_soul(path).cloned().unwrap_or_else(Soul::default_soul)
        }
    } else {
        soul_registry.get_soul("default")
            .or_else(|| {
                // Fallback: if no "default" soul, use the first available soul in the registry
                let available = soul_registry.list_souls();
                if !available.is_empty() {
                    log::info!("No 'default' soul found. Using '{}' as fallback.", available[0]);
                    soul_registry.get_soul(&available[0])
                } else {
                    None
                }
            })
            .cloned()
            .unwrap_or_else(Soul::default_soul)
    };

    // Model
    let actual_provider = provider.unwrap_or(&config.default_agent.provider);
    let actual_model = model.unwrap_or(&config.default_agent.model);
    let model_id = if actual_model.contains('/') {
        actual_model.to_string()
    } else {
        format!("{}/{}", actual_provider, actual_model)
    };
    let model_obj = ModelRegistry::get_model(&model_id)
        .unwrap_or_else(|| panic!("Model not available: {}. Check your API keys.", model_id));

    // Knowledge Nexus
    let pharmakon_dir = home.join(".pharmakon");
    let _ = std::fs::create_dir_all(&pharmakon_dir);
    let nexus_db_path = pharmakon_dir.join("knowledge_nexus");
    let graph_db_path = pharmakon_dir.join("knowledge_graph.db");
    let nexus = Arc::new(
        pharmakon_memory::weaver::KnowledgeNexus::new(
            nexus_db_path.to_str().unwrap(),
            graph_db_path.to_str().unwrap(),
        ).await?,
    );
    let fact_memory = Arc::new(Mutex::new(pharmakon_memory::fact_memory::BeliefSystem::new()?));

    let agent = Agent::new(model_obj, session_id.to_string())
        .with_store(session_store.clone())
        .with_knowledge_nexus(nexus)
        .with_fact_memory(fact_memory)
        .with_fallback_models(config.default_agent.fallback_models.clone());

    agent.set_soul(soul).await;
    pharmakon_core::tool_init::init_all_agent_tools(&agent).await?;

    Ok(agent)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("warn"));

    let config = Config::load().unwrap_or_else(|e| {
        log::error!("Error loading config: {}. Using default.", e);
        Config::default()
    });

    let cli = Cli::parse();

    // Database setup
    let home = dirs::home_dir().expect("Could not find home directory");
    let db_path = home.join(".pharmakon").join("pharmakon.db");
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db_url = if cfg!(windows) {
        format!("sqlite://{}", db_path.to_str().unwrap())
    } else {
        format!("sqlite:///{}", db_path.to_str().unwrap())
    };
    let session_store = Arc::new(DbSessionStore::new(&db_url).await?);

    // Handle bare positional message: `pharmakon "hello world"` → agent one-shot
    if !cli.message.is_empty() {
        let msg = cli.message.join(" ");
        let agent = build_agent(&config, session_store, "cli-oneshot", None, None, None).await?;
        let agent_arc = Arc::new(agent);
        match agent_arc.chat(&msg).await {
            Ok(response) => println!("{}", response),
            Err(e) => log::error!("Agent error: {}", e),
        }
        std::process::exit(0);
    }

    match cli.command {
        // Default: TUI (interactive chat)
        None | Some(Commands::Tui { .. }) => {
            let cmd = if let Some(Commands::Tui { soul, model, message }) = cli.command {
                (soul, model, message)
            } else {
                (None, None, None)
            };

            let agent = build_agent(
                &config, session_store, "tui-session",
                cmd.0.as_deref(), cmd.1.as_deref(), None,
            ).await?;
            let agent_arc = Arc::new(agent);

            // Try TUI; fall back to REPL
            if let Err(e) = tui::run_tui(agent_arc.clone(), cmd.2).await {
                log::warn!("TUI failed ({}), falling back to REPL.", e);
                tui::run_repl(agent_arc).await?;
            }
        }

        Some(Commands::Agent { message, session, soul, provider, model }) => {
            // Generate a unique session ID for one-shot invocations to prevent
            // cross-invocation message accumulation, unless user explicitly set one.
            let session_id = if session.is_empty() {
                format!("cli-{}", uuid::Uuid::new_v4().to_string().chars().take(8).collect::<String>())
            } else {
                session
            };

            let agent = build_agent(
                &config, session_store, &session_id,
                soul.as_deref(), provider.as_deref(), model.as_deref(),
            ).await?;
            let agent_arc = Arc::new(agent);

            if let Some(msg) = message {
                // One-shot mode
                match agent_arc.chat(&msg).await {
                    Ok(response) => println!("{}", response),
                    Err(e) => log::error!("Agent error: {}", e),
                }
                std::process::exit(0);
            } else {
                // Interactive REPL mode
                tui::run_repl(agent_arc).await?;
            }
        }

        Some(Commands::Gateway { subcommand, port, verbose: _, soul: soul_path }) => {
            if let Some(sub) = subcommand {
                match sub {
                    GatewaySubcommands::Stop => { service_installer::stop_service()?; return Ok(()); }
                    GatewaySubcommands::Status => { service_installer::get_service_status()?; return Ok(()); }
                    GatewaySubcommands::Install => { service_installer::install_service(port)?; return Ok(()); }
                    GatewaySubcommands::Start => { /* fall through */ }
                }
            }

            let agent = build_agent(
                &config, session_store.clone(), "gateway",
                soul_path.as_deref(), None, None,
            ).await?;
            let agent_arc = Arc::new(agent);
            let cron_manager = Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);

            let mut gateway = pharmakon_gateway::Gateway::new(port, agent_arc.clone(), cron_manager, config);

            // Channels
            let secret_store = SecretStore::new();
            if let Ok(token) = secret_store.get_secret("TELEGRAM_BOT_TOKEN") {
                log::info!("Activating Telegram channel...");
                gateway.add_channel(Arc::new(pharmakon_gateway::channels::telegram::TelegramChannel::new(token)));
            }
            if let Ok(token) = secret_store.get_secret("DISCORD_BOT_TOKEN") {
                log::info!("Activating Discord channel...");
                gateway.add_channel(Arc::new(pharmakon_gateway::channels::discord::DiscordChannel::new(token)));
            }

            // Heartbeat
            let heartbeat = pharmakon_core::automation::heartbeat::HeartbeatManager::new(agent_arc.clone(), 3600);
            heartbeat.start().await;

            gateway.run().await?;
        }

        Some(Commands::Onboard) => { wizard::run_wizard()?; }

        Some(Commands::Doctor { repair }) => {
            println!("🩺 Pharmakon System Diagnostics");
            println!("===============================");

            // Config
            println!("\n📋 Configuration:");
            println!("  Config path: ~/.pharmakon/config.json");
            println!("  Default provider: {}", config.default_agent.provider);
            println!("  Default model: {}", config.default_agent.model);
            println!("  Gateway port: {}", config.gateway.port);
            println!("  DM policy: {}", config.gateway.dm_policy);

            // Database
            println!("\n🗄 Database:");
            if db_path.exists() {
                let meta = std::fs::metadata(&db_path)?;
                println!("  SQLite DB: {} ({} KB)", db_path.display(), meta.len() / 1024);
                // List sessions
                let sessions = session_store.list_sessions().await.unwrap_or_default();
                println!("  Sessions: {}", sessions.len());
                for s in sessions.iter().take(10) {
                    println!("    - {}", s);
                }
                if sessions.len() > 10 {
                    println!("    ... and {} more", sessions.len() - 10);
                }
            } else {
                println!("  SQLite DB: not found (will be created on first run)");
            }

            // API Keys
            println!("\n🔑 API Keys:");
            let secret_store = SecretStore::new();
            for p in &["GEMINI", "OPENAI", "ANTHROPIC", "GROQ", "PERPLEXITY"] {
                let key_name = format!("{}_API_KEY", p);
                if let Ok(key) = secret_store.get_secret(&key_name) {
                    let masked = if key.len() > 8 {
                        format!("{}...{}", &key[..4], &key[key.len()-4..])
                    } else {
                        "****".to_string()
                    };
                    println!("  {}=✓ {}", key_name, masked);
                } else {
                    println!("  {}=✗ (not set)", key_name);
                }
            }

            // Tools
            println!("\n🔧 Tools:");
            let agent = build_agent(&config, session_store.clone(), "doctor", None, None, None).await?;
            let reg = agent.registry.lock().await;
            let tools = reg.all_metadata();
            println!("  Total tool metadata: {}", tools.len());
            let loaded = reg.get_loaded();
            println!("  Loaded tools: {}", loaded.len());

            // Memory
            println!("\n🧠 Knowledge Nexus:");
            let nexus_dir = home.join(".pharmakon").join("knowledge_nexus");
            if nexus_dir.exists() {
                println!("  Nexus DB: {}", nexus_dir.display());
            } else {
                println!("  Nexus DB: not yet created");
            }

            // Snapshots
            let snap_dir = home.join(".pharmakon").join("snapshots");
            if snap_dir.exists() {
                let count = std::fs::read_dir(&snap_dir)?.count();
                println!("\n📸 Snapshots: {} files in {}", count, snap_dir.display());
            }

            // Event Log
            let event_log_dir = home.join(".pharmakon").join("event_log");
            if event_log_dir.exists() {
                println!("\n📜 Event Log: {}", event_log_dir.display());
            }

            if repair {
                println!("\n🔧 Running repairs...");
                let _ = std::fs::create_dir_all(home.join(".pharmakon"));
                let _ = std::fs::create_dir_all(home.join(".pharmakon").join("snapshots"));
                let _ = std::fs::create_dir_all(home.join(".pharmakon").join("event_log"));
                let _ = std::fs::create_dir_all(home.join(".pharmakon").join("context"));
                let _ = std::fs::create_dir_all(home.join(".pharmakon").join("workspace"));
                println!("  Created missing directories.");

                // Clean up orphaned one-shot sessions
                match session_store.cleanup_orphan_sessions().await {
                    Ok(0) => println!("  No orphan sessions to clean."),
                    Ok(n) => println!("  Cleaned {} orphan session messages.", n),
                    Err(e) => log::warn!("  Orphan cleanup failed: {}", e),
                }
            }

            println!("\n✅ Diagnostics complete.");
        }

        Some(Commands::Trajectory { session, format, output }) => {
            let history = session_store.load_history(&session).await?;
            if history.is_empty() {
                println!("No history found for session: {}", session);
                return Ok(());
            }

            let output_str = match format.as_str() {
                "json" => serde_json::to_string_pretty(&history)?,
                "markdown" | "md" => {
                    let mut md = format!("# Session: {}\n\n", session);
                    for msg in &history {
                        let content = msg.content.as_ref().map(|c| c.to_string()).unwrap_or_default();
                        md.push_str(&format!("## {}\n\n{}\n\n", msg.role, content));
                    }
                    md
                }
                _ => {
                    // Plain text
                    let mut text = format!("Session: {}\n{}\n", session, "=".repeat(60));
                    for msg in &history {
                        let content = msg.content.as_ref().map(|c| c.to_string()).unwrap_or_default();
                        text.push_str(&format!("\n[{}]\n{}\n", msg.role.to_uppercase(), content));
                    }
                    text
                }
            };

            if let Some(path) = output {
                std::fs::write(&path, &output_str)?;
                println!("Trajectory written to {}", path);
            } else {
                println!("{}", output_str);
            }
        }

        Some(Commands::Secrets { command }) => {
            let store = SecretStore::new();
            match command {
                SecretsCommands::Set { key, value } => {
                    store.set_secret(&key, &value)?;
                    println!("✅ Secret '{}' stored.", key);
                }
                SecretsCommands::Get { key } => {
                    match store.get_secret(&key) {
                        Ok(val) => println!("{} = {}", key, val),
                        Err(_) => println!("❌ Secret '{}' not found.", key),
                    }
                }
                SecretsCommands::List => {
                    println!("Secrets are stored in OS keyring.");
                    for p in &["GEMINI_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GROQ_API_KEY", "PERPLEXITY_API_KEY", "DEEPSEEK_API_KEY", "TELEGRAM_BOT_TOKEN", "DISCORD_BOT_TOKEN"] {
                        if store.get_secret(p).is_ok() {
                            println!("  ✓ {}", p);
                        }
                    }
                }
                SecretsCommands::Remove { key } => {
                    // fallback file-based removal
                    let secrets_dir = home.join(".pharmakon");
                    let secrets_file = secrets_dir.join("secrets.json");
                    if secrets_file.exists() {
                        let content = std::fs::read_to_string(&secrets_file)?;
                        let mut map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)?;
                        map.remove(&key);
                        std::fs::write(&secrets_file, serde_json::to_string_pretty(&map)?)?;
                        println!("✅ Secret '{}' removed.", key);
                    } else {
                        println!("No secrets file found.");
                    }
                }
            }
        }

        Some(Commands::Pairing { subcommand }) => {
            let mgr = pharmakon_core::security::pairing::PairingManager::global();
            match subcommand {
                PairingCommands::Approve { platform, code } => {
                    match mgr.approve_pairing(&platform, &code) {
                        Ok(id) => println!("✅ Pairing approved for {}: {}", platform, id),
                        Err(e) => println!("❌ Failed: {}", e),
                    }
                }
                PairingCommands::List => {
                    let pairings = mgr.list_allowed();
                    println!("--- Active Pairings ---");
                    if pairings.is_empty() { println!("No active pairings."); }
                    else { for p in pairings { println!(" - {}", p); } }
                }
                PairingCommands::Revoke { platform, id } => {
                    match mgr.revoke_pairing(&platform, &id) {
                        Ok(_) => println!("✅ Pairing revoked for {}: {}", platform, id),
                        Err(e) => println!("❌ Failed: {}", e),
                    }
                }
            }
        }

        Some(Commands::Service { subcommand }) => {
            match subcommand {
                ServiceCommands::Install { port } => { service_installer::install_service(port)?; }
                ServiceCommands::Stop => { service_installer::stop_service()?; }
                ServiceCommands::Status => { service_installer::get_service_status()?; }
            }
        }

        Some(Commands::Distill { base_model, target_model }) => {
            println!("🧬 Starting manual Ollama Trajectory Distillation...");
            println!("  Base model: {}", base_model);
            println!("  Target model: {}", target_model);
            let distiller = pharmakon_core::orchestration::ollama_distiller::OllamaDistiller::new(session_store);
            match distiller.distill(&base_model, &target_model).await {
                Ok(name) => println!("🎉 Distillation successful! Model '{}' compiled and registered inside Ollama.", name),
                Err(e) => println!("❌ Distillation failed: {}", e),
            }
        }

        Some(Commands::Gui { soul, model }) => {
            let agent = build_agent(
                &config, session_store.clone(), "gui-session",
                soul.as_deref(), None, model.as_deref(),
            ).await?;
            let agent_arc = Arc::new(agent);

            let cron_manager = Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);

            println!("💊 Launching Pharmakon Desktop...");
            if let Err(e) = pharmakon_gateway::ui::run_app(agent_arc, session_store, cron_manager) {
                log::error!("Desktop app error: {}", e);
                println!("Falling back to web UI at http://localhost:19999");
                let _ = open::that("http://localhost:19999");
            }
        }

        Some(Commands::Ui { port }) => {
            let url = format!("http://localhost:{}", port);
            println!("Opening Pharmakon Interface at {}...", url);
            let _ = open::that(url);
        }
    }

    Ok(())
}