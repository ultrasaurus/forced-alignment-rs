pub mod audio;
pub mod transcript;

mod ctc;
mod model;

use anyhow::Result;
use transcript::{
    ANOMALOUS_DURATION_ABS_SECS, ANOMALOUS_DURATION_RATIO, AlignReport, SUSPECT_THRESHOLD, Segment,
    SuspectReason, SuspectWord, Transcript,
};

pub const SAMPLE_RATE: u32 = 16_000;

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

/// Length of the trailing run of words (ending at the last word) that are
/// both below the suspect threshold and pace-collapsed relative to
/// `median_normalized_duration`. Returns 0 if there are too few words to
/// establish a median, or if the trailing run doesn't meet
/// [`transcript::MIN_TRUNCATION_RUN_WORDS`].
fn truncated_run_len(words: &[transcript::Word], median_normalized_duration: Option<f64>) -> usize {
    let Some(median) = median_normalized_duration else {
        return 0;
    };
    let run = words
        .iter()
        .rev()
        .take_while(|w| {
            let score_low = w.score.is_some_and(|s| s < SUSPECT_THRESHOLD);
            let pace_collapsed = word_duration(w)
                .map(|d| d / w.word.chars().count().max(1) as f64)
                .is_some_and(|n| n < median / transcript::TRUNCATION_PACE_RATIO);
            score_low && pace_collapsed
        })
        .count();
    if run >= transcript::MIN_TRUNCATION_RUN_WORDS {
        run
    } else {
        0
    }
}

/// Flag words whose score is below threshold or whose duration is anomalous
/// relative to the segment's typical per-character pace (see
/// [`transcript::SuspectReason`]).
fn detect_suspects(words: &[transcript::Word]) -> Vec<SuspectWord> {
    let median_normalized_duration = median_normalized_duration(words);
    let truncated_from = words.len() - truncated_run_len(words, median_normalized_duration);

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

            let reason = if i >= truncated_from {
                Some(SuspectReason::Truncated)
            } else if is_anomalous_duration {
                Some(SuspectReason::AnomalousDuration)
            } else if score < SUSPECT_THRESHOLD {
                Some(SuspectReason::LowScore)
            } else {
                None
            };

            reason.map(|reason| SuspectWord {
                word_index: i,
                word: w.word.clone(),
                score,
                reason,
            })
        })
        .collect()
}

/// Load the wav2vec2 model (and initialize its device) now, so its memory is
/// taken up front rather than on the first `align` call. Useful on a GPU
/// shared with other models: a lazy first load can fail if other work has
/// filled the card by then. `align` reuses the preloaded model.
pub fn preload() -> Result<()> {
    model::preload()
}

/// Options for [`align_with`].
#[derive(Debug, Clone, Default)]
pub struct AlignOptions {
    /// Indexes into `text.split_whitespace()` of the words that start a
    /// sentence. When given:
    /// - speech that isn't in the text is placed between sentences in
    ///   preference to inside one: entering it inside a sentence carries a
    ///   one-off cost;
    /// - a whole sentence the audio doesn't contain (left out by the reader,
    ///   or past where the recording stops) is skipped rather than crushed
    ///   into the speech around it: its words get no times, and
    ///   [`AlignReport::skipped`] lists the run.
    ///
    /// `None` (the default) does neither: every word is timed, and audio
    /// that stops early shows as [`transcript::SuspectReason::Truncated`].
    pub sentence_starts: Option<Vec<usize>>,
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
/// If the audio ends before the text does, the Viterbi DP is forced to crush
/// the remaining reference words into whatever frames are left, collapsing
/// both their score and duration. A trailing run of at least
/// [`transcript::MIN_TRUNCATION_RUN_WORDS`] consecutive words — ending at the
/// last word — that are both below [`transcript::SUSPECT_THRESHOLD`] and
/// pace-collapsed relative to the segment's median (see
/// [`transcript::TRUNCATION_PACE_RATIO`]) is classified as
/// [`transcript::SuspectReason::Truncated`]. A single late low-score word
/// without that collapse is [`transcript::SuspectReason::LowScore`] instead —
/// it's as likely to be a mispronunciation as a truncation.
pub fn align(samples: &[f32], text: &str) -> Result<(Transcript, AlignReport)> {
    align_with(samples, text, &AlignOptions::default())
}

/// [`align`], with options.
pub fn align_with(
    samples: &[f32],
    text: &str,
    options: &AlignOptions,
) -> Result<(Transcript, AlignReport)> {
    let duration_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let emissions = model::run_inference(samples)?;
    let (words, filtered, insertions, skipped) = ctc::viterbi_align(
        &emissions,
        text,
        duration_secs,
        options.sentence_starts.as_deref(),
    )?;

    // Skipped words have no times: the segment spans the timed ones.
    let start = words.iter().find_map(|w| w.start).unwrap_or(0.0);
    let end = words
        .iter()
        .rev()
        .find_map(|w| w.end)
        .unwrap_or(duration_secs as f64);

    let suspect = detect_suspects(&words);
    let report = AlignReport {
        filtered,
        suspect,
        insertions,
        skipped,
        threshold: SUSPECT_THRESHOLD,
    };

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
        let (words, filtered, _insertions, _) =
            crate::ctc::viterbi_align(&emissions, "hello ## world", 2.0, None).unwrap();
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
        let (_words, filtered, _insertions, _) =
            crate::ctc::viterbi_align(&emissions, "hello world", 2.0, None).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn trailing_collapsed_run_flagged_as_truncated() {
        // Four normally-paced, high-scoring words establish a ~0.1s/char
        // median, then two trailing words collapse to near-zero duration and
        // score — the signature of the Viterbi DP running out of audio
        // frames before it ran out of reference text.
        let words = vec![
            word("alpha", 0.0, 0.5, 0.95),
            word("beta", 0.5, 0.9, 0.95),
            word("gamma", 0.9, 1.4, 0.9),
            word("delta", 1.4, 1.9, 0.92),
            word("he", 1.9, 1.92, 0.02),
            word("a", 1.92, 1.93, 0.01),
        ];
        let suspect = detect_suspects(&words);
        assert_eq!(
            suspect.len(),
            2,
            "expected only the trailing pair flagged, got {suspect:?}"
        );
        assert_eq!(suspect[0].word, "he");
        assert_eq!(suspect[0].reason, SuspectReason::Truncated);
        assert_eq!(suspect[1].word, "a");
        assert_eq!(suspect[1].reason, SuspectReason::Truncated);
    }

    #[test]
    fn isolated_trailing_low_score_word_is_not_truncated() {
        // Regression guard: a single low-scoring word at the end (with
        // normal-paced neighbors) is a plausible mispronunciation or noise
        // artifact, not evidence the audio was cut short. Only a run of at
        // least MIN_TRUNCATION_RUN_WORDS should be classified Truncated.
        let words = vec![
            word("alpha", 0.0, 0.5, 0.95),
            word("beta", 0.5, 0.9, 0.95),
            word("gamma", 0.9, 1.4, 0.9),
            word("delta", 1.4, 1.9, 0.92),
            word("epsilon", 1.9, 2.4, 0.91),
            word("a", 2.4, 2.42, 0.01),
        ];
        let suspect = detect_suspects(&words);
        assert_eq!(suspect.len(), 1);
        assert_eq!(suspect[0].word, "a");
        assert_eq!(suspect[0].reason, SuspectReason::LowScore);
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
        let suspect = detect_suspects(&words);
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
        let suspect = detect_suspects(&words);
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
            (0..v)
                .map(|i| {
                    if i == winner {
                        winner_logit
                    } else {
                        background_logit
                    }
                })
                .collect()
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

        let (words, filtered, insertions, _) =
            crate::ctc::viterbi_align(&emissions, "hi ok", 1.0, None).unwrap();

        assert!(filtered.is_empty());
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hi");
        assert_eq!(words[1].word, "ok");
        // Neighboring words shouldn't have absorbed the inserted frames.
        assert!(words[0].end.unwrap() <= words[1].start.unwrap());

        assert_eq!(
            insertions.len(),
            1,
            "expected exactly one insertion, got {insertions:?}"
        );
        assert_eq!(insertions[0].before_word_index, 1);
        assert!(
            insertions[0].decoded_text.contains('x'),
            "decoded: {:?}",
            insertions[0].decoded_text
        );
    }

    /// Per-frame emissions with one clear winner each, for "frames" written
    /// as letters: `-` is blank, `|` the separator, anything else a letter.
    fn emissions_for(frames: &str) -> crate::model::Emissions {
        let vocab = base_vocab();
        let v = vocab.len();
        let log_probs = frames
            .chars()
            .map(|ch| {
                let winner = match ch {
                    '-' => 0,
                    c => vocab.iter().position(|t| t == &c.to_string()).unwrap(),
                };
                (0..v)
                    .map(|i| {
                        if i == winner {
                            0.9_f32.ln()
                        } else {
                            (0.1 / (v - 1) as f32).ln()
                        }
                    })
                    .collect()
            })
            .collect();
        crate::model::Emissions { log_probs, vocab }
    }

    #[test]
    fn a_word_does_not_stretch_over_speech_after_it() {
        // "ok", then inserted speech, then "kk" (as in "...book" after the
        // last word): the K frames after the insertion are only the letter
        // recurring, not more of the word.
        let frames = "OK-XXXXXXKK-";
        let (words, _, insertions, _) =
            crate::ctc::viterbi_align(&emissions_for(frames), "ok", frames.len() as f32, None)
                .unwrap();
        assert_eq!(insertions.len(), 1, "{insertions:?}");
        assert_eq!(words[0].end, Some(2.0), "{words:?}");
    }

    #[test]
    fn with_sentence_starts_a_lead_in_takes_the_opening_words_up_to_their_last_match() {
        // The lead-in says "ok" too, before the reading's own "ok".
        let frames = "OK-XXXXXX-OK-";
        let (words, _, insertions, _) = crate::ctc::viterbi_align(
            &emissions_for(frames),
            "ok",
            frames.len() as f32,
            Some(&[0]),
        )
        .unwrap();
        assert_eq!(words[0].start, Some(10.0), "{words:?}");
        assert_eq!(insertions.len(), 1, "{insertions:?}");
        assert_eq!(insertions[0].before_word_index, 0);
    }

    /// Aligns `text`, one sentence per word, against `frames` (one second
    /// each): each word's start, and the skips.
    fn align_sentences(
        frames: &str,
        text: &str,
        sentence_starts: Option<&[usize]>,
    ) -> (Vec<Option<f64>>, Vec<transcript::Skip>) {
        let (words, _, _, skips) = crate::ctc::viterbi_align(
            &emissions_for(frames),
            text,
            frames.len() as f32,
            sentence_starts,
        )
        .unwrap();
        (words.iter().map(|w| w.start).collect(), skips)
    }

    fn skip(start_word_index: usize, end_word_index: usize, at: f64) -> transcript::Skip {
        transcript::Skip {
            start_word_index,
            end_word_index,
            at,
        }
    }

    #[test]
    fn with_sentence_starts_an_unsaid_sentence_is_skipped() {
        // "no" isn't said: "hi" keeps its own frames, not pulled early.
        let (starts, skips) = align_sentences("OK--HI-", "ok no hi", Some(&[0, 1, 2]));
        assert_eq!(starts, vec![Some(0.0), None, Some(4.0)]);
        assert_eq!(skips.len(), 1, "{skips:?}");
        assert_eq!((skips[0].start_word_index, skips[0].end_word_index), (1, 2));
        assert!((2.0..=4.0).contains(&skips[0].at), "{skips:?}");
    }

    #[test]
    fn back_to_back_skipped_sentences_are_one_run() {
        let (starts, skips) = align_sentences("OK--HI-", "ok no so hi", Some(&[0, 1, 2, 3]));
        assert_eq!(starts, vec![Some(0.0), None, None, Some(4.0)]);
        assert_eq!(skips.len(), 1, "{skips:?}");
        assert_eq!((skips[0].start_word_index, skips[0].end_word_index), (1, 3));
    }

    #[test]
    fn opening_and_closing_sentences_can_be_skipped() {
        let (starts, skips) = align_sentences("-OK-", "no ok", Some(&[0, 1]));
        assert_eq!(starts, vec![None, Some(1.0)]);
        assert_eq!(skips, vec![skip(0, 1, 1.0)]);

        // The audio stops before the text does, and is shorter than it.
        let (starts, skips) = align_sentences("OK-", "ok no hi", Some(&[0, 1, 2]));
        assert_eq!(starts, vec![Some(0.0), None, None]);
        assert_eq!(skips.len(), 1, "{skips:?}");
        assert_eq!((skips[0].start_word_index, skips[0].end_word_index), (1, 3));
    }

    #[test]
    fn said_sentences_are_not_skipped() {
        let (starts, skips) = align_sentences("OK-NO-HI-", "ok no hi", Some(&[0, 1, 2]));
        assert_eq!(starts, vec![Some(0.0), Some(3.0), Some(6.0)]);
        assert!(skips.is_empty(), "{skips:?}");
    }

    #[test]
    fn without_sentence_starts_nothing_is_skipped() {
        let (starts, skips) = align_sentences("OK---HI-", "ok no hi", None);
        assert!(starts.iter().all(Option::is_some), "{starts:?}");
        assert!(skips.is_empty());
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
        assert!(
            report.filtered.is_empty(),
            "unexpected filtered: {:?}",
            report.filtered
        );
        assert!(
            report.suspect.is_empty(),
            "unexpected suspect: {:?}",
            report.suspect
        );
    }
}
