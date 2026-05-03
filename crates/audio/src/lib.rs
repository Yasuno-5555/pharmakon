pub mod stream;
pub mod codec;

pub trait AudioProcessor: Send + Sync {
    fn name(&self) -> &str;
}
