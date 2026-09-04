use crate::model::Emissions;
use crate::transcript::{FilteredWord, Insertion, Word};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// One character of the normalized transcript, mapped to a vocab token id.
struct CharToken {
    id: usize,
    /// Index into the original word list this character belongs to, or None for the
    /// word-delimiter token ("|").
    word_idx: Option<usize>,
}

/// Log-space penalty subtracted from a frame's best non-blank emission before
/// it's allowed to compete for the filler state. Keeps ordinary silence/noise
/// out of filler (which would otherwise fabricate spurious insertions) while
/// letting genuine inserted speech win. Tune empirically against
/// `validation-samples/as-we-may-think-6*` (should detect "chapter") and the
/// clean samples (should stay at zero insertions).
const FILLER_PENALTY: f32 = 1.5;

/// CTC emissions are "spiky" — even within a single real word, a letter's
/// probability spikes for 1-2 frames then dips back toward blank before the
/// next letter spikes. That's true for inserted speech too, so a strict
/// contiguous-frame requirement would fragment one insertion into several
/// too-short bursts. Bridge filler bursts separated by a gap this small
/// before applying [`MIN_FILLER_FRAMES`] — observed gaps within one inserted
/// word ("chapter") topped out around 3 frames on real audio (see
/// `debug_dump_chapter_six_frames` history / validation-samples).
const MAX_FILLER_GAP_FRAMES: usize = 4;

/// Minimum contiguous filler frames to report as an insertion, rather than
/// discarding as a short noise blip. wav2vec2-base's stride is ~20ms/frame;
/// 6 frames (~120ms) is short enough to catch a quick "Chapter"/"Section"
/// but long enough to reject the 1-2 frame consonant-boundary blips observed
/// on real audio (see validation-samples/as-we-may-think-6*).
const MIN_FILLER_FRAMES: usize = 6;

/// Viterbi forced alignment between CTC emissions and the reference text.
///
/// Frames that don't match the reference text (e.g. a reader-inserted
/// "Chapter" before a numeral) are routed into an optional filler state
/// instead of corrupting a neighboring word's span; see [`Insertion`].
///
/// Returns the aligned words, words dropped before alignment because they
/// contained no characters in the wav2vec2 vocabulary, and detected
/// insertions.
pub fn viterbi_align(
    emissions: &Emissions,
    text: &str,
    audio_duration_secs: f32,
) -> Result<(Vec<Word>, Vec<FilteredWord>, Vec<Insertion>)> {
    let vocab_map: HashMap<&str, usize> = emissions
        .vocab
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), i))
        .collect();
    let blank_id = *vocab_map
        .get("<pad>")
        .ok_or_else(|| anyhow!("vocab has no <pad> (blank) token"))?;
    let word_sep_id = *vocab_map
        .get("|")
        .ok_or_else(|| anyhow!("vocab has no '|' word-separator token"))?;

    // Drop words with no alignable characters (e.g. markdown artifacts like "##" or "---").
    // Record filtered words with their original position for the AlignReport.
    let mut filtered: Vec<FilteredWord> = Vec::new();
    let words: Vec<&str> = text
        .split_whitespace()
        .enumerate()
        .filter_map(|(i, w)| {
            if w.to_uppercase()
                .chars()
                .any(|ch| vocab_map.contains_key(ch.to_string().as_str()))
            {
                Some(w)
            } else {
                filtered.push(FilteredWord { word: w.to_string(), original_index: i });
                None
            }
        })
        .collect();
    if words.is_empty() {
        return Ok((vec![], filtered, vec![]));
    }

    let mut tokens: Vec<CharToken> = Vec::new();
    for (wi, word) in words.iter().enumerate() {
        if wi > 0 {
            tokens.push(CharToken { id: word_sep_id, word_idx: None });
        }
        for ch in word.to_uppercase().chars() {
            if let Some(&id) = vocab_map.get(ch.to_string().as_str()) {
                tokens.push(CharToken { id, word_idx: Some(wi) });
            }
        }
    }
    let label_ids: Vec<usize> = tokens.iter().map(|t| t.id).collect();

    let (frame_spans, filler_spans) =
        ctc_forced_align(&emissions.log_probs, &label_ids, blank_id)?;

    // Group per-token frame spans into per-word spans.
    let mut word_spans: Vec<Option<(usize, usize, f32, usize)>> = vec![None; words.len()];
    for (token, (start, end, score)) in tokens.iter().zip(frame_spans.iter()) {
        if let Some(wi) = token.word_idx {
            let entry = word_spans[wi].get_or_insert((*start, *end, 0.0, 0));
            entry.0 = entry.0.min(*start);
            entry.1 = entry.1.max(*end);
            entry.2 += score;
            entry.3 += 1;
        }
    }

    let num_frames = emissions.log_probs.len().max(1);
    let seconds_per_frame = audio_duration_secs / num_frames as f32;

    let mut out = Vec::with_capacity(words.len());
    for (word, span) in words.iter().zip(word_spans.into_iter()) {
        let (start, end, score_sum, count) =
            span.ok_or_else(|| anyhow!("word '{word}' produced no alignable characters"))?;
        out.push(Word {
            word: word.to_string(),
            start: Some((start as f32 * seconds_per_frame) as f64),
            end: Some((end as f32 * seconds_per_frame) as f64),
            score: Some((score_sum / count as f32) as f64),
            speaker: None,
        });
    }

    // `gap_index` on a filler span is the CharToken column, which spans word
    // separators too; map it down to a word index by counting how many real
    // words are fully consumed by that column.
    let gap_to_word_idx: Vec<usize> = {
        let mut v = Vec::with_capacity(label_ids.len() + 1);
        let mut consumed = 0usize;
        v.push(0);
        for tok in &tokens {
            if let Some(wi) = tok.word_idx {
                consumed = wi + 1;
            }
            v.push(consumed);
        }
        v
    };

    let insertions = filler_spans
        .into_iter()
        .filter(|s| s.end_frame - s.start_frame >= MIN_FILLER_FRAMES)
        .filter_map(|s| {
            let decoded_text = decode_filler(&emissions.log_probs, &emissions.vocab, s.start_frame, s.end_frame, blank_id);
            if decoded_text.0.trim().is_empty() {
                // An empty greedy decode means the filler span didn't
                // resolve to any character, not even a low-confidence one —
                // in practice this is silence/noise the aligner routed to
                // the filler state, not a real leaked-audio fragment (see
                // false-positive analysis of forced-alignment "leaked
                // audio" warnings against manually-classified samples).
                return None;
            }
            Some(Insertion {
                before_word_index: gap_to_word_idx[s.gap_index],
                start: (s.start_frame as f32 * seconds_per_frame) as f64,
                end: (s.end_frame as f32 * seconds_per_frame) as f64,
                decoded_text: decoded_text.0,
                score: decoded_text.1,
            })
        })
        .collect();

    Ok((out, filtered, insertions))
}

/// Greedy CTC decode of an unconstrained frame span: argmax per frame,
/// collapse repeats, drop blanks. `|` is rendered as a space. Best-effort —
/// not a real ASR decode, just enough to eyeball what was said.
fn decode_filler(
    log_probs: &[Vec<f32>],
    vocab: &[String],
    start_frame: usize,
    end_frame: usize,
    blank_id: usize,
) -> (String, f64) {
    let mut out = String::new();
    let mut prob_sum = 0.0f64;
    let mut prob_count = 0usize;
    let mut prev: Option<usize> = None;
    for frame in &log_probs[start_frame..end_frame] {
        let (best_id, best_val) = frame
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        if Some(best_id) != prev {
            if best_id != blank_id {
                let ch = vocab[best_id].as_str();
                out.push_str(if ch == "|" { " " } else { ch });
                prob_sum += best_val.exp() as f64;
                prob_count += 1;
            }
            prev = Some(best_id);
        }
    }
    let score = if prob_count > 0 { prob_sum / prob_count as f64 } else { 0.0 };
    (out.trim().to_lowercase(), score)
}

/// A contiguous run of frames routed into the filler (non-reference) state.
/// `gap_index` is the reference-token column the filler sits at — i.e. the
/// number of reference tokens already consumed when the filler began.
struct FillerSpan {
    gap_index: usize,
    start_frame: usize,
    end_frame: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Ref,
    Filler,
}

/// How a `(t, j, Ref)` cell's optimum was reached.
#[derive(Clone, Copy, PartialEq)]
enum RefFrom {
    StayRef,
    FromFiller,
    Move,
}

/// How a `(t, j, Filler)` cell's optimum was reached.
#[derive(Clone, Copy, PartialEq)]
enum FillerFrom {
    StayFiller,
    EnterFromRef,
}

/// Forced alignment via dynamic programming over (frame, token, mode) states.
///
/// Each reference-token column `j` (number of tokens consumed so far) has two
/// modes: `Ref` (normal forced alignment — blank or the current/next
/// reference token) and `Filler` (an optional garbage state that can absorb
/// any non-blank frame content without being forced into blank or a
/// neighboring token's span). This is what lets reader- or TTS-inserted
/// speech get flagged as an [`Insertion`] instead of corrupting the
/// timestamps of adjacent reference words.
///
/// Returns per-token frame spans (as before) plus detected filler spans.
fn ctc_forced_align(
    log_probs: &[Vec<f32>],
    tokens: &[usize],
    blank_id: usize,
) -> Result<(Vec<(usize, usize, f32)>, Vec<FillerSpan>)> {
    let t_len = log_probs.len();
    let l_len = tokens.len();
    if t_len < l_len {
        return Err(anyhow!(
            "audio too short ({t_len} frames) to align {l_len} characters"
        ));
    }

    const NEG_INF: f32 = f32::NEG_INFINITY;

    let mut dp_ref = vec![vec![NEG_INF; l_len + 1]; t_len + 1];
    let mut dp_filler = vec![vec![NEG_INF; l_len + 1]; t_len + 1];
    let mut back_ref: Vec<Vec<Option<RefFrom>>> = vec![vec![None; l_len + 1]; t_len + 1];
    let mut back_filler: Vec<Vec<Option<FillerFrom>>> = vec![vec![None; l_len + 1]; t_len + 1];
    dp_ref[0][0] = 0.0;

    for t in 1..=t_len {
        let frame = &log_probs[t - 1];
        let filler_emit = frame
            .iter()
            .enumerate()
            .filter(|&(id, _)| id != blank_id)
            .map(|(_, &p)| p)
            .fold(NEG_INF, f32::max)
            - FILLER_PENALTY;

        for j in 0..=l_len {
            let stay_emit = if j == 0 {
                frame[blank_id]
            } else {
                frame[blank_id].max(frame[tokens[j - 1]])
            };

            // Ref cell.
            let stay_ref = dp_ref[t - 1][j] + stay_emit;
            let from_filler = dp_filler[t - 1][j] + stay_emit;
            let mv = if j > 0 { dp_ref[t - 1][j - 1] + frame[tokens[j - 1]] } else { NEG_INF };
            let (best_ref, from) = [
                (stay_ref, RefFrom::StayRef),
                (from_filler, RefFrom::FromFiller),
                (mv, RefFrom::Move),
            ]
            .into_iter()
            .fold((NEG_INF, RefFrom::StayRef), |acc, cand| if cand.0 >= acc.0 { cand } else { acc });
            dp_ref[t][j] = best_ref;
            back_ref[t][j] = Some(from);

            // Filler cell.
            let stay_filler = dp_filler[t - 1][j] + filler_emit;
            let enter = dp_ref[t - 1][j] + filler_emit;
            let (best_filler, ffrom) = if stay_filler >= enter {
                (stay_filler, FillerFrom::StayFiller)
            } else {
                (enter, FillerFrom::EnterFromRef)
            };
            dp_filler[t][j] = best_filler;
            back_filler[t][j] = Some(ffrom);
        }
    }

    // Allow ending in either mode, so a trailing insertion after the last
    // reference word is representable too.
    let mut mode = if dp_ref[t_len][l_len] >= dp_filler[t_len][l_len] { Mode::Ref } else { Mode::Filler };

    let mut spans = vec![(usize::MAX, 0usize, 0.0f32, 0usize); l_len];
    // Reverse-chronological (frame_index, gap_index) pairs for filler frames;
    // reversed into chronological order and grouped into spans below.
    let mut filler_frames: Vec<(usize, usize)> = Vec::new();

    let mut t = t_len;
    let mut j = l_len;
    while t > 0 {
        let frame = &log_probs[t - 1];
        match mode {
            Mode::Ref => {
                let from = back_ref[t][j].unwrap();
                match from {
                    RefFrom::Move => {
                        let tok = tokens[j - 1];
                        let entry = &mut spans[j - 1];
                        entry.0 = entry.0.min(t - 1);
                        entry.1 = entry.1.max(t);
                        entry.2 += frame[tok].exp();
                        entry.3 += 1;
                        j -= 1;
                    }
                    RefFrom::StayRef | RefFrom::FromFiller => {
                        if j > 0 {
                            let tok = tokens[j - 1];
                            if frame[tok] >= frame[blank_id] {
                                let entry = &mut spans[j - 1];
                                entry.0 = entry.0.min(t - 1);
                                entry.1 = entry.1.max(t);
                                entry.2 += frame[tok].exp();
                                entry.3 += 1;
                            }
                        }
                        if from == RefFrom::FromFiller {
                            mode = Mode::Filler;
                        }
                    }
                }
            }
            Mode::Filler => {
                filler_frames.push((t - 1, j));
                let from = back_filler[t][j].unwrap();
                if from == FillerFrom::EnterFromRef {
                    mode = Mode::Ref;
                }
            }
        }
        t -= 1;
    }

    filler_frames.reverse();
    let mut filler_spans: Vec<FillerSpan> = Vec::new();
    for (frame_idx, gap_index) in filler_frames {
        match filler_spans.last_mut() {
            Some(last)
                if last.gap_index == gap_index
                    && frame_idx.saturating_sub(last.end_frame) <= MAX_FILLER_GAP_FRAMES =>
            {
                last.end_frame = frame_idx + 1;
            }
            _ => filler_spans.push(FillerSpan { gap_index, start_frame: frame_idx, end_frame: frame_idx + 1 }),
        }
    }

    Ok((
        spans
            .into_iter()
            .map(|(start, end, score_sum, count)| {
                let score = if count > 0 { score_sum / count as f32 } else { 0.0 };
                (start, end, score)
            })
            .collect(),
        filler_spans,
    ))
}
