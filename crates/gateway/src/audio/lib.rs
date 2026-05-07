pub mod codec;
pub mod stream;

pub trait AudioProcessor: Send + Sync {
    fn name(&self) -> &str;
}
