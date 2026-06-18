pub mod audio;
pub mod transcript;

mod ctc;
mod model;

use anyhow::Result;
use transcript::{AlignReport, Segment, SuspectReason, SuspectWord, Transcript, SUSPECT_THRESHOLD};

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
        let (words, filtered) =
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
        let (_words, filtered) =
            crate::ctc::viterbi_align(&emissions, "hello world", 2.0).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn suspect_words_flagged_at_end_are_truncated() {
        // With uniform low-probability emissions every word will score near
        // 1/V which is well below SUSPECT_THRESHOLD, so all words are suspect.
        // Words whose start time >= 90% of duration are classified Truncated.
        let emissions = fake_emissions(200, base_vocab());
        let (words, filtered) =
            crate::ctc::viterbi_align(&emissions, "hello world", 2.0).unwrap();
        let report = AlignReport {
            filtered,
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
pub fn align(samples: &[f32], text: &str) -> Result<(Transcript, AlignReport)> {
    let duration_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let emissions = model::run_inference(samples)?;
    let (words, filtered) = ctc::viterbi_align(&emissions, text, duration_secs)?;

    let start = words.first().and_then(|w| w.start).unwrap_or(0.0);
    let end = words.last().and_then(|w| w.end).unwrap_or(duration_secs as f64);

    let truncation_boundary = duration_secs as f64 * 0.9;
    let suspect: Vec<SuspectWord> = words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let score = w.score?;
            if score < SUSPECT_THRESHOLD {
                let reason = if w.start.unwrap_or(0.0) >= truncation_boundary {
                    SuspectReason::Truncated
                } else {
                    SuspectReason::LowScore
                };
                Some(SuspectWord { word_index: i, word: w.word.clone(), score, reason })
            } else {
                None
            }
        })
        .collect();

    let report = AlignReport { filtered, suspect, threshold: SUSPECT_THRESHOLD };

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
