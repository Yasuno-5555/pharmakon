use pharmakon_core::agent::Agent;
use pharmakon_core::model::{
    AgentModel, AgentResult, CompletionRequest, CompletionResponse, FunctionCall, MessageContent,
    ToolCall,
};
use serde_json::json;
use std::sync::Arc;

struct ParallelMockModel;

#[async_trait::async_trait]
impl AgentModel for ParallelMockModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let has_tokyo = request.messages.iter().any(|m| {
            m.role == "tool"
                && m.content
                    .as_ref()
                    .map(|c| c.to_string())
                    .unwrap_or_default()
                    == "Sunny"
        });
        let has_osaka = request.messages.iter().any(|m| {
            m.role == "tool"
                && m.content
                    .as_ref()
                    .map(|c| c.to_string())
                    .unwrap_or_default()
                    == "Rainy"
        });

        if has_tokyo && has_osaka {
            return Ok(CompletionResponse {
                content: Some(MessageContent::Text(
                    "The weather in Tokyo is Sunny and in Osaka is Rainy.".to_string(),
                )),
                tool_calls: None,
                usage: None,
                finish_reason: None,
            });
        }

        let last_msg = request.messages.last().unwrap();
        let content = last_msg
            .content
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_default();

        if content.contains("Check weather in Tokyo and Osaka") {
            Ok(CompletionResponse {
                content: Some(MessageContent::Text(
                    "I'll check the weather for both cities.".to_string(),
                )),
                tool_calls: Some(vec![
                    ToolCall {
                        id: "call_tokyo".to_string(),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: "get_weather".to_string(),
                            arguments: json!({ "location": "Tokyo" }).to_string(),
                            thought_signature: None,
                        },
                    },
                    ToolCall {
                        id: "call_osaka".to_string(),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: "get_weather".to_string(),
                            arguments: json!({ "location": "Osaka" }).to_string(),
                            thought_signature: None,
                        },
                    },
                ]),
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
        "parallel-mock"
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

pub struct WeatherTool;

#[async_trait::async_trait]
impl pharmakon_common::Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }
    fn description(&self) -> &str {
        "Get weather for a location."
    }
    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            },
            "required": ["location"]
        })
    }
    async fn call(&self, args: serde_json::Value) -> pharmakon_common::AgentResult<String> {
        let loc = args["location"].as_str().unwrap_or("Unknown");
        if loc == "Tokyo" {
            Ok("Sunny".to_string())
        } else {
            Ok("Rainy".to_string())
        }
    }
}

#[tokio::test]
async fn test_parallel_tool_execution() {
    let model = Arc::new(ParallelMockModel);
    let agent = Agent::new(model, "parallel-session".to_string());
    agent.add_tool(Arc::new(WeatherTool)).await;

    let result = agent
        .chat("Check weather in Tokyo and Osaka")
        .await
        .unwrap();
    assert!(result.contains("Sunny"));
    assert!(result.contains("Rainy"));

    // Check history for parallel tool calls
    // It should have: User, Assistant (2 calls), Tool (Tokyo), Tool (Osaka), Assistant (Final)
    // Wait, since we join_all, the order of tool results might vary, but they should both be there.
    let history = agent.get_history().await;
    let tool_results: Vec<_> = history.iter().filter(|m| m.role == "tool").collect();
    assert_eq!(tool_results.len(), 2);
}