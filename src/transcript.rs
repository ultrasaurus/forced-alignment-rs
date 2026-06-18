use serde::{Deserialize, Serialize};

/// Score threshold below which a word is considered suspect.
pub const SUSPECT_THRESHOLD: f64 = 0.3;

/// A word from the input text that was dropped before alignment because it
/// contained no characters representable in the wav2vec2 vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilteredWord {
    pub word: String,
    /// Position in the original `text.split_whitespace()` sequence.
    pub original_index: usize,
}

/// Why a word was flagged as suspect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuspectReason {
    /// Score below threshold anywhere in the audio.
    LowScore,
    /// Score below threshold AND word starts in the final 10% of audio
    /// duration — strong signal that the audio was truncated.
    Truncated,
}

/// A word whose alignment confidence is low enough to warrant review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspectWord {
    /// Index into the aligned word list (filtered words not counted).
    pub word_index: usize,
    /// The word text as it appeared in the input.
    pub word: String,
    /// Mean CTC probability across frames assigned to this word (0.0 – 1.0).
    pub score: f64,
    /// Why this word was flagged.
    pub reason: SuspectReason,
}

/// Diagnostic report returned alongside the [`Transcript`] from `align()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignReport {
    /// Words dropped before Viterbi because they had no alignable characters.
    pub filtered: Vec<FilteredWord>,
    /// Words aligned with low confidence.
    pub suspect: Vec<SuspectWord>,
    /// Score threshold used to classify suspects.
    pub threshold: f64,
}

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

    /// Start time in seconds from the beginning of the audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,

    /// End time in seconds from the beginning of the audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,

    /// Mean CTC token probability across frames assigned to this word (0.0 – 1.0).
    /// Clean speech typically scores 0.8 and above; truncated or forced words score near 0.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}
