use crate::model::Emissions;
use crate::transcript::{FilteredWord, Word};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// One character of the normalized transcript, mapped to a vocab token id.
struct CharToken {
    id: usize,
    /// Index into the original word list this character belongs to, or None for the
    /// word-delimiter token ("|").
    word_idx: Option<usize>,
}

/// Viterbi forced alignment between CTC emissions and the reference text.
///
/// Returns the aligned words and a list of words that were dropped before
/// alignment because they contained no characters in the wav2vec2 vocabulary.
pub fn viterbi_align(
    emissions: &Emissions,
    text: &str,
    audio_duration_secs: f32,
) -> Result<(Vec<Word>, Vec<FilteredWord>)> {
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
        return Ok((vec![], filtered));
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

    let frame_spans = ctc_forced_align(&emissions.log_probs, &label_ids, blank_id)?;

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
    Ok((out, filtered))
}

/// Forced alignment via dynamic programming over (frame, token) states.
///
/// Returns, for each token, the (start_frame, end_frame_exclusive, avg_prob) span of
/// frames assigned to it.
fn ctc_forced_align(
    log_probs: &[Vec<f32>],
    tokens: &[usize],
    blank_id: usize,
) -> Result<Vec<(usize, usize, f32)>> {
    let t_len = log_probs.len();
    let l_len = tokens.len();
    if t_len < l_len {
        return Err(anyhow!(
            "audio too short ({t_len} frames) to align {l_len} characters"
        ));
    }

    const NEG_INF: f32 = f32::NEG_INFINITY;
    // dp[t][j] = best log-prob using first t frames with j tokens consumed.
    let mut dp = vec![vec![NEG_INF; l_len + 1]; t_len + 1];
    dp[0][0] = 0.0;
    for t in 1..=t_len {
        dp[t][0] = dp[t - 1][0] + log_probs[t - 1][blank_id];
    }

    for t in 1..=t_len {
        let frame = &log_probs[t - 1];
        for j in 1..=l_len {
            let tok = tokens[j - 1];
            let stay_emit = frame[blank_id].max(frame[tok]);
            let stay = dp[t - 1][j] + stay_emit;
            let mv = dp[t - 1][j - 1] + frame[tok];
            dp[t][j] = stay.max(mv);
        }
    }

    // Backtrace.
    let mut spans = vec![(usize::MAX, 0usize, 0.0f32, 0usize); l_len];
    let mut t = t_len;
    let mut j = l_len;
    while t > 0 {
        if j == 0 {
            t -= 1;
            continue;
        }
        let frame = &log_probs[t - 1];
        let tok = tokens[j - 1];
        let mv = dp[t - 1][j - 1] + frame[tok];
        let stay = dp[t - 1][j] + frame[blank_id].max(frame[tok]);
        let from_move = mv >= stay;
        let assign_to_token = from_move || frame[tok] >= frame[blank_id];

        if assign_to_token {
            let entry = &mut spans[j - 1];
            entry.0 = entry.0.min(t - 1);
            entry.1 = entry.1.max(t);
            entry.2 += frame[tok].exp();
            entry.3 += 1;
        }
        t -= 1;
        if from_move {
            j -= 1;
        }
    }

    Ok(spans
        .into_iter()
        .map(|(start, end, score_sum, count)| {
            let score = if count > 0 { score_sum / count as f32 } else { 0.0 };
            (start, end, score)
        })
        .collect())
}
