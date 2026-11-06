use crate::model::Emissions;
use crate::transcript::{FilteredWord, Insertion, Word};
use anyhow::{Result, anyhow};
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
/// letting genuine inserted speech win. Tuned empirically against a reading
/// that says "Chapter" before a section numeral not in the text (should
/// detect "chapter"; `validation-samples/room-one*` now covers this case)
/// and the clean validation samples (should stay at zero insertions).
const FILLER_PENALTY: f32 = 1.5;

/// CTC emissions are "spiky" — even within a single real word, a letter's
/// probability spikes for 1-2 frames then dips back toward blank before the
/// next letter spikes. That's true for inserted speech too, so a strict
/// contiguous-frame requirement would fragment one insertion into several
/// too-short bursts. Bridge filler bursts separated by a gap this small
/// before applying [`MIN_FILLER_FRAMES`] — observed gaps within one inserted
/// word ("chapter") topped out around 3 frames on real audio.
const MAX_FILLER_GAP_FRAMES: usize = 4;

/// Minimum contiguous filler frames to report as an insertion, rather than
/// discarding as a short noise blip. wav2vec2-base's stride is ~20ms/frame;
/// 6 frames (~120ms) is short enough to catch a quick "Chapter"/"Section"
/// but long enough to reject the 1-2 frame consonant-boundary blips observed
/// on real audio.
const MIN_FILLER_FRAMES: usize = 6;

/// Log-space cost, charged once each time the filler state is entered, for
/// entering it anywhere but between two sentences, when the caller gives
/// sentence boundaries. Speech that isn't in the text (a preamble, an
/// announced chapter title) usually sits between sentences, but a short word
/// inside the inserted speech ("the", "to") can match a text word, putting
/// the rest of the insertion inside the sentence. Charged per entry, not per
/// frame, so a long insertion between sentences costs no more than before.
const MID_SENTENCE_FILLER_ENTRY_PENALTY: f32 = 3.0;

/// Taken off [`FILLER_PENALTY`] for filler before the first word, when the
/// caller gives sentence boundaries. A recording's lead-in (a title, a
/// LibriVox preamble) often says the opening words too; without this the
/// text's opening sentence can match either place at about the same cost.
/// The reading proper comes last, so this makes the lead-in take everything
/// up to the last match.
const LEAD_IN_FILLER_DISCOUNT: f32 = 0.5;

/// Viterbi forced alignment between CTC emissions and the reference text.
///
/// Frames that don't match the reference text (e.g. a reader-inserted
/// "Chapter" before a numeral) are routed into an optional filler state
/// instead of corrupting a neighboring word's span; see [`Insertion`].
///
/// `sentence_starts`, when given, holds the indexes (into
/// `text.split_whitespace()`) of the words that start a sentence; entering
/// filler anywhere else then costs [`MID_SENTENCE_FILLER_ENTRY_PENALTY`].
///
/// Returns the aligned words, words dropped before alignment because they
/// contained no characters in the wav2vec2 vocabulary, and detected
/// insertions.
pub fn viterbi_align(
    emissions: &Emissions,
    text: &str,
    audio_duration_secs: f32,
    sentence_starts: Option<&[usize]>,
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
    // Original index of each kept word.
    let mut kept_index: Vec<usize> = Vec::new();
    let words: Vec<&str> = text
        .split_whitespace()
        .enumerate()
        .filter_map(|(i, w)| {
            if w.to_uppercase()
                .chars()
                .any(|ch| vocab_map.contains_key(ch.to_string().as_str()))
            {
                kept_index.push(i);
                Some(w)
            } else {
                filtered.push(FilteredWord {
                    word: w.to_string(),
                    original_index: i,
                });
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
            tokens.push(CharToken {
                id: word_sep_id,
                word_idx: None,
            });
        }
        for ch in word.to_uppercase().chars() {
            if let Some(&id) = vocab_map.get(ch.to_string().as_str()) {
                tokens.push(CharToken {
                    id,
                    word_idx: Some(wi),
                });
            }
        }
    }
    let label_ids: Vec<usize> = tokens.iter().map(|t| t.id).collect();

    let lead_in_discount = if sentence_starts.is_some() {
        LEAD_IN_FILLER_DISCOUNT
    } else {
        0.0
    };
    let enter_penalty = filler_entry_penalties(&tokens, &kept_index, sentence_starts);

    let (frame_spans, filler_spans) = ctc_forced_align(
        &emissions.log_probs,
        &label_ids,
        blank_id,
        &enter_penalty,
        lead_in_discount,
    )?;

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
    for (word, span) in words.iter().zip(word_spans) {
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
            let decoded_text = decode_filler(
                &emissions.log_probs,
                &emissions.vocab,
                s.start_frame,
                s.end_frame,
                blank_id,
            );
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

/// The cost of entering filler at each column (tokens consumed): zero
/// everywhere without `sentence_starts` (original word indexes), and
/// otherwise [`MID_SENTENCE_FILLER_ENTRY_PENALTY`] except between sentences:
/// at either end, or beside a word separator whose next word starts a
/// sentence. `kept_index[k]` is the original index of kept word `k`.
fn filler_entry_penalties(
    tokens: &[CharToken],
    kept_index: &[usize],
    sentence_starts: Option<&[usize]>,
) -> Vec<f32> {
    let Some(starts) = sentence_starts else {
        return vec![0.0; tokens.len() + 1];
    };
    // A kept word starts a sentence if a sentence starts at it, or at
    // filtered words just before it.
    let mut starts_sentence = vec![false; kept_index.len()];
    for &s in starts {
        if let Some(k) = kept_index.iter().position(|&i| i >= s) {
            starts_sentence[k] = true;
        }
    }
    (0..=tokens.len())
        .map(|j| {
            let next_word = tokens[j..].iter().find_map(|t| t.word_idx);
            let at_gap = j == 0
                || j == tokens.len()
                || tokens[j - 1].word_idx.is_none()
                || tokens[j].word_idx.is_none();
            let between_sentences = match next_word {
                None => true,
                Some(w) => at_gap && starts_sentence[w],
            };
            if between_sentences {
                0.0
            } else {
                MID_SENTENCE_FILLER_ENTRY_PENALTY
            }
        })
        .collect()
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
    let score = if prob_count > 0 {
        prob_sum / prob_count as f64
    } else {
        0.0
    };
    (out.trim().to_lowercase(), score)
}

/// A reference token's frames: the first, one past the last, and its mean
/// probability over them.
type TokenSpan = (usize, usize, f32);

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

/// Both back-pointers of every `(t, j)` cell for frames `t` in `1..=t_len`,
/// two cells per byte, since they are the one table that grows with frames
/// × tokens. A cell takes 3 bits of its 4-bit half: [`RefFrom`] in bits 0–1
/// and [`FillerFrom`] in bit 2. Frame 0 has no back-pointers (the backtrace
/// stops before it), so it isn't stored.
struct BackPointers {
    bytes: Vec<u8>,
    width: usize,
}

impl BackPointers {
    fn new(t_len: usize, width: usize) -> Self {
        Self {
            bytes: vec![0; (t_len * width).div_ceil(2)],
            width,
        }
    }

    /// The byte holding cell `(t, j)` and the shift of its half (0 or 4).
    fn slot(&self, t: usize, j: usize) -> (usize, u32) {
        let i = (t - 1) * self.width + j;
        (i / 2, (i % 2) as u32 * 4)
    }

    /// Records cell `(t, j)`. Each cell is set once, and every half starts
    /// at zero, so OR-ing it in is enough.
    fn set(&mut self, t: usize, j: usize, from: RefFrom, ffrom: FillerFrom) {
        let code = match from {
            RefFrom::StayRef => 0,
            RefFrom::FromFiller => 1,
            RefFrom::Move => 2,
        } | match ffrom {
            FillerFrom::StayFiller => 0,
            FillerFrom::EnterFromRef => 4,
        };
        let (byte, shift) = self.slot(t, j);
        self.bytes[byte] |= code << shift;
    }

    fn code(&self, t: usize, j: usize) -> u8 {
        let (byte, shift) = self.slot(t, j);
        (self.bytes[byte] >> shift) & 0xF
    }

    fn ref_from(&self, t: usize, j: usize) -> RefFrom {
        match self.code(t, j) & 0b11 {
            0 => RefFrom::StayRef,
            1 => RefFrom::FromFiller,
            _ => RefFrom::Move,
        }
    }

    fn filler_from(&self, t: usize, j: usize) -> FillerFrom {
        if self.code(t, j) & 0b100 == 0 {
            FillerFrom::StayFiller
        } else {
            FillerFrom::EnterFromRef
        }
    }
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
/// `enter_penalty[j]` is subtracted each time filler is entered at column
/// `j`, and `lead_in_discount` is added to each filler frame at column 0.
///
/// Returns per-token frame spans (as before) plus detected filler spans.
fn ctc_forced_align(
    log_probs: &[Vec<f32>],
    tokens: &[usize],
    blank_id: usize,
    enter_penalty: &[f32],
    lead_in_discount: f32,
) -> Result<(Vec<TokenSpan>, Vec<FillerSpan>)> {
    let t_len = log_probs.len();
    let l_len = tokens.len();
    if t_len < l_len {
        return Err(anyhow!(
            "audio too short ({t_len} frames) to align {l_len} characters"
        ));
    }

    const NEG_INF: f32 = f32::NEG_INFINITY;

    // Scores are kept for two frames only (`prev_*` for t - 1, `cur_*` for
    // t): a cell reads nothing older than the frame before it, and the
    // backtrace needs only the last frame's final scores. Back-pointers are
    // kept for every frame, since the backtrace walks all of them.
    let width = l_len + 1;
    let mut prev_ref = vec![NEG_INF; width];
    let mut prev_filler = vec![NEG_INF; width];
    let mut cur_ref = vec![NEG_INF; width];
    let mut cur_filler = vec![NEG_INF; width];
    let mut back = BackPointers::new(t_len, width);
    prev_ref[0] = 0.0;

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
            let stay_ref = prev_ref[j] + stay_emit;
            let from_filler = prev_filler[j] + stay_emit;
            let mv = if j > 0 {
                prev_ref[j - 1] + frame[tokens[j - 1]]
            } else {
                NEG_INF
            };
            let (best_ref, from) = [
                (stay_ref, RefFrom::StayRef),
                (from_filler, RefFrom::FromFiller),
                (mv, RefFrom::Move),
            ]
            .into_iter()
            .fold((NEG_INF, RefFrom::StayRef), |acc, cand| {
                if cand.0 >= acc.0 { cand } else { acc }
            });
            cur_ref[j] = best_ref;

            // Filler cell.
            let filler_emit = if j == 0 {
                filler_emit + lead_in_discount
            } else {
                filler_emit
            };
            let stay_filler = prev_filler[j] + filler_emit;
            let enter = prev_ref[j] + filler_emit - enter_penalty[j];
            let (best_filler, ffrom) = if stay_filler >= enter {
                (stay_filler, FillerFrom::StayFiller)
            } else {
                (enter, FillerFrom::EnterFromRef)
            };
            cur_filler[j] = best_filler;
            back.set(t, j, from, ffrom);
        }
        std::mem::swap(&mut prev_ref, &mut cur_ref);
        std::mem::swap(&mut prev_filler, &mut cur_filler);
    }

    // Allow ending in either mode, so a trailing insertion after the last
    // reference word is representable too. After the last swap, `prev_*`
    // holds frame `t_len`.
    let mut mode = if prev_ref[l_len] >= prev_filler[l_len] {
        Mode::Ref
    } else {
        Mode::Filler
    };

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
                let from = back.ref_from(t, j);
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
                            // Filler is entered and left at the same column,
                            // so the token before it is the one the stay
                            // frames since then (later in time) were given
                            // to. Its own frames are the ones before the
                            // filler, ending at its Move frame; the later
                            // ones are only its letter recurring in the
                            // inserted speech, or in the stretch after it,
                            // and would stretch the token over the
                            // insertion. Drop them.
                            if j > 0 {
                                spans[j - 1] = (usize::MAX, 0, 0.0, 0);
                            }
                            mode = Mode::Filler;
                        }
                    }
                }
            }
            Mode::Filler => {
                filler_frames.push((t - 1, j));
                let from = back.filler_from(t, j);
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
            _ => filler_spans.push(FillerSpan {
                gap_index,
                start_frame: frame_idx,
                end_frame: frame_idx + 1,
            }),
        }
    }

    Ok((
        spans
            .into_iter()
            .map(|(start, end, score_sum, count)| {
                let score = if count > 0 {
                    score_sum / count as f32
                } else {
                    0.0
                };
                (start, end, score)
            })
            .collect(),
        filler_spans,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_pointers_round_trip() {
        // An odd width puts cells of one frame in both halves of a byte, and
        // an odd cell count leaves the last byte half-used.
        let refs = [RefFrom::StayRef, RefFrom::FromFiller, RefFrom::Move];
        let fillers = [FillerFrom::StayFiller, FillerFrom::EnterFromRef];
        let combo = |t: usize, j: usize| (refs[(t + j) % 3], fillers[(t * 7 + j) % 2]);
        let (t_len, width) = (5, 7);
        let mut back = BackPointers::new(t_len, width);
        for t in 1..=t_len {
            for j in 0..width {
                let (r, f) = combo(t, j);
                back.set(t, j, r, f);
            }
        }
        for t in 1..=t_len {
            for j in 0..width {
                let (r, f) = combo(t, j);
                assert!(back.ref_from(t, j) == r && back.filler_from(t, j) == f);
            }
        }
    }

    #[test]
    fn filler_entry_is_free_only_between_sentences() {
        // "ab cd ef": tokens A B | C D | E F. Sentences start at words 0
        // and 2; word 1 ("cd") is mid-sentence.
        let tok = |word_idx: Option<usize>| CharToken { id: 0, word_idx };
        let tokens = [
            tok(Some(0)),
            tok(Some(0)),
            tok(None),
            tok(Some(1)),
            tok(Some(1)),
            tok(None),
            tok(Some(2)),
            tok(Some(2)),
        ];
        let p = MID_SENTENCE_FILLER_ENTRY_PENALTY;
        assert_eq!(
            filler_entry_penalties(&tokens, &[0, 1, 2], None),
            vec![0.0; 9]
        );
        assert_eq!(
            filler_entry_penalties(&tokens, &[0, 1, 2], Some(&[0, 2])),
            // Columns: before A, in "ab", before |, after | (before "cd"),
            // in "cd", before |, after | (before "ef"), in "ef", the end.
            vec![0.0, p, p, p, p, 0.0, 0.0, p, 0.0]
        );
        // A sentence that starts at a filtered word (original index 2,
        // dropped) starts at the next kept word.
        assert_eq!(
            filler_entry_penalties(&tokens, &[0, 1, 3], Some(&[0, 2]))[5..7],
            [0.0, 0.0]
        );
    }
}
