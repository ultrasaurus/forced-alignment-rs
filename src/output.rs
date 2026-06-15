use crate::align::WordTiming;
use serde::Serialize;

/// WhisperX-style output schema.
#[derive(Serialize)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
    pub words: Vec<WordTiming>,
}

#[derive(Serialize)]
pub struct AlignmentResult {
    pub segments: Vec<Segment>,
}
