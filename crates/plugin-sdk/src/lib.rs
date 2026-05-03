use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn version(&self) -> &str;
    
    async fn on_load(&self) -> Result<()> { Ok(()) }
    async fn on_unload(&self) -> Result<()> { Ok(()) }
    
    // Tools provided by this plugin
    fn tools(&self) -> Vec<Box<dyn crate::PluginTool>>;
}

#[async_trait]
pub trait PluginTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn call(&self, args: Value) -> Result<String>;
}
