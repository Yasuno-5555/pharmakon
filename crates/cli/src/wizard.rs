use dialoguer::{Input, Select, Password, Confirm, theme::ColorfulTheme};
use anyhow::{Result, anyhow};
use rust_i18n::t;
use pharmakon_common::{Config, SecretStore};
use pharmakon_core::flows::crestodian::Crestodian;
use crate::GeminiModel;
use std::fs;
use std::sync::Arc;

pub fn run_wizard() -> Result<()> {
    let theme = ColorfulTheme::default();
    
    // 1. Welcome & Security Note
    println!("\n=== {} ===", t!("intro_title"));
    println!("{}", t!("intro_note"));
    println!("\n--- {} ---", t!("security_note_title"));
    println!("{}", t!("security_note_text"));
    
    let ok = Confirm::with_theme(&theme)
        .with_prompt(t!("security_confirm").to_string())
        .default(false)
        .interact()?;
    
    if !ok {
        println!("{}", t!("denied_by_user"));
        return Ok(());
    }

    // 2. Setup Mode Selection
    let mode_options = vec![t!("mode_quickstart"), t!("mode_advanced")];
    let mode_idx = Select::with_theme(&theme)
        .with_prompt(t!("setup_mode_prompt").to_string())
        .items(&mode_options)
        .default(0)
        .interact()?;
    let is_advanced = mode_idx == 1;

    // 3. Agent Name
    let name: String = Input::with_theme(&theme)
        .with_prompt(t!("agent_name_prompt").to_string())
        .default("Claw".into())
        .interact_text()?;

    // 4. Provider & API Key
    let provider_options = vec!["gemini", "openai", "anthropic", "groq", "perplexity"];
    let provider_idx = Select::with_theme(&theme)
        .with_prompt(t!("provider_prompt").to_string())
        .items(&provider_options)
        .default(0) // Now gemini is 0 in this list if we want it to be default
        .interact()?;
    let provider = provider_options[provider_idx].to_string();
    
    let api_key: String = Password::with_theme(&theme)
        .with_prompt(format!("Enter your {} API Key", provider))
        .interact()?;

    // 5. Telegram Setup
    let enable_telegram = Confirm::with_theme(&theme)
        .with_prompt(t!("telegram_enable_prompt").to_string())
        .default(false)
        .interact()?;
    
    let mut telegram_token = None;
    if enable_telegram {
        let token: String = Password::with_theme(&theme)
            .with_prompt(t!("telegram_token_prompt").to_string())
            .interact()?;
        telegram_token = Some(token);
    }

    // 6. Advanced settings (Port, Auth)
    let mut gateway_port = 18789;
    let mut auth_mode = "token".to_string();
    let mut auth_value = "pharmakon-secret-token".to_string();

    if is_advanced {
        gateway_port = Input::with_theme(&theme)
            .with_prompt(t!("gateway_port_prompt").to_string())
            .default(18789)
            .interact_text()?;

        let auth_options = vec![t!("auth_token"), t!("auth_password")];
        let auth_idx = Select::with_theme(&theme)
            .with_prompt(t!("gateway_auth_prompt").to_string())
            .items(&auth_options)
            .default(0)
            .interact()?;
        
        if auth_idx == 0 {
            auth_mode = "token".to_string();
            auth_value = Password::with_theme(&theme)
                .with_prompt(t!("gateway_token_prompt").to_string())
                .interact()?;
        } else {
            auth_mode = "password".to_string();
            auth_value = Password::with_theme(&theme)
                .with_prompt(t!("gateway_password_prompt").to_string())
                .interact()?;
        }
    }

    // 7. Save configuration
    println!("\n{}", t!("configuring_agent", name = &name, provider = &provider));
    
    let mut config = Config::load().unwrap_or_default();
    config.default_agent.provider = provider.clone();
    config.default_agent.model = match provider.as_str() {
        "openai" => "gpt-4o".to_string(),
        "anthropic" => "claude-3-5-sonnet-20240620".to_string(),
        "gemini" => "gemini-1.5-pro".to_string(),
        _ => "default".to_string(),
    };
    
    config.gateway.port = gateway_port;
    config.gateway.dm_policy = "pairing".to_string();
    if auth_mode == "token" {
        config.gateway.webhook_secret = Some(auth_value.clone());
    }

    config.save()?;
    println!("✅ Configuration saved to ~/.pharmakon/config.json");

    // 8. Save secrets to Keyring
    let secret_store = SecretStore::new();
    let secret_name = format!("{}_API_KEY", provider.to_uppercase());
    secret_store.set_secret(&secret_name, &api_key)?;
    println!("✅ Stored {} in security layer.", secret_name);
    
    if let Some(tg_token) = telegram_token {
        secret_store.set_secret("TELEGRAM_BOT_TOKEN", &tg_token)?;
        println!("✅ Stored TELEGRAM_BOT_TOKEN in security layer.");
    }
    
    if auth_mode == "token" {
        secret_store.set_secret("GATEWAY_TOKEN", &auth_value)?;
        println!("✅ Stored GATEWAY_TOKEN in security layer.");
    } else {
        secret_store.set_secret("GATEWAY_PASSWORD", &auth_value)?;
        println!("✅ Stored GATEWAY_PASSWORD in security layer.");
    }
    
    println!("💡 Note: Secrets are stored in your OS Keyring with a fallback to ~/.pharmakon/secrets.json");

    
    // 8.5 Install as background service?
    let install_daemon = Confirm::with_theme(&theme)
        .with_prompt("Do you want to install Pharmakon as a persistent background service?")
        .default(true)
        .interact()?;
    
    if install_daemon {
        if let Err(e) = crate::service_installer::install_service(gateway_port) {
            println!("❌ Failed to install service: {}", e);
        }
    }

    // 9. Finalize
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    let workspace_dir = home.join(".pharmakon").join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    println!("\n{}", t!("setup_success"));
    println!("{}", t!("next_steps"));
    
    Ok(())
}

pub async fn run_conversational_wizard() -> Result<()> {
    let theme = ColorfulTheme::default();
    println!("\n=== {} (Conversational) ===", t!("intro_title"));
    
    // 1. Check for basic API key to start
    let secret_store = SecretStore::new();
    let gemini_key = secret_store.get_secret("GEMINI_API_KEY").or_else(|_| std::env::var("GEMINI_API_KEY"));
    
    let api_key = match gemini_key {
        Ok(key) => key,
        Err(_) => {
            println!("To start conversational onboarding, I need a Gemini API Key first.");
            let key: String = Password::with_theme(&theme)
                .with_prompt("Enter your Gemini API Key")
                .interact()?;
            secret_store.set_secret("GEMINI_API_KEY", &key)?;
            key
        }
    };

    // 2. Initialize Crestodian Agent
    let model = Arc::new(GeminiModel::new(api_key, "gemini-1.5-pro".to_string()));
    let mut agent = Crestodian::create_agent(model);
    
    println!("\n--- Talking to Crestodian ---");
    println!("Type 'exit' to finish setup.\n");

    loop {
        let input: String = Input::with_theme(&theme)
            .with_prompt("You")
            .interact_text()?;
        
        if input == "exit" || input == "quit" {
            break;
        }

        match agent.chat(&input).await {
            Ok(response) => {
                println!("\nCrestodian: {}\n", response);
            }
            Err(e) => {
                println!("\nError: {}\n", e);
            }
        }
    }

    Ok(())
}
