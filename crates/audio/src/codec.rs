use anyhow::Result;

pub trait AudioCodec: Send + Sync {
    fn name(&self) -> &str;
    fn encode(&self, input: &[f32]) -> Result<Vec<u8>>;
    fn decode(&self, input: &[u8]) -> Result<Vec<f32>>;
}

pub struct PcmCodec;

impl AudioCodec for PcmCodec {
    fn name(&self) -> &str { "pcm" }
    fn encode(&self, input: &[f32]) -> Result<Vec<u8>> {
        let mut output = Vec::with_capacity(input.len() * 4);
        for &sample in input {
            output.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(output)
    }
    fn decode(&self, input: &[u8]) -> Result<Vec<f32>> {
        let mut output = Vec::with_capacity(input.len() / 4);
        for chunk in input.chunks_exact(4) {
            output.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
        Ok(output)
    }
}
