use crate::model::Emissions;
use anyhow::Result;

#[derive(Debug, serde::Serialize)]
pub struct WordTiming {
    pub word: String,
    pub start: f32,
    pub end: f32,
    pub score: f32,
}

/// Viterbi forced alignment between CTC emissions and the reference text.
pub fn align(_emissions: &Emissions, _text: &str) -> Result<Vec<WordTiming>> {
    todo!("tokenize text to label sequence, build trellis, backtrace, map frames to word spans")
}
