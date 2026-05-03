use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait AudioStream: Send + Sync {
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn read(&mut self, buf: &mut [f32]) -> Result<usize>;
    async fn write(&mut self, buf: &[f32]) -> Result<()>;
}

pub struct DummyStream;

#[async_trait]
impl AudioStream for DummyStream {
    async fn start(&mut self) -> Result<()> { Ok(()) }
    async fn stop(&mut self) -> Result<()> { Ok(()) }
    async fn read(&mut self, _buf: &mut [f32]) -> Result<usize> { Ok(0) }
    async fn write(&mut self, _buf: &[f32]) -> Result<()> { Ok(()) }
}
