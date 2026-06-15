use anyhow::Result;
use std::path::Path;

/// Decode an audio file to mono f32 samples at the given target sample rate.
pub fn load_audio(_path: &Path, _target_sample_rate: u32) -> Result<Vec<f32>> {
    todo!("decode via symphonia, downmix to mono, resample via rubato")
}
