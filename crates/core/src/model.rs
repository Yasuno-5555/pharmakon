use async_trait::async_trait;
pub use pharmakon_common::{
    AgentError, AgentErrorCode, AgentModel, AgentResult, CompletionRequest, CompletionResponse,
    ContentPart, ExecutionProfile, FunctionCall, FunctionDefinition, Message, MessageContent,
    ToolCall, ToolCategory, ToolDefinition, ToolMeta, Usage,
};

pub struct MockModel;

#[async_trait]
impl AgentModel for MockModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let user_msg = request
            .messages
            .last()
            .and_then(|m| m.content.as_ref())
            .map(|c| c.to_string())
            .unwrap_or_default();
        Ok(CompletionResponse {
            content: Some(MessageContent::Text(format!(
                "Mock response to: {}",
                user_msg
            ))),
            tool_calls: None,
            usage: None,
            finish_reason: None,
        })
    }

    async fn stream_complete(
        &self,
        _request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        let stream = futures::stream::iter(vec![
            Ok("Mock ".to_string()),
            Ok("stream ".to_string()),
            Ok("response".to_string()),
        ]);
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "mock-model"
    }

    fn context_window(&self) -> usize {
        8192
    }

    fn max_output_tokens(&self) -> usize {
        4096
    }

    fn is_mock(&self) -> bool {
        true
    }
}
