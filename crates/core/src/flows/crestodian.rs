use crate::agent::Agent;
use crate::model::AgentModel;
use crate::soul::Soul;
use pharmakon_tools::config_tool::ConfigTool;
use std::sync::Arc;

pub struct Crestodian;

impl Crestodian {
    pub fn create_agent(model: Arc<dyn AgentModel>) -> Agent {
        let soul = Soul {
            name: "Crestodian".to_string(),
            version: "1.0.0".to_string(),
            author: "Team Pharmakon".to_string(),
            traits: vec!["helpful".to_string(), "guide".to_string()],
            system_prompt: "You are the Pharmakon onboarding assistant. Your goal is to help the user configure their assistant. You can manage settings and secrets using the manage_config tool.\n\nInstructions:\n1. Greet the user warmly.\n2. If they want Telegram, guide them to @BotFather.\n3. Use manage_config to save settings and secrets.\n   - For API keys, use uppercase names like GEMINI_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY.\n   - For bot tokens, use names like TELEGRAM_BOT_TOKEN.\n4. Confirm once saved.\n5. Tell them they are ready once done!".to_string(),
            tool_allowlist: Some(vec!["manage_config".to_string(), "shell".to_string()]),
            ..Default::default()
        };

        let mut agent = Agent::new(model, "onboarding-session".to_string());
        agent = agent.with_soul(soul);

        agent.add_tool(Arc::new(ConfigTool));
        agent.add_tool(Arc::new(pharmakon_tools::ShellTool));

        agent
    }
}
