use crate::agent::Agent;
use crate::model::{CompletionRequest, Message, MessageContent};
use anyhow::Result;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::sync::Weak;
use tokio::process::Command;

pub struct RlfcTool {
    agent_ref: Weak<Agent>,
}

impl RlfcTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }

    async fn run_linter(&self, path: &str) -> Result<(bool, String)> {
        let output = Command::new("cargo")
            .arg("clippy")
            .arg("--fix")
            .arg("--allow-dirty")
            .arg("--allow-staged")
            .output()
            .await?;
        
        let success = output.status.success();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok((success, stderr))
    }
}

#[async_trait]
impl Tool for RlfcTool {
    fn name(&self) -> &str {
        "rlfc"
    }

    fn description(&self) -> &str {
        "Reinforcement Learning from Compiler Feedback. Optimize code by running an autonomous \
         fix-and-check loop using Rust compiler and Clippy feedback. Successful patterns are \
         learned and indexed into the Knowledge Nexus."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to optimize" },
                "max_iterations": { "type": "integer", "default": 3 }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap();
        let max_iters = args["max_iterations"].as_u64().unwrap_or(3);

        if let Some(agent) = self.agent_ref.upgrade() {
            // Index initial code state before the fix loop
            let initial_code = std::fs::read_to_string(path).unwrap_or_default();
            let mut current_iteration = 0;
            let mut last_error = String::from("No errors detected yet");

            while current_iteration < max_iters {
                current_iteration += 1;
                log::info!("RLFC: Iteration {} for {}", current_iteration, path);

                let (success, stderr) = self.run_linter(path).await.map_err(|e| AgentError(e.to_string()))?;

                if success {
                    let success_msg = format!("RLFC: Successfully optimized {} after {} iterations.", path, current_iteration);
                    
                    // Record causal edge: error_code → fixed_code (fixed_by)
                    if let Some(nexus) = &agent.knowledge_nexus {
                        // Index the code that was fixed
                        let code_id = format!("rlfc:code:{}", path);
                        let _ = nexus.remember_batch(vec![(
                            code_id.clone(),
                            format!("RLFC-optimized code for {}:\n\n{}", path, initial_code)
                        )]).await;
                        let error_id = format!("rlfc:error:{}-iter{}", path, current_iteration);
                        let success_id = format!("rlfc:success:{}", path);
                        let _ = nexus.record_causal_edge(
                            &error_id,
                            &success_id,
                            pharmakon_memory::graph::Edge::FIXED_BY,
                            1.0,
                        ).await;
                    }

                    // Index success pattern into Nexus
                    if let Some(nexus) = &agent.knowledge_nexus {
                        let content = std::fs::read_to_string(path).unwrap_or_default();
                        let _ = nexus.remember_batch(vec![(
                            format!("rlfc:success:{}", path),
                            format!("Success pattern for {}:\n\n{}", path, content)
                        )]).await;
                    }

                    return Ok(success_msg);
                }

                last_error = stderr;
                log::warn!("RLFC: Iteration {} failed. Error: {}", current_iteration, last_error.chars().take(100).collect::<String>());

                // Ask model to fix based on error
                let code = std::fs::read_to_string(path).unwrap_or_default();
                let prompt = format!(
                    "The following code has lint/compiler errors. Please fix it. \n\n\
                     FILE: {}\nCODE:\n{}\n\nERROR:\n{}",
                    path, code, last_error
                );

                let messages = vec![
                    Message {
                        role: "system".to_string(),
                        content: Some(MessageContent::Text("You are an expert Rust engineer specializing in Clippy fixes.".to_string())),
                        ..Default::default()
                    },
                    Message {
                        role: "user".to_string(),
                        content: Some(MessageContent::Text(prompt)),
                        ..Default::default()
                    },
                ];

                let model = {
                    let m = agent.model.lock().await;
                    (*m).clone()
                };

                let req = CompletionRequest {
                    messages,
                    temperature: Some(0.1),
                    max_tokens: Some(2048),
                    tools: None,
                };

                let response = model.complete(req).await.map_err(|e| AgentError(e.to_string()))?;
                if let Some(fixed_code) = response.content.as_ref().and_then(|c| c.as_text()) {
                    // Extract code block if model wrapped it
                    let mut clean_code = fixed_code.to_string();
                    if let Some(start) = clean_code.find("```rust") {
                        if let Some(end) = clean_code[start+7..].find("```") {
                            clean_code = clean_code[start+7..start+7+end].trim().to_string();
                        }
                    }
                    std::fs::write(path, clean_code).map_err(|e| AgentError(e.to_string()))?;
                }
            }

            // Record causal edge on failure: code → error (caused_by)
            if let Some(nexus) = &agent.knowledge_nexus {
                let code_id = format!("rlfc:code:{}", path);
                let error_id = format!("rlfc:error:{}", path);
                // Index the error for future reference
                let _ = nexus.remember_batch(vec![(
                    error_id.clone(),
                    format!("RLFC error for {}:\n\n{}", path, last_error)
                )]).await;
                let _ = nexus.record_causal_edge(
                    &error_id, &code_id, pharmakon_memory::graph::Edge::CAUSED_BY, 0.8,
                ).await;
            }

            Err(AgentError(format!("RLFC: Failed to optimize {} after {} iterations. Last error: {}", path, max_iters, last_error)))
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}