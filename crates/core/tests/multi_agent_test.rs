
use pharmakon_core::agent::Agent;
use pharmakon_core::model::{
    AgentModel, AgentResult, CompletionRequest, CompletionResponse, FunctionCall, MessageContent,
    ToolCall,
};
use pharmakon_core::orchestration::Supervisor;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

struct MultiAgentMockModel;

#[async_trait::async_trait]
impl AgentModel for MultiAgentMockModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let last_msg = request.messages.last().unwrap();
        let content = last_msg
            .content
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_default();

        if content.contains("Research latest AI trends") {
            Ok(CompletionResponse {
                content: Some(MessageContent::Text(
                    "I'll ask the researcher to look into this.".to_string(),
                )),
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    r#type: "function".to_string(),
                    function: FunctionCall {
                        name: "send_message".to_string(),
                        arguments: json!({
                            "to": "Researcher",
                            "message": "Find latest AI trends for 2024."
                        })
                        .to_string(),
                        thought_signature: None,
                    },
                }]),
                usage: None,
                finish_reason: None,
            })
        } else if content.contains("Find latest AI trends") {
            Ok(CompletionResponse {
                content: Some(MessageContent::Text(
                    "I found that Generative AI is huge in 2024.".to_string(),
                )),
                tool_calls: Some(vec![ToolCall {
                    id: "call_2".to_string(),
                    r#type: "function".to_string(),
                    function: FunctionCall {
                        name: "send_message".to_string(),
                        arguments: json!({
                            "to": "Manager",
                            "message": "AI trends found: Generative AI is the main focus."
                        })
                        .to_string(),
                        thought_signature: None,
                    },
                }]),
                usage: None,
                finish_reason: None,
            })
        } else if content.contains("AI trends found") {
            Ok(CompletionResponse {
                content: Some(MessageContent::Text(
                    "Generative AI is the top trend for 2024.".to_string(),
                )),
                tool_calls: Some(vec![ToolCall {
                    id: "call_3".to_string(),
                    r#type: "function".to_string(),
                    function: FunctionCall {
                        name: "final_answer".to_string(),
                        arguments: json!({
                            "answer": "The top AI trend for 2024 is Generative AI."
                        })
                        .to_string(),
                        thought_signature: None,
                    },
                }]),
                usage: None,
                finish_reason: None,
            })
        } else {
            Ok(CompletionResponse {
                content: Some(MessageContent::Text("Ok.".to_string())),
                tool_calls: None,
                usage: None,
                finish_reason: None,
            })
        }
    }

    fn name(&self) -> &str {
        "multi-agent-mock"
    }
    fn context_window(&self) -> usize {
        8192
    }
    fn max_output_tokens(&self) -> usize {
        4096
    }
    async fn stream_complete(
        &self,
        _request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        unimplemented!()
    }
}

#[tokio::test]
#[ignore = "requires Ollama with llama3.2"]
async fn test_multi_agent_collaboration() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let model = Arc::new(MultiAgentMockModel);
    let manager = Arc::new(Mutex::new(Agent::new(
        model.clone(),
        "manager-session".to_string(),
    )));
    let researcher = Arc::new(Mutex::new(Agent::new(
        model.clone(),
        "researcher-session".to_string(),
    )));

    manager
        .lock()
        .await
        .add_tool(Arc::new(pharmakon_core::orchestration::TeamMessageTool {
            from: "Manager".to_string(),
        }))
        .await;
    manager
        .lock()
        .await
        .add_tool(Arc::new(pharmakon_core::orchestration::FinalAnswerTool))
        .await;
    researcher
        .lock()
        .await
        .add_tool(Arc::new(pharmakon_core::orchestration::TeamMessageTool {
            from: "Researcher".to_string(),
        }))
        .await;

    let mut supervisor = Supervisor::new(
        "Research latest AI trends".to_string(),
        "Manager".to_string(),
    );
    supervisor.add_agent("Manager".to_string(), manager);
    supervisor.add_agent("Researcher".to_string(), researcher);

    let result: String = supervisor.run().await.unwrap();
    assert!(result.contains("The top AI trend for 2024 is Generative AI."));
}
