use serde::{Deserialize, Serialize};

/// Score threshold below which a word is considered suspect.
pub const SUSPECT_THRESHOLD: f64 = 0.3;

/// A word's per-character duration is flagged if it exceeds the median
/// per-character duration (across the segment) by this multiple.
pub const ANOMALOUS_DURATION_RATIO: f64 = 4.0;

/// A word's duration is flagged if it exceeds this many seconds outright,
/// regardless of the median (catches segments where multiple leaks have
/// already skewed the median upward).
pub const ANOMALOUS_DURATION_ABS_SECS: f64 = 1.0;

/// A word's per-character duration is considered pace-collapsed — the
/// signature of remaining reference words getting crushed into the last few
/// frames when the Viterbi DP runs out of audio before it runs out of text —
/// if it falls below the segment's median duration by this factor.
pub const TRUNCATION_PACE_RATIO: f64 = 4.0;

/// Minimum length of a trailing run of pace-collapsed, low-score words
/// required to classify as [`SuspectReason::Truncated`]. A single bad word
/// this late in a file could just be a mispronunciation or noise; genuine
/// truncation collapses several consecutive words because the Viterbi DP has
/// run out of frames, not just one.
pub const MIN_TRUNCATION_RUN_WORDS: usize = 2;

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
    /// Belongs to a trailing run of at least [`MIN_TRUNCATION_RUN_WORDS`]
    /// consecutive words (ending at the last word) that are all below the
    /// suspect threshold AND pace-collapsed relative to the segment's median
    /// (see [`TRUNCATION_PACE_RATIO`]) — the DP running out of audio frames
    /// before it runs out of reference text. A single late low-score word is
    /// [`LowScore`] instead; truncation crushes a run, not one word.
    Truncated,
    /// Duration far exceeds the segment's typical per-character pace (or
    /// exceeds an absolute cap), even though the score is above threshold —
    /// signals that extra audio (e.g. a leaked fragment) was absorbed into
    /// this word's span instead of being left unaligned.
    AnomalousDuration,
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

/// Speech found in the audio that isn't in the reference text — e.g. a reader
/// saying "Chapter" before a numeral, or a TTS engine leaking a fragment of
/// another clip. Detected as a run of frames the forced-aligner routes into
/// an optional filler state instead of smearing onto a neighboring word.
///
/// `decoded_text` is a best-effort, unconstrained greedy CTC decode of the
/// filler frames — useful for a human to eyeball or for fuzzy-matching
/// against the reference corpus (e.g. to identify a leaked TTS clip), but
/// low-confidence and not meant to be trusted like an aligned [`Word`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insertion {
    /// Index into the aligned word list of the word this insertion precedes.
    /// Equal to `words.len()` for an insertion after the final word.
    pub before_word_index: usize,
    /// Start time in seconds from the beginning of the audio.
    pub start: f64,
    /// End time in seconds from the beginning of the audio.
    pub end: f64,
    /// Best-effort greedy decode of the inserted speech. Low confidence.
    pub decoded_text: String,
    /// Mean probability of the decoded characters (0.0 - 1.0). Not
    /// comparable to [`Word::score`] or [`SuspectWord::score`] — those score
    /// confidence in a *known* word; this scores confidence in an
    /// unconstrained decode of unknown content.
    pub score: f64,
}

/// Diagnostic report returned alongside the [`Transcript`] from `align()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignReport {
    /// Words dropped before Viterbi because they had no alignable characters.
    pub filtered: Vec<FilteredWord>,
    /// Words aligned with low confidence.
    pub suspect: Vec<SuspectWord>,
    /// Speech detected outside the reference text (see [`Insertion`]).
    pub insertions: Vec<Insertion>,
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
