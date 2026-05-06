use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool};
use serde_json::{Value, json};

pub struct EnvironmentProbeTool;

impl EnvironmentProbeTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for EnvironmentProbeTool {
    fn name(&self) -> &str {
        "probe_environment"
    }
    fn description(&self) -> &str {
        "Probe the current environment for available tools and configuration."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        let mut info = Vec::new();
        info.push(format!("OS: {}", std::env::consts::OS));
        info.push(format!("Arch: {}", std::env::consts::ARCH));

        if let Ok(path) = std::env::var("PATH") {
            info.push(format!("PATH: {}", path));
        }

        Ok(info.join("\n"))
    }
}
