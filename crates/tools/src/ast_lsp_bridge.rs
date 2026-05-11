use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::Value;
use crate::lsp;

pub struct AstLspBridgeTool;

#[async_trait]
impl Tool for AstLspBridgeTool {
    fn name(&self) -> &str {
        "ast_lsp_bridge"
    }

    fn description(&self) -> &str {
        "Bridge AST-level intent to rust-analyzer LSP queries for definitions, references, and hover type data."
    }

    fn parameters(&self) -> Value {
        lsp::LspTool::new().parameters()
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        lsp::LspTool::new().call(args).await
    }
}
