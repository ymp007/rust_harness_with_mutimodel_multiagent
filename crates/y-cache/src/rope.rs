/// Dynamic RoPE Position Offset calculator for KV-cache reuse.
pub struct RoPEPositionShifter {
    base_frequency: f32,
}

impl RoPEPositionShifter {
    pub fn new(base_frequency: f32) -> Self {
        Self { base_frequency }
    }

    pub fn with_default_llama() -> Self {
        Self::new(10000.0)
    }

    /// Computes rotary frequency multiplier for dynamic RoPE context shifting.
    pub fn compute_frequency_multiplier(&self, pos_delta: usize, head_dim: usize) -> Vec<f32> {
        let mut freqs = Vec::with_capacity(head_dim / 2);
        for i in 0..(head_dim / 2) {
            let exponent = (2 * i) as f32 / head_dim as f32;
            let freq = 1.0 / self.base_frequency.powf(exponent);
            freqs.push((pos_delta as f32) * freq);
        }
        freqs
    }
}
