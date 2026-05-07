use crate::agent::Agent;
use anyhow::Result;
use pharmakon_common::AgentResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Supervisor {
    pub goal: String,
    pub agents: HashMap<String, Arc<Mutex<Agent>>>,
    pub manager_name: String,
}

impl Supervisor {
    pub fn new(goal: String, manager_name: String) -> Self {
        Self {
            goal,
            agents: HashMap::new(),
            manager_name,
        }
    }

    pub fn add_agent(&mut self, name: String, agent: Arc<Mutex<Agent>>) {
        self.agents.insert(name, agent);
    }

    pub async fn run(&mut self) -> Result<String> {
        log::info!("Supervisor starting task: {}", self.goal);

        let mut current_agent_name = self.manager_name.clone();
        let mut last_message = self.goal.clone();
        let mut conversation_active = true;
        let mut turn_count = 0;
        let max_turns = 20;

        while conversation_active && turn_count < max_turns {
            turn_count += 1;
            log::info!("Turn {}: {}'s turn", turn_count, current_agent_name);

            let agent_arc = self
                .agents
                .get(&current_agent_name)
                .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", current_agent_name))?;

            let response = {
                let agent = agent_arc.lock().await;
                agent.chat(&last_message).await?
            };

            // Check if the agent wants to talk to someone else
            let next_target = {
                let agent = agent_arc.lock().await;
                let state_arc = agent.get_current_session_state().await;
                let state = state_arc.lock().await;
                let mut target = None;

                // Scan history in reverse to find routing tool calls from the last turn
                for msg in state.history.iter().rev() {
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            if tc.function.name == "send_message" {
                                let args: serde_json::Value =
                                    serde_json::from_str(&tc.function.arguments)
                                        .unwrap_or_default();
                                if let Some(to) = args["to"].as_str() {
                                    target = Some(to.to_string());
                                    last_message =
                                        args["message"].as_str().unwrap_or_default().to_string();
                                }
                            } else if tc.function.name == "final_answer" {
                                conversation_active = false;
                                let args: serde_json::Value =
                                    serde_json::from_str(&tc.function.arguments)
                                        .unwrap_or_default();
                                last_message =
                                    args["answer"].as_str().unwrap_or_default().to_string();
                            }
                        }
                    }
                    if target.is_some() || !conversation_active || msg.role == "user" {
                        break;
                    }
                }
                target
            };

            if let Some(target) = next_target {
                current_agent_name = target;
            } else if !conversation_active {
                log::info!("Supervisor: Task completed.");
                break;
            } else {
                // If no specific target, maybe return to manager or end
                if current_agent_name != self.manager_name {
                    current_agent_name = self.manager_name.clone();
                    last_message = response;
                } else {
                    conversation_active = false;
                }
            }
        }

        Ok(last_message)
    }
}

pub struct TeamMessageTool {
    pub from: String,
}

#[async_trait::async_trait]
impl pharmakon_common::Tool for TeamMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }
    fn description(&self) -> &str {
        "Send a message to another agent in the team."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": { "type": "string", "description": "The name of the agent to send the message to." },
                "message": { "type": "string", "description": "The content of the message." }
            },
            "required": ["to", "message"]
        })
    }
    async fn call(&self, _args: serde_json::Value) -> AgentResult<String> {
        Ok("Message routed by supervisor.".to_string())
    }
}

pub struct FinalAnswerTool;

#[async_trait::async_trait]
impl pharmakon_common::Tool for FinalAnswerTool {
    fn name(&self) -> &str {
        "final_answer"
    }
    fn description(&self) -> &str {
        "Provide the final answer to the supervisor and end the task."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string", "description": "The final result or answer." }
            },
            "required": ["answer"]
        })
    }
    async fn call(&self, args: serde_json::Value) -> AgentResult<String> {
        Ok(args["answer"].as_str().unwrap_or_default().to_string())
    }
}
