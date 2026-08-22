pub mod audio;
pub mod transcript;

mod ctc;
mod model;

use anyhow::Result;
use transcript::{
    AlignReport, Segment, SuspectReason, SuspectWord, Transcript, ANOMALOUS_DURATION_ABS_SECS,
    ANOMALOUS_DURATION_RATIO, SUSPECT_THRESHOLD,
};

pub const SAMPLE_RATE: u32 = 16_000;

#[cfg(test)]
mod tests {
    use super::*;
    use transcript::SuspectReason;

    /// Synthetic emissions: T frames, V vocab tokens, uniform low probability.
    /// Used to drive viterbi_align without a real model.
    fn fake_emissions(frames: usize, vocab: Vec<String>) -> crate::model::Emissions {
        let v = vocab.len();
        let uniform = (1.0_f32 / v as f32).ln();
        crate::model::Emissions {
            log_probs: vec![vec![uniform; v]; frames],
            vocab,
        }
    }

    fn base_vocab() -> Vec<String> {
        // Minimal wav2vec2-style vocab: blank, separator, A-Z.
        let mut v = vec!["<pad>".to_string(), "|".to_string()];
        for c in b'A'..=b'Z' {
            v.push((c as char).to_string());
        }
        v
    }

    #[test]
    fn filtered_words_recorded_with_correct_index() {
        // "hello ## world" — "##" has no alignable chars and should be filtered.
        let emissions = fake_emissions(200, base_vocab());
        let (words, filtered, _insertions) =
            crate::ctc::viterbi_align(&emissions, "hello ## world", 2.0).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].word, "##");
        assert_eq!(filtered[0].original_index, 1);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hello");
        assert_eq!(words[1].word, "world");
    }

    #[test]
    fn no_filtered_words_for_clean_text() {
        let emissions = fake_emissions(200, base_vocab());
        let (_words, filtered, _insertions) =
            crate::ctc::viterbi_align(&emissions, "hello world", 2.0).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn suspect_words_flagged_at_end_are_truncated() {
        // With uniform low-probability emissions every word will score near
        // 1/V which is well below SUSPECT_THRESHOLD, so all words are suspect.
        // Words whose start time >= 90% of duration are classified Truncated.
        let emissions = fake_emissions(200, base_vocab());
        let (words, filtered, insertions) =
            crate::ctc::viterbi_align(&emissions, "hello world", 2.0).unwrap();
        let report = AlignReport {
            filtered,
            insertions,
            suspect: words
                .iter()
                .enumerate()
                .filter_map(|(i, w)| {
                    let score = w.score?;
                    if score < transcript::SUSPECT_THRESHOLD {
                        let reason = if w.start.unwrap_or(0.0) >= 2.0 * 0.9 {
                            SuspectReason::Truncated
                        } else {
                            SuspectReason::LowScore
                        };
                        Some(transcript::SuspectWord { word_index: i, word: w.word.clone(), score, reason })
                    } else {
                        None
                    }
                })
                .collect(),
            threshold: transcript::SUSPECT_THRESHOLD,
        };
        // Under uniform emissions all words are suspect (score << 0.3).
        assert!(!report.suspect.is_empty());
        // The last word starts near the end so should be Truncated.
        let last = report.suspect.last().unwrap();
        assert_eq!(last.reason, SuspectReason::Truncated);
    }

    fn word(text: &str, start: f64, end: f64, score: f64) -> transcript::Word {
        transcript::Word {
            word: text.to_string(),
            start: Some(start),
            end: Some(end),
            score: Some(score),
            speaker: None,
        }
    }

    #[test]
    fn anomalous_duration_flagged_even_above_score_threshold() {
        // Reproduces the hypertext87 seg06 leak: "Another" absorbs ~7s of
        // leaked audio (score 0.424, above SUSPECT_THRESHOLD) while every
        // other word paces normally at ~0.2s/word with high scores.
        let words = vec![
            word("Another", 0.44, 2.64, 0.424),
            word("thing", 7.15, 7.31, 0.999),
            word("worth", 7.31, 7.51, 0.97),
            word("noting", 7.51, 7.81, 0.95),
            word("is", 7.81, 7.95, 0.96),
        ];
        let suspect = detect_suspects(&words, 8.0);
        assert_eq!(suspect.len(), 1);
        assert_eq!(suspect[0].word, "Another");
        assert_eq!(suspect[0].reason, SuspectReason::AnomalousDuration);
    }

    #[test]
    fn normal_pacing_has_no_anomalous_duration() {
        let words = vec![
            word("hello", 0.0, 0.2, 0.95),
            word("there", 0.2, 0.4, 0.96),
            word("my", 0.4, 0.55, 0.97),
            word("friend", 0.55, 0.8, 0.98),
        ];
        let suspect = detect_suspects(&words, 1.0);
        assert!(suspect.is_empty());
    }

    #[test]
    fn detects_insertion_between_words_without_corrupting_neighbors() {
        // Reference text "hi ok"; audio simulates a reader inserting "xx"
        // between the two words. Builds explicit per-frame emissions rather
        // than fake_emissions' uniform distribution, so specific frames can
        // strongly favor a letter outside the reference vocabulary at that
        // position (H, I, |, O, K) — the filler state should absorb those
        // frames instead of the Viterbi DP smearing them onto "hi" or "ok".
        let vocab = base_vocab();
        let idx = |ch: char| vocab.iter().position(|t| t == &ch.to_string()).unwrap();
        let pad = 0usize; // "<pad>" is vocab[0] per base_vocab()
        let sep = 1usize; // "|" is vocab[1]
        let v = vocab.len();
        let winner_logit = (0.9_f32).ln();
        let background_logit = (0.1_f32 / (v - 1) as f32).ln();

        let frame_for = |winner: usize| -> Vec<f32> {
            (0..v).map(|i| if i == winner { winner_logit } else { background_logit }).collect()
        };

        let log_probs = vec![
            frame_for(idx('H')),
            frame_for(idx('I')),
            frame_for(sep),
            frame_for(idx('X')),
            frame_for(idx('X')),
            frame_for(idx('X')),
            frame_for(idx('X')),
            frame_for(idx('X')),
            frame_for(idx('X')),
            frame_for(pad),
            frame_for(idx('O')),
            frame_for(idx('K')),
        ];
        let emissions = crate::model::Emissions { log_probs, vocab };

        let (words, filtered, insertions) =
            crate::ctc::viterbi_align(&emissions, "hi ok", 1.0).unwrap();

        assert!(filtered.is_empty());
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hi");
        assert_eq!(words[1].word, "ok");
        // Neighboring words shouldn't have absorbed the inserted frames.
        assert!(words[0].end.unwrap() <= words[1].start.unwrap());

        assert_eq!(insertions.len(), 1, "expected exactly one insertion, got {insertions:?}");
        assert_eq!(insertions[0].before_word_index, 1);
        assert!(insertions[0].decoded_text.contains('x'), "decoded: {:?}", insertions[0].decoded_text);
    }

    /// Requires model weights (~360MB) downloaded from HuggingFace.
    #[test]
    #[ignore]
    fn clean_audio_has_no_suspect_words() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("validation-samples/short-sentence.mp3");
        let text = "it may contain annotations, additions and footnotes";
        let samples = audio::load_audio(&path, SAMPLE_RATE).unwrap();
        let (_transcript, report) = align(&samples, text).unwrap();
        assert!(report.filtered.is_empty(), "unexpected filtered: {:?}", report.filtered);
        assert!(report.suspect.is_empty(), "unexpected suspect: {:?}", report.suspect);
    }
}

/// Run forced alignment on pre-loaded 16 kHz mono audio samples against a known transcript.
///
/// Returns a [`Transcript`] with word-level timestamps and an [`AlignReport`] describing
/// any filtered or suspect words. `word_segments` is not populated; `language` is always `"en"`.
///
/// # Input preprocessing
///
/// For best results, pass text that matches what was actually spoken:
/// - Use the same normalization applied before synthesis (e.g. "for example" not "e.g.")
/// - Strip speaker-directive prefixes such as `Speaker 1:` — these are not spoken
/// - Strip leading/trailing punctuation from tokens — the CTC vocab contains only letters
///   and `|`; punctuation deflates scores for otherwise clean words
///
/// # Scores
///
/// Word scores are mean CTC token probabilities in `[0.0, 1.0]`. Clean speech
/// consistently scores `0.8` and above. Words below [`transcript::SUSPECT_THRESHOLD`]
/// (0.3) are reported in [`AlignReport::suspect`].
///
/// # Truncation detection
///
/// If the audio ends before the text does, tail words are forced into the last
/// frames by the Viterbi constraint and score near zero. Words below threshold
/// whose start time falls in the final 10% of audio duration are classified as
/// [`transcript::SuspectReason::Truncated`].
fn word_duration(w: &transcript::Word) -> Option<f64> {
    Some(w.end? - w.start?)
}

/// Median of per-character duration across words with timing, used as the
/// segment's typical pace for [`transcript::SuspectReason::AnomalousDuration`].
/// Returns `None` if there are too few timed words for a median to be
/// meaningful.
fn median_normalized_duration(words: &[transcript::Word]) -> Option<f64> {
    const MIN_WORDS_FOR_MEDIAN: usize = 4;
    let mut normalized: Vec<f64> = words
        .iter()
        .filter_map(|w| {
            let d = word_duration(w)?;
            Some(d / w.word.chars().count().max(1) as f64)
        })
        .collect();
    if normalized.len() < MIN_WORDS_FOR_MEDIAN {
        return None;
    }
    normalized.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(normalized[normalized.len() / 2])
}

/// Flag words whose score is below threshold or whose duration is anomalous
/// relative to the segment's typical per-character pace (see
/// [`transcript::SuspectReason`]).
fn detect_suspects(words: &[transcript::Word], duration_secs: f32) -> Vec<SuspectWord> {
    let truncation_boundary = duration_secs as f64 * 0.9;
    let median_normalized_duration = median_normalized_duration(words);
    words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let score = w.score?;
            let duration = word_duration(w);
            let normalized = duration.map(|d| d / w.word.chars().count().max(1) as f64);
            let is_anomalous_duration = duration.is_some_and(|d| d > ANOMALOUS_DURATION_ABS_SECS)
                || median_normalized_duration
                    .zip(normalized)
                    .is_some_and(|(median, n)| n > median * ANOMALOUS_DURATION_RATIO);

            let reason = if is_anomalous_duration {
                Some(SuspectReason::AnomalousDuration)
            } else if score < SUSPECT_THRESHOLD {
                if w.start.unwrap_or(0.0) >= truncation_boundary {
                    Some(SuspectReason::Truncated)
                } else {
                    Some(SuspectReason::LowScore)
                }
            } else {
                None
            };

            reason.map(|reason| SuspectWord { word_index: i, word: w.word.clone(), score, reason })
        })
        .collect()
}

pub fn align(samples: &[f32], text: &str) -> Result<(Transcript, AlignReport)> {
    let duration_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let emissions = model::run_inference(samples)?;
    let (words, filtered, insertions) = ctc::viterbi_align(&emissions, text, duration_secs)?;

    let start = words.first().and_then(|w| w.start).unwrap_or(0.0);
    let end = words.last().and_then(|w| w.end).unwrap_or(duration_secs as f64);

    let suspect = detect_suspects(&words, duration_secs);
    let report = AlignReport { filtered, suspect, insertions, threshold: SUSPECT_THRESHOLD };

    Ok((
        Transcript {
            segments: vec![Segment {
                start,
                end,
                text: text.to_string(),
                words,
                speaker: None,
            }],
            word_segments: None,
            language: "en".to_string(),
        },
        report,
    ))
}
