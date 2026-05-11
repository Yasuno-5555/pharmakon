use async_trait::async_trait;
use pharmakon_common::{AgentModel, AgentResult, CompletionRequest, CompletionResponse};

pub struct TestModel;

#[async_trait]
impl AgentModel for TestModel {
    async fn complete(&self, _request: CompletionRequest) -> AgentResult<CompletionResponse> {
        Ok(CompletionResponse {
            content: None,
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
        let stream = futures::stream::iter(vec![]);
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "test"
    }

    fn context_window(&self) -> usize {
        128000
    }

    fn max_output_tokens(&self) -> usize {
        4096
    }
}
