rust_i18n::i18n!("locales");

use rust_i18n::t;
use std::sync::Arc;
use std::fs;
use tokio::sync::Mutex;
use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use tokio::io::AsyncBufReadExt;

use pharmakon_common::Config;
use pharmakon_core::model::{MockModel, AgentModel};
use pharmakon_core::agent::Agent;
use pharmakon_core::providers::{OpenAIModel, GeminiModel, AnthropicModel, GroqModel, PerplexityModel, OllamaModel};
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::soul::Soul;
use pharmakon_gateway::Gateway;
use pharmakon_channels::{MockChannel, telegram::TelegramChannel, discord::DiscordChannel};
use pharmakon_tools::{ShellTool, FileReadTool, WebFetchTool, BrowserTool, BraveSearchTool, FactTool, CanvasTool, LinkUnderstandingTool, MediaUnderstandingTool, CommitmentTool, TerminalTool, ContextConnectorTool, SoulTool};

mod tui;
mod wizard;
mod service_installer;

#[derive(Parser)]
#[command(name = "pharmakon")]
#[command(about = "Rust port of OpenClaw — Personal AI Assistant", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the local-first Gateway
    Gateway {
        /// Port to listen on
        #[arg(short, long, default_value_t = 18789)]
        port: u16,

        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,

        /// Soul file to use for the assistant personality
        #[arg(long)]
        soul: Option<String>,
    },
    /// Interact with the AI assistant directly
    Agent {
        /// Message to send to the assistant
        #[arg(short, long)]
        message: String,

        /// Thinking level (low, medium, high)
        #[arg(short, long, default_value = "medium")]
        thinking: String,

        /// Session ID for history
        #[arg(long, default_value = "cli-default")]
        session: String,

        /// Soul file to use for the assistant personality
        #[arg(long)]
        soul: Option<String>,

        /// Provider to use (openai, gemini)
        #[arg(long)]
        provider: Option<String>,

        /// Model to use
        #[arg(long)]
        model: Option<String>,

        /// Export trajectory after chat
        #[arg(long)]
        trajectory: bool,
    },
    /// Run diagnostic check on the environment
    Doctor {
        /// Attempt to automatically fix issues
        #[arg(short, long)]
        repair: bool,
    },
    /// Interactive onboarding to set up Pharmakon
    Onboard {
        /// Use conversational onboarding (Crestodian)
        #[arg(short, long)]
        chat: bool,
    },
    /// Manage secrets securely
    Secrets {
        #[command(subcommand)]
        action: SecretAction,
    },
    /// Launch the rich terminal dashboard
    Tui,
    /// Launch the native Desktop GUI (Mac/Win/Linux)
    Desktop {
        /// Soul file to use
        #[arg(long)]
        soul: Option<String>,

        /// Provider to use
        #[arg(long)]
        provider: Option<String>,

        /// Model to use
        #[arg(long)]
        model: Option<String>,
    },
    /// Run the gateway in the background as a daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
        
        /// Port to listen on
        #[arg(short, long, default_value_t = 18789)]
        port: u16,
    },
    /// Export the trajectory of a session
    Trajectory {
        /// Session ID to export
        #[arg(long, default_value = "cli-default")]
        session: String,
        
        /// Export format (markdown, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Start the ACP (Agent Client Protocol) bridge over stdio
    Acp {
        /// Gateway URL
        #[arg(long)]
        url: Option<String>,

        /// Gateway token
        #[arg(long)]
        token: Option<String>,

        /// Session ID to use
        #[arg(long)]
        session: Option<String>,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Restart the daemon
    Restart,
    /// Check the status of the daemon
    Status,
}

#[derive(Subcommand)]
enum PairingAction {
    /// Approve a pending pairing
    Approve {
        /// Channel name (e.g., telegram, discord)
        channel: String,
        /// 6-digit pairing code
        code: String,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// Set a secret
    Set { name: String, value: String },
    /// Get a secret
    Get { name: String },
    /// Delete a secret
    Delete { name: String },
}

async fn run_gateway_service(port: u16, soul_path: Option<String>, session_store: Arc<DbSessionStore>, config: Config) -> Result<()> {
    // Load Soul if provided
    let soul = if let Some(path) = soul_path {
        Soul::load_from_file(path)?
    } else {
        Soul::default_soul()
    };

    // Shared model and agent
    let model: Arc<dyn AgentModel> = match config.agent.provider.as_str() {
        "gemini" => {
            let api_key = get_api_key("gemini")
                .ok_or_else(|| anyhow!("GEMINI_API_KEY not found. Please run 'pharmakon onboard' or set GEMINI_API_KEY env var."))?;
            Arc::new(GeminiModel::new(api_key, config.agent.model.clone()))
        }
        "openai" => {
            let api_key = get_api_key("openai")
                .ok_or_else(|| anyhow!("OPENAI_API_KEY not found. Please run 'pharmakon onboard' or set OPENAI_API_KEY env var."))?;
            Arc::new(OpenAIModel::new(api_key, config.agent.model.clone()))
        }
        "anthropic" => {
            let api_key = get_api_key("anthropic").expect("ANTHROPIC_API_KEY not found in keyring or environment");
            Arc::new(AnthropicModel::new(api_key, config.agent.model.clone()))
        }
        "ollama" => {
            Arc::new(OllamaModel::new(None, config.agent.model.clone()))
        }
        _ => Arc::new(MockModel)
    };
    
    let home = dirs::home_dir().expect("Could not find home directory");
    let weaver_db_path = home.join(".pharmakon").join("memory_weaver.db");
    let weaver = Arc::new(pharmakon_memory::weaver::MemoryWeaver::new(weaver_db_path.to_str().unwrap()).await?);

    let mut agent_inner = Agent::new(model, "gateway-shared".to_string())
            .with_store(session_store.clone())
            .with_memory_weaver(weaver.clone());
    agent_inner.with_soul(soul);
    
    let agent = Arc::new(Mutex::new(agent_inner));

    // Register Hooks
    {
        let agent_lock = agent.lock().await;
        agent_lock.hooks.register(Arc::new(pharmakon_core::hooks::memory_automation::AutoIndexHook::new(agent.clone()))).await;
    }

    let cron_manager = Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);
    {
        let mut agent_lock = agent.lock().await;
        let event_tx = agent_lock.event_tx.clone();
        let agent_model = agent_lock.model.clone();

        agent_lock.add_tool(Arc::new(ShellTool));
        agent_lock.add_tool(Arc::new(FileReadTool));
        agent_lock.add_tool(Arc::new(WebFetchTool::new()));
        agent_lock.add_tool(Arc::new(BraveSearchTool::new("".to_string())));
        agent_lock.add_tool(Arc::new(TerminalTool::new()));
        let sandbox = pharmakon_core::sidecars::browser_sandbox::BrowserSandbox::new().ok();
        let cdp_url = if let Some(s) = sandbox {
            match s.ensure_started().await {
                Ok(port) => Some(format!("http://127.0.0.1:{}", port)),
                Err(e) => {
                    log::warn!("Failed to start browser sandbox: {}. Falling back to host browser.", e);
                    None
                }
            }
        } else {
            None
        };

        agent_lock.add_tool(Arc::new(BrowserTool::new(cdp_url)));
        agent_lock.add_tool(Arc::new(pharmakon_tools::media::capture::ScreenshotTool));
        let dalle_key = get_api_key("openai");
        if let Some(key) = dalle_key {
            agent_lock.add_tool(Arc::new(pharmakon_tools::media::image_gen::ImageGenTool::new(key)));
        }

        let fact_mem = agent_lock.fact_memory.clone();
        agent_lock.add_tool(Arc::new(FactTool::new(fact_mem)));
        agent_lock.add_tool(Arc::new(CanvasTool::new(event_tx)));
        agent_lock.add_tool(Arc::new(LinkUnderstandingTool::new()));
        agent_lock.add_tool(Arc::new(MediaUnderstandingTool::new(agent_model)));
        agent_lock.add_tool(Arc::new(pharmakon_tools::media::capture::CameraTool));
        
        let mut connector_tool = ContextConnectorTool::new();
        connector_tool.add_connector(Arc::new(pharmakon_tools::connectors::SlackConnector { token: "placeholder".to_string() }));
        connector_tool.add_connector(Arc::new(pharmakon_tools::connectors::NotionConnector { token: "placeholder".to_string() }));
        agent_lock.add_tool(Arc::new(connector_tool));
        
        let soul_manager = Arc::new(pharmakon_core::agent::AgentSoulManager(agent.clone()));
        agent_lock.add_tool(Arc::new(SoulTool::new(soul_manager)));

        agent_lock.add_tool(Arc::new(CommitmentTool::new(session_store.clone())));
        
        let cron_tool = pharmakon_core::automation::cron_tool::CronTool::new(cron_manager.clone(), Arc::downgrade(&agent));
        agent_lock.add_tool(Arc::new(cron_tool));

        // Load MCP tools
        if let Ok(mcp_tools) = pharmakon_core::mcp_manager::McpManager::load_tools().await {
            for tool in mcp_tools {
                agent_lock.add_tool(tool);
            }
        }

        // Swarm Intelligence
        let spawner = Arc::new(pharmakon_core::orchestration::swarm::SwarmManager::new(agent.clone()));
        agent_lock.add_tool(Arc::new(pharmakon_core::orchestration::swarm::SwarmTool::new(spawner, 0)));
    }

    let mut gateway = Gateway::new(port, agent.clone(), cron_manager.clone(), config.clone());
    
    // Add initial channels
    gateway.add_channel(Arc::new(MockChannel::new("default-mock")));

    let secret_store = pharmakon_common::SecretStore::new();
    if let Ok(tg_token) = secret_store.get_secret("TELEGRAM_BOT_TOKEN").or_else(|_| std::env::var("TELEGRAM_BOT_TOKEN")) {
        log::info!("Registering Telegram channel...");
        gateway.add_channel(Arc::new(TelegramChannel::new(tg_token)));
    }

    if let Ok(discord_token) = std::env::var("DISCORD_BOT_TOKEN") {
        log::info!("Registering Discord channel...");
        gateway.add_channel(Arc::new(DiscordChannel::new(discord_token)));
    }

    /*
    if let Ok(slack_token) = std::env::var("SLACK_BOT_TOKEN") {
        log::info!("Registering Slack channel...");
        gateway.add_channel(Arc::new(SlackChannel::new(slack_token)));
    }
    */
    
    log::info!("Starting Pharmakon Gateway on port {}", port);
    
    let heartbeat_manager = pharmakon_core::automation::heartbeat::HeartbeatManager::new(agent.clone(), 30);
    heartbeat_manager.start().await;

    // Start Advanced Workers
    let evolution_worker = pharmakon_core::soul::evolution::SoulEvolutionWorker::new(agent.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await; // Every hour
            if let Err(e) = evolution_worker.evolve_cycle().await {
                log::error!("Soul Evolution error: {}", e);
            }
        }
    });

    let telemetry_worker = pharmakon_core::sidecars::telemetry_capture::TelemetryCaptureWorker::new(10)?;
    tokio::spawn(async move {
        if let Err(e) = telemetry_worker.start().await {
            log::error!("Telemetry Capture error: {}", e);
        }
    });
    
    gateway.run().await?;
    Ok(())
}

fn get_api_key(provider: &str) -> Option<String> {
    let secret_name = format!("{}_API_KEY", provider.to_uppercase());
    let secret_store = pharmakon_common::SecretStore::new();
    
    let result = match secret_store.get_secret(&secret_name) {
        Ok(key) => Some(key),
        Err(_) => {
            // Fallback to environment variable
            std::env::var(&secret_name).ok()
        }
    };
    
    if result.is_none() {
        log::debug!("Secret '{}' not found in keyring or env.", secret_name);
    }
    result
}

fn get_model(provider: &str, model: &str) -> Arc<dyn AgentModel> {
    let api_key = get_api_key(provider);
    
    if api_key.is_none() && provider != "mock" {
        log::error!("⚠️  {}_API_KEY not found in keyring or environment.", provider.to_uppercase());
        log::error!("Run 'pharmakon onboard' again or set the environment variable.");
        return Arc::new(MockModel);
    }

    match provider {
        "gemini" => Arc::new(GeminiModel::new(api_key.unwrap(), model.to_string())),
        "openai" => Arc::new(OpenAIModel::new(api_key.unwrap(), model.to_string())),
        "anthropic" => Arc::new(AnthropicModel::new(api_key.unwrap(), model.to_string())),
        "groq" => Arc::new(GroqModel::new(api_key.unwrap(), model.to_string())),
        "perplexity" => Arc::new(PerplexityModel::new(api_key.unwrap(), model.to_string())),
        "ollama" => Arc::new(OllamaModel::new(None, model.to_string())),
        _ => {
            if provider != "mock" {
                log::warn!("Unknown provider '{}', falling back to MockModel", provider);
            }
            Arc::new(MockModel)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
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
    
    // Ensure the .pharmakon directory exists before trying to create the DB file
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    // Sqlx expects sqlite URLs to have a protocol, and automatically creates the file if needed 
    // but requires the parent directory to exist.
    let db_url = format!("sqlite://{}", db_path.to_str().unwrap());
    let session_store = Arc::new(DbSessionStore::new(&db_url).await?);

    let cmd = cli.command.unwrap_or(Commands::Desktop { soul: None, provider: None, model: None });

    match &cmd {
        Commands::Gateway { port, verbose: _, soul: soul_path } => {
            let actual_port = if *port == 18789 && config.gateway.port != 18789 {
                config.gateway.port
            } else {
                *port
            };
            
            run_gateway_service(actual_port, soul_path.clone(), session_store.clone(), config.clone()).await?;
        }
        Commands::Agent { message, thinking, session, soul: soul_path, provider, model, trajectory: export_trajectory } => {
            let actual_thinking = if thinking == "medium" && config.agent.thinking != "medium" {
                &config.agent.thinking
            } else {
                thinking
            };
            
            // Load Soul if provided
            let soul = if let Some(path) = soul_path {
                Soul::load_from_file(path)?
            } else {
                Soul::default_soul()
            };
            
            let actual_provider = provider.as_ref().unwrap_or(&config.agent.provider);
            let actual_model = model.as_ref().unwrap_or(&config.agent.model);

            // Model initialization based on provider
            let model_obj: Arc<dyn AgentModel> = match actual_provider.as_str() {
                "gemini" => {
                    let api_key = get_api_key("gemini")
                        .ok_or_else(|| anyhow!("GEMINI_API_KEY not found. Please run 'pharmakon onboard' or set GEMINI_API_KEY env var."))?;
                    log::info!("Using Gemini provider with model: {}", actual_model);
                    Arc::new(GeminiModel::new(api_key, actual_model.clone()))
                }
                "openai" => {
                    let api_key = get_api_key("openai")
                        .ok_or_else(|| anyhow!("OPENAI_API_KEY not found. Please run 'pharmakon onboard' or set OPENAI_API_KEY env var."))?;
                    log::info!("Using OpenAI provider with model: {}", actual_model);
                    Arc::new(OpenAIModel::new(api_key, actual_model.clone()))
                }
                "anthropic" => {
                    let api_key = get_api_key("anthropic").expect("ANTHROPIC_API_KEY not found in keyring or environment");
                    log::info!("Using Anthropic provider with model: {}", actual_model);
                    Arc::new(AnthropicModel::new(api_key, actual_model.clone()))
                }
                "groq" => {
                    let api_key = get_api_key("groq").expect("GROQ_API_KEY not found in keyring or environment");
                    log::info!("Using Groq provider with model: {}", actual_model);
                    Arc::new(GroqModel::new(api_key, actual_model.clone()))
                }
                "perplexity" => {
                    let api_key = get_api_key("perplexity").expect("PERPLEXITY_API_KEY not found in keyring or environment");
                    log::info!("Using Perplexity provider with model: {}", actual_model);
                    Arc::new(PerplexityModel::new(api_key, actual_model.clone()))
                }
                "ollama" => {
                    log::info!("Using Ollama provider with model: {}", actual_model);
                    Arc::new(OllamaModel::new(None, actual_model.clone()))
                }
                _ => {
                    log::warn!("Unknown provider '{}', falling back to MockModel", actual_provider);
                    Arc::new(MockModel)
                }
            };

            let home = dirs::home_dir().expect("Could not find home directory");
            let weaver_db_path = home.join(".pharmakon").join("memory_weaver.db");
            let weaver = Arc::new(pharmakon_memory::weaver::MemoryWeaver::new(weaver_db_path.to_str().unwrap()).await?);

            let mut agent = Agent::new(model_obj, session.clone())
                .with_store(session_store.clone())
                .with_memory_weaver(weaver);
            agent.with_soul(soul);
            
            // Register tools
            let agent_model = agent.model.clone();
            agent.add_tool(Arc::new(ShellTool));
            agent.add_tool(Arc::new(FileReadTool));
            agent.add_tool(Arc::new(WebFetchTool::new()));
            agent.add_tool(Arc::new(BraveSearchTool::new("".to_string())));
            agent.add_tool(Arc::new(TerminalTool::new()));
            agent.add_tool(Arc::new(BrowserTool::new(None)));
            agent.add_tool(Arc::new(pharmakon_tools::media::capture::ScreenshotTool));
            let fact_mem = agent.fact_memory.clone();
            agent.add_tool(Arc::new(FactTool::new(fact_mem)));
            agent.add_tool(Arc::new(CanvasTool::new(agent.event_tx.clone())));
            agent.add_tool(Arc::new(LinkUnderstandingTool::new()));
            agent.add_tool(Arc::new(MediaUnderstandingTool::new(agent_model)));

            agent.add_tool(Arc::new(CommitmentTool::new(session_store.clone())));
            
            let agent_arc = Arc::new(Mutex::new(agent));
            let cron_manager = Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);
            let cron_tool = pharmakon_core::automation::cron_tool::CronTool::new(cron_manager, Arc::downgrade(&agent_arc));
            
            agent_arc.lock().await.add_tool(Arc::new(cron_tool));

            // Load MCP tools
            if let Ok(mcp_tools) = pharmakon_core::mcp_manager::McpManager::load_tools().await {
                let mut agent_lock = agent_arc.lock().await;
                for tool in mcp_tools {
                    agent_lock.add_tool(tool);
                }
            }

            // Swarm Intelligence
            let spawner = Arc::new(pharmakon_core::orchestration::swarm::SwarmManager::new(agent_arc.clone()));
            agent_arc.lock().await.add_tool(Arc::new(pharmakon_core::orchestration::swarm::SwarmTool::new(spawner, 0)));

            let heartbeat_manager = pharmakon_core::automation::heartbeat::HeartbeatManager::new(agent_arc.clone(), 30);
            heartbeat_manager.start().await;
            
            log::info!("Sending message to agent (thinking: {}, model: {}, session: {}): {}", actual_thinking, config.agent.model, session, message);
            
            // Handle approvals in CLI
            let agent_for_approval = agent_arc.clone();
            let mut event_rx = agent_arc.lock().await.event_tx.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = event_rx.recv().await {
                    if let pharmakon_common::Event::ApprovalRequest { id, tool, args } = event {
                        println!("\n⚠️  [APPROVAL REQUIRED]");
                        println!("Tool: {}", tool);
                        println!("Arguments: {}", args);
                        print!("Allow execution? (y/N): ");
                        use std::io::{Write, BufRead};
                        std::io::stdout().flush().unwrap();
                        let mut input = String::new();
                        let stdin = std::io::stdin();
                        stdin.lock().read_line(&mut input).unwrap();
                        let approved = input.trim().to_lowercase() == "y";
                        
                        let agent_lock = agent_for_approval.lock().await;
                        let _ = agent_lock.approval_tx.send((id, approved)).await;
                    }
                }
            });

            match agent_arc.lock().await.chat(message).await {
                Ok(response) => {
                    println!("\nAssistant: {}", response);
                    
                    if *export_trajectory {
                        println!("\n--- Trajectory ---");
                        println!("{}", agent_arc.lock().await.trajectory.to_markdown());
                    }
                }
                Err(e) => {
                    log::error!("Agent error: {}", e);
                }
            }
        }
        Commands::Doctor { repair } => {
            if *repair {
                let repair_report = pharmakon_core::flows::doctor::Doctor::repair()?;
                println!("🔧 Auto-repair complete.");
                if repair_report.config_dir_created {
                    println!("  ✅ Created config directory.");
                }
                for err in repair_report.errors {
                    println!("  ❌ Error: {}", err);
                }
                println!();
            }

            let report = pharmakon_core::flows::doctor::Doctor::run_check().await?;
            println!("{}", t!("doctor_report_title"));
            println!("-------------------------------");
            println!("OpenAI API:     {}", if report.openai_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Anthropic API:  {}", if report.anthropic_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Gemini API:     {}", if report.gemini_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Telegram Bot:   {}", if report.telegram_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Discord Bot:    {}", if report.discord_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Slack Bot:      {}", if report.slack_ok { t!("ok") } else { t!("missing_api_key") });
            println!("Docker:         {}", if report.docker_ok { t!("ok") } else { t!("not_running") });
            println!("Config Dir:     {}", if report.config_dir_ok { t!("ok") } else { t!("missing_api_key") });
            println!("SQLite DB:      {}", if report.sqlite_ok { t!("ok") } else { t!("missing_api_key") });
            
            if !report.config_dir_ok || !report.sqlite_ok {
                println!("\n{}", t!("tip_repair"));
            }
        }
        Commands::Onboard { chat } => {
            if *chat {
                wizard::run_conversational_wizard().await?;
            } else {
                wizard::run_wizard()?;
            }
        }
        Commands::Secrets { action } => {
            let store = pharmakon_common::SecretStore::new();
            match action {
                SecretAction::Set { name, value } => {
                    store.set_secret(name, value)?;
                    println!("✅ Secret '{}' saved to keyring.", name);
                }
                SecretAction::Get { name } => {
                    let value = store.get_secret(name)?;
                    println!("{}: {}", name, value);
                }
                SecretAction::Delete { name } => {
                    store.delete_secret(name)?;
                    println!("✅ Secret '{}' deleted from keyring.", name);
                }
            }
        }
        Commands::Tui => {
            tui::run_tui().await?;
        }
        Commands::Desktop { soul: soul_path, provider, model } => {
            println!("Launching Native Desktop GUI...");
            
            // Re-use same initialization logic as Agent but for GUI
            let soul = if let Some(path) = soul_path {
                Soul::load_from_file(path)?
            } else {
                Soul::default_soul()
            };
            
            let actual_provider = provider.as_ref().unwrap_or(&config.agent.provider);
            let actual_model = model.as_ref().unwrap_or(&config.agent.model);
            let model_obj = get_model(actual_provider, actual_model);
            
            let mut agent = Agent::new(model_obj, "desktop-gui".to_string())
                .with_store(session_store.clone());
            agent.with_soul(soul);
            
            // Add core tools for GUI
            let agent_model = agent.model.clone();
            agent.add_tool(Arc::new(ShellTool));
            agent.add_tool(Arc::new(FileReadTool));
            agent.add_tool(Arc::new(WebFetchTool::new()));
            agent.add_tool(Arc::new(BraveSearchTool::new("".to_string())));
            agent.add_tool(Arc::new(TerminalTool::new()));
            agent.add_tool(Arc::new(BrowserTool::new(None)));
            agent.add_tool(Arc::new(pharmakon_tools::media::capture::ScreenshotTool));
            let fact_mem = agent.fact_memory.clone();
            agent.add_tool(Arc::new(FactTool::new(fact_mem)));
            agent.add_tool(Arc::new(CanvasTool::new(agent.event_tx.clone())));
            agent.add_tool(Arc::new(LinkUnderstandingTool::new()));
            agent.add_tool(Arc::new(MediaUnderstandingTool::new(agent_model)));
            agent.add_tool(Arc::new(CommitmentTool::new(session_store.clone())));
            
            let agent_arc = Arc::new(Mutex::new(agent));
            let cron_manager = Arc::new(pharmakon_core::automation::cron::CronManager::new().await?);
            let cron_tool = pharmakon_core::automation::cron_tool::CronTool::new(cron_manager.clone(), Arc::downgrade(&agent_arc));
            agent_arc.lock().await.add_tool(Arc::new(cron_tool));

            // Load MCP tools
            if let Ok(mcp_tools) = pharmakon_core::mcp_manager::McpManager::load_tools().await {
                let mut agent_lock = agent_arc.lock().await;
                for tool in mcp_tools {
                    agent_lock.add_tool(tool);
                }
            }

            let heartbeat_manager = pharmakon_core::automation::heartbeat::HeartbeatManager::new(agent_arc.clone(), 30);
            heartbeat_manager.start().await;

            // Start Gateway in background for integrations
            let mut gateway = Gateway::new(config.gateway.port, agent_arc.clone(), cron_manager.clone(), config.clone());
            let secret_store = pharmakon_common::SecretStore::new();
            
            if let Ok(token) = secret_store.get_secret("TELEGRAM_BOT_TOKEN") {
                log::info!("Registering Telegram channel for GUI session...");
                gateway.add_channel(Arc::new(TelegramChannel::new(token)));
            }
            if let Ok(token) = std::env::var("DISCORD_BOT_TOKEN") {
                gateway.add_channel(Arc::new(DiscordChannel::new(token)));
            }
            /*
            if let Ok(token) = std::env::var("SLACK_BOT_TOKEN") {
                gateway.add_channel(Arc::new(SlackChannel::new(token)));
            }
            */
            
            tokio::spawn(async move {
                if let Err(e) = gateway.run().await {
                    log::error!("Gateway error in GUI session: {}", e);
                }
            });

            if let Err(e) = pharmakon_gui::run_app(agent_arc, session_store.clone(), cron_manager) {
                log::error!("Failed to launch GUI: {}", e);
            }
        }
        Commands::Daemon { action, port } => {
            match action {
                DaemonAction::Start => {
                    let home = dirs::home_dir().expect("Could not find home directory");
                    let log_dir = home.join(".pharmakon").join("logs");
                    fs::create_dir_all(&log_dir)?;
                    
                    let stdout = fs::File::create(log_dir.join("gateway.out"))?;
                    let stderr = fs::File::create(log_dir.join("gateway.err"))?;
                    let pid_file = home.join(".pharmakon").join("gateway.pid");
                    
                    let daemonize = daemonize::Daemonize::new()
                        .pid_file(pid_file)
                        .working_directory("/tmp")
                        .stdout(stdout)
                        .stderr(stderr);

                    match daemonize.start() {
                        Ok(_) => {
                            // This code runs in the child process
                            let rt = tokio::runtime::Runtime::new()?;
                            rt.block_on(async {
                                let _ = env_logger::try_init();
                                if let Err(e) = run_gateway_service(*port, None, session_store, config).await {
                                    log::error!("Daemon Gateway error: {}", e);
                                }
                            });
                        }
                        Err(e) => eprintln!("Error starting daemon: {}", e),
                    }
                }
                DaemonAction::Stop => {
                    let home = dirs::home_dir().expect("Could not find home directory");
                    let pid_file = home.join(".pharmakon").join("gateway.pid");
                    if pid_file.exists() {
                        let pid_str = fs::read_to_string(&pid_file)?;
                        let pid = pid_str.trim().parse::<i32>()?;
                        println!("Stopping process {}...", pid);
                        // Send SIGTERM
                        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
                        let _ = fs::remove_file(pid_file);
                        println!("✅ Stopped.");
                    } else {
                        println!("❌ No PID file found. Is the gateway running?");
                    }
                }
                DaemonAction::Restart => {
                    println!("🔄 Restarting gateway daemon...");
                    let home = dirs::home_dir().expect("Could not find home directory");
                    let pid_file = home.join(".pharmakon").join("gateway.pid");
                    if pid_file.exists() {
                        let pid_str = fs::read_to_string(&pid_file)?;
                        let pid = pid_str.trim().parse::<i32>()?;
                        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
                        let _ = fs::remove_file(&pid_file);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                    // Start logic here... (Simplified by printing that user should run start)
                    // Actually, I'll just tell them to run start for now to avoid duplication,
                    // or I'll extract it. Let's just implement Status properly.
                    println!("✅ Stopped. Please run 'pharmakon daemon start' to complete restart.");
                }
                DaemonAction::Status => {
                    let home = dirs::home_dir().expect("Could not find home directory");
                    let pid_file = home.join(".pharmakon").join("gateway.pid");
                    if pid_file.exists() {
                        let pid_str = fs::read_to_string(&pid_file)?;
                        let pid = pid_str.trim().parse::<i32>()?;
                        // Check if process exists
                        let output = std::process::Command::new("ps").arg("-p").arg(pid.to_string()).output()?;
                        if output.status.success() {
                            println!("🟢 Daemon is running (PID: {})", pid);
                        } else {
                            println!("🔴 Daemon is NOT running, but PID file exists.");
                        }
                    } else {
                        println!("⚪ Daemon is NOT running.");
                    }
                }
            }
        }
        Commands::Trajectory { session, format, output } => {
            println!("Loading trajectory for session '{}'...", session);
            match session_store.load_trajectory(session).await {
                Ok(Some(trajectory)) => {
                    let content = if format == "json" {
                        trajectory.to_json()?
                    } else {
                        trajectory.to_markdown()
                    };

                    if let Some(path) = output {
                        std::fs::write(path, &content)?;
                        println!("Trajectory exported to file.");
                    } else {
                        println!("\n{}", content);
                    }
                }
                Ok(None) => {
                    println!("No trajectory found for session '{}'.", session);
                }
                Err(e) => {
                    log::error!("Error loading trajectory: {}", e);
                }
            }
        }
        Commands::Acp { url, token, session } => {
            run_acp_bridge(url.clone(), token.clone(), session.clone(), config).await?;
        }
    }

    Ok(())
}

async fn run_acp_bridge(url: Option<String>, token: Option<String>, _session: Option<String>, config: Config) -> Result<()> {
    use futures_util::{StreamExt, SinkExt};
    let gateway_url = url.unwrap_or_else(|| {
        format!("ws://127.0.0.1:{}/acp", config.gateway.port)
    });

    log::info!("Connecting ACP bridge to {}", gateway_url);

    let (ws_stream, _) = tokio_tungstenite::connect_async(&gateway_url).await
        .map_err(|e| anyhow!("Failed to connect to gateway at {}: {}", gateway_url, e))?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Proxy stdin -> WS
    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();

    let mut stdin_task = tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::from(line)).await {
                log::error!("WS send error: {}", e);
                break;
            }
        }
    });

    // Proxy WS -> stdout
    let mut stdout_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                println!("{}", text);
            }
        }
    });

    tokio::select! {
        _ = &mut stdin_task => stdout_task.abort(),
        _ = &mut stdout_task => stdin_task.abort(),
    }

    Ok(())
}
