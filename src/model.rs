use anyhow::Result;

/// CTC log-probabilities: shape [time_steps, vocab_size].
pub struct Emissions {
    pub log_probs: Vec<Vec<f32>>,
    pub vocab: Vec<String>,
}

/// Load the MMS CTC model and run inference on audio samples (16kHz mono).
pub fn run_inference(_samples: &[f32]) -> Result<Emissions> {
    todo!("load MMS via candle + hf-hub, run forward pass, return log-softmax output")
}
