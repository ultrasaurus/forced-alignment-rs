pub mod audio;
pub mod transcript;

mod ctc;
mod model;

use anyhow::Result;
use transcript::{Segment, Transcript};

pub const SAMPLE_RATE: u32 = 16_000;

/// Run forced alignment on pre-loaded 16 kHz mono audio samples against a known transcript.
///
/// Returns a [`Transcript`] with word-level timestamps. `word_segments` is not populated;
/// `language` is always `"en"`.
pub fn align(samples: &[f32], text: &str) -> Result<Transcript> {
    let duration_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let emissions = model::run_inference(samples)?;
    let words = ctc::viterbi_align(&emissions, text, duration_secs)?;

    let start = words.first().and_then(|w| w.start).unwrap_or(0.0);
    let end = words.last().and_then(|w| w.end).unwrap_or(duration_secs as f64);

    Ok(Transcript {
        segments: vec![Segment {
            start,
            end,
            text: text.to_string(),
            words,
            speaker: None,
        }],
        word_segments: None,
        language: "en".to_string(),
    })
}
