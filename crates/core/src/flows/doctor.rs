use anyhow::Result;
use std::env;
use std::fs;

pub struct Doctor;

#[derive(Debug, Clone)]
pub struct HealthReport {
    pub openai_ok: bool,
    pub anthropic_ok: bool,
    pub gemini_ok: bool,
    pub telegram_ok: bool,
    pub discord_ok: bool,
    pub slack_ok: bool,
    pub docker_ok: bool,
    pub sqlite_ok: bool,
    pub config_dir_ok: bool,
}

#[derive(Debug, Clone)]
pub struct RepairReport {
    pub config_dir_created: bool,
    pub db_initialized: bool,
    pub errors: Vec<String>,
}

impl Doctor {
    pub async fn run_check() -> Result<HealthReport> {
        log::info!("System Doctor: Starting health check...");
        
        let secret_store = pharmakon_common::SecretStore::new();
        
        let openai_ok = env::var("OPENAI_API_KEY").is_ok() || secret_store.get_secret("OPENAI_API_KEY").is_ok();
        let anthropic_ok = env::var("ANTHROPIC_API_KEY").is_ok() || secret_store.get_secret("ANTHROPIC_API_KEY").is_ok();
        let gemini_ok = env::var("GEMINI_API_KEY").is_ok() || secret_store.get_secret("GEMINI_API_KEY").is_ok();
        
        let telegram_ok = env::var("TELEGRAM_BOT_TOKEN").is_ok() || secret_store.get_secret("TELEGRAM_BOT_TOKEN").is_ok();
        let discord_ok_token = env::var("DISCORD_BOT_TOKEN").is_ok() || secret_store.get_secret("DISCORD_BOT_TOKEN").is_ok();
        let slack_ok_token = env::var("SLACK_BOT_TOKEN").is_ok() || secret_store.get_secret("SLACK_BOT_TOKEN").is_ok();
        
        // Simple docker check
        let docker_ok = std::process::Command::new("docker")
            .arg("info")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
            
        // Config directory check
        let home = dirs::home_dir().expect("Could not find home directory");
        let config_dir = home.join(".pharmakon");
        let config_dir_ok = config_dir.exists();
        
        // SQLite check
        let db_path = config_dir.join("pharmakon.db");
        let sqlite_ok = db_path.exists();
        
        Ok(HealthReport {
            openai_ok,
            anthropic_ok,
            gemini_ok,
            telegram_ok,
            discord_ok: discord_ok_token,
            slack_ok: slack_ok_token,
            docker_ok,
            sqlite_ok,
            config_dir_ok,
        })
    }

    pub fn repair() -> Result<RepairReport> {
        log::info!("System Doctor: Attempting auto-repair...");
        let mut report = RepairReport {
            config_dir_created: false,
            db_initialized: false,
            errors: Vec::new(),
        };

        let home = match dirs::home_dir() {
            Some(h) => h,
            None => {
                report.errors.push("Could not find home directory".to_string());
                return Ok(report);
            }
        };

        let config_dir = home.join(".pharmakon");
        if !config_dir.exists() {
            if let Err(e) = fs::create_dir_all(&config_dir) {
                report.errors.push(format!("Failed to create config directory: {}", e));
            } else {
                report.config_dir_created = true;
            }
        }

        // Database initialization is typically handled by the persistence layer,
        // but we can ensure the file is at least touchable if needed,
        // or just rely on the fact that creating the dir is enough for sqlite to create the file.
        
        Ok(report)
    }
}
