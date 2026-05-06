use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolRegistry};
use serde_json::{Value, json};
use std::sync::{Arc, Weak};

pub struct SkillFactoryTool {
    agent_ref: Weak<dyn ToolRegistry>,
}

impl SkillFactoryTool {
    pub fn new(agent: Weak<dyn ToolRegistry>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for SkillFactoryTool {
    fn name(&self) -> &str {
        "skill_factory"
    }
    fn description(&self) -> &str {
        "Synthesize a new tool (skill) from code. Use this when you need a specific capability you don't currently have."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Short, unique name for the tool (e.g. 'analyze_wav')" },
                "description": { "type": "string", "description": "What the tool does" },
                "code": { "type": "string", "description": "Python or Shell code that implements the skill" },
                "language": { "type": "string", "enum": ["python", "shell"] }
            },
            "required": ["name", "description", "code", "language"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| AgentError("Missing name".to_string()))?;
        let description = args["description"]
            .as_str()
            .ok_or_else(|| AgentError("Missing description".to_string()))?;
        let code = args["code"]
            .as_str()
            .ok_or_else(|| AgentError("Missing code".to_string()))?;
        let language = args["language"]
            .as_str()
            .ok_or_else(|| AgentError("Missing language".to_string()))?;

        log::info!("SkillFactory: Synthesizing new skill '{}'...", name);

        // Create a wrapper tool that runs this code
        let dynamic_tool = DynamicSkill {
            name: name.to_string(),
            description: description.to_string(),
            code: code.to_string(),
            language: language.to_string(),
        };

        if let Some(agent) = self.agent_ref.upgrade() {
            agent.add_tool(Arc::new(dynamic_tool)).await;
            Ok(format!(
                "Successfully synthesized and installed new skill: '{}'. You can now call it like any other tool.",
                name
            ))
        } else {
            Err(AgentError(
                "Agent reference lost during synthesis".to_string(),
            ))
        }
    }
}

struct DynamicSkill {
    name: String,
    description: String,
    code: String,
    language: String,
}

#[async_trait]
impl Tool for DynamicSkill {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        // Generic parameters for now - the agent should know how to use it based on the code it wrote
        json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Input for the skill" }
            },
            "required": ["input"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let input = args["input"].as_str().unwrap_or_default();

        let output = if self.language == "python" {
            // Run in python sandbox
            tokio::process::Command::new("python3")
                .arg("-c")
                .arg(format!("input_val = r'''{}''';\n{}", input, self.code))
                .output()
                .await
                .map_err(|e| AgentError(format!("Python execution failed: {}", e)))?
        } else {
            // Run as shell
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(format!("export SKILL_INPUT='{}';\n{}", input, self.code))
                .output()
                .await
                .map_err(|e| AgentError(format!("Shell execution failed: {}", e)))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(format!("stdout: {}\nstderr: {}", stdout, stderr))
    }
}
