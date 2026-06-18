# Alignment Eval Notes

## Test 1 — Truncation detection (`short-sentence-added-at-the-end.json`)

**Audio**: original `short-sentence.mp3`
**Text**: `short-sentence.md` + " at the end" appended (words not in audio)

**Result**: 0 filtered, 2 suspect (Truncated)
- `"at"` score=0.001
- `"end"` score=0.000

`"the"` (also not in audio) scored 0.658 — **false negative**. Silence at the
end of the audio happened to produce CTC emissions where T/H/E letters scored
well by chance.

`"footnotes"` (last real word) scored 0.361 — below "the" which isn't in the
audio at all. Trailing silence appears to depress scores for the final real word.

**Takeaway**: truncation is detectable but noisy at the word level. Individual
false negatives and false positives are possible. Useful as a flag before
listening, not as a definitive diagnosis.

---

## Test 2 — Clean segment (`authorship_seg10`, 2026-06-18)

**Audio**: `vibe/data/2026-06-18 9-30a/authorship_seg10_generated.wav`
**Text**: `vibe/data/2026-06-18 9-30a/authorship_seg10.txt` (3 paragraphs, 252 words)

**Result**: 3 filtered, 4 suspect (all LowScore, none Truncated)

Suspect words:
- `"Speaker"` ×3 — score 0.000, 0.000, 0.143 — paragraph-label prefix, not spoken
- `"Or,"` — score 0.000 — first word of a paragraph; comma plus paragraph-boundary
  silence likely depresses score

Other low-scoring words (below 0.5, above threshold):
- `"(e.g.,"` 0.374 — normalizer expands this to "for example", so the spoken
  audio doesn't match the raw text token; expected to disappear once normalized
  text is used
- `"'(#s"` 0.499, `"and"` 0.482 — borderline, may improve with normalized text

**Takeaway**: on a clean segment the only sub-threshold words are the `Speaker N:`
prefixes (not spoken — they are VibeVoice speaker directives) and tokens the
normalizer transforms before synthesis. Content words all score above 0.3. Two
fixes needed before the signal is clean:

**Action items**:
1. **Normalize text before alignment** — pass text through the same normalizer
   used for synthesis so the aligner sees "for example" not "e.g.", etc.
2. **Strip `Speaker N:` prefixes** — required by VibeVoice but not spoken; the
   normalizer does not remove them, so this needs a separate pre-strip step
   before alignment.

## Test 3 — Clean segment, normalized text (`authorship_seg10_normalized.txt`)

Same wav as Test 2, normalized text input (267 words after normalization).

**Result**: 7 filtered, 4 suspect — identical suspect list to Test 2:
- `"Speaker"` ×3 — unchanged, normalizer doesn't strip them
- `"Or,"` — unchanged

`"(e.g.,"` is gone — normalizer expanded it to "for example" which scores fine.
No new suspects introduced by normalization.

**Takeaway**: with `Speaker N:` prefixes stripped, this segment would have only
`"Or,"` flagged. That's likely a punctuation artifact — the comma is part of the
token but invisible to the CTC model, so "Or" scores as if it were an unknown
token. A pre-tokenization punctuation strip on the alignment input would fix it.

**Revised action items**:
1. Strip `Speaker N:` prefixes before alignment.
2. Strip punctuation from word tokens before alignment (commas, quotes, parens)
   — the CTC vocab doesn't include them and they deflate scores for otherwise
   clean words like `"Or,"`.
3. Use normalized text (already confirmed to help).

## Test 4 — Clean segment, fully preprocessed (`authorship_seg10_align_input.txt`)

Same wav, preprocessed input: normalized → `Speaker N:` stripped → leading/trailing
punctuation stripped per token.

**Result**: 4 filtered, **0 suspect words**. Lowest scoring content word: `"and"` at
0.482 — well above the 0.3 threshold.

**Confirmed preprocessing pipeline** for alignment input:
1. Normalize text (same pass used for synthesis)
2. Strip `Speaker N:` prefix lines
3. Strip leading/trailing punctuation from each whitespace-delimited token

With this pipeline, a clean segment returns zero suspects. Any suspect words in
future runs are a genuine signal worth investigating before or during listening.
