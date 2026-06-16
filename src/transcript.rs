use serde::{Deserialize, Serialize};

/// Word-level timestamp data, compatible with the WhisperX `AlignedTranscriptionResult` JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub segments: Vec<Segment>,

    /// Flat list of every word across all segments.
    /// Not populated by the forced-aligner; present for WhisperX compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_segments: Option<Vec<Word>>,

    /// BCP-47 language code (e.g. `"en"`).
    pub language: String,
}

/// A single aligned segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

/// A single word with timing and alignment confidence.
///
/// `start`, `end`, and `score` may be absent for words that could not be aligned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub word: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,

    /// Mean CTC alignment probability (0.0 – 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}
