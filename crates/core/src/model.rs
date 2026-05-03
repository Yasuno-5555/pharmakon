pub use pharmakon_common::{Message, MessageContent, ContentPart, ToolCall, FunctionCall, CompletionRequest, ToolDefinition, FunctionDefinition, CompletionResponse, Usage, AgentModel, AgentError, AgentResult};
use async_trait::async_trait;

pub struct MockModel;

#[async_trait]
impl AgentModel for MockModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let user_msg = request.messages.last()
            .and_then(|m| m.content.as_ref())
            .map(|c| c.to_string())
            .unwrap_or_else(|| "".to_string());
        Ok(CompletionResponse {
            content: Some(MessageContent::Text(format!("Mock response to: {}", user_msg))),
            tool_calls: None,
            usage: None,
        })
    }

    async fn stream_complete(&self, _request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>> {
        let stream = futures::stream::iter(vec![Ok("Mock ".to_string()), Ok("stream ".to_string()), Ok("response".to_string())]);
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "mock-model"
    }
}
