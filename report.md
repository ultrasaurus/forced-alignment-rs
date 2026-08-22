# AlignReport

`align()` returns `(Transcript, AlignReport)`. The report surfaces two
categories of anomaly without any extra overhead — both are byproducts of the
Viterbi pass that already runs.

See `src/transcript.rs` and `src/lib.rs` for full API docs (`cargo doc --open`).

## Filtered words

Words dropped before alignment because they contain no characters in the
wav2vec2 vocabulary (e.g. punctuation-only tokens, markdown artifacts like
`##`). Recorded with their position in the original whitespace-split sequence
so callers can splice `[word]` annotations back into context.

## Suspect words

Words aligned with low confidence (`score < 0.3`). Two sub-classes:

- **`LowScore`** — low confidence anywhere in the audio; could be a
  mispronunciation, hallucinated silence, or a preprocessing mismatch
- **`Truncated`** — belongs to a trailing run of at least
  `MIN_TRUNCATION_RUN_WORDS` consecutive words, ending at the last word, that
  are both low confidence and pace-collapsed relative to the segment's median
  (see "Truncation heuristic" below); strong signal the audio ended before
  the text did. A single late low-score word without the pace collapse is
  `LowScore` instead.

Scores are mean CTC token probabilities in `[0.0, 1.0]`. Clean speech scores
`0.8` and above; forced/truncated words score near `0.0`.

## Annotated output format

Callers can render the report inline with the original text, per sentence:

```
the author's [##] *contribution* to the work
```

- `[word]` — filtered (dropped before alignment)
- `*word*` — suspect (aligned but low-confidence)

Sentence splitting uses `unicode_segmentation::UnicodeSegmentation::unicode_sentences`
for multilingual correctness.

## Input preprocessing

Scores are only meaningful when the alignment input matches what was actually
spoken. Required steps (confirmed by eval — see `align-report-eval/`):

1. **Normalize** — same pass used for synthesis; e.g. "for example" not "e.g."
2. **Strip speaker directives** — e.g. `Speaker 1:` prefixes are not spoken
3. **Strip punctuation** — leading/trailing punctuation per token; the CTC vocab
   contains only letters and `|`, so punctuation deflates scores for clean words

With these three steps, a known-clean TTS segment returns **zero suspect words**
(lowest content-word score: 0.482). Without them, the `Speaker N:` prefixes and
punctuation-heavy tokens generate spurious suspects.

## Truncation heuristic

Originally: any low-score word starting in the final 10% of audio duration.
That produced false positives — an isolated low-scoring word late in a long
file (e.g. a short function word like "he" or "a") isn't evidence of
truncation on its own; CTC scores dip for lots of unrelated reasons.

Current heuristic instead looks for the DP's actual truncation signature: when
it runs out of audio before it runs out of text, it doesn't just lower
confidence on the last word — it crushes the *remaining* words' durations
toward zero, since they're all forced into whatever frames are left. So
`Truncated` requires a trailing run (ending at the last word, length >=
`MIN_TRUNCATION_RUN_WORDS`) that is both low-score AND pace-collapsed
(per-character duration well below the segment's median — see
`TRUNCATION_PACE_RATIO`). A single bad word without the collapse falls back to
`LowScore`.

False negatives are still possible: a word in silence can score well by
chance if the acoustic noise happens to activate its letter tokens (observed:
`"the"` scoring 0.658 in silence after audio end). Because the run is found by
scanning backward from the last word and stopping at the first word that
doesn't qualify, one such well-scoring word in the middle of an otherwise
collapsed tail can truncate (pun intended) the detected run short of the real
extent — a tradeoff against the false positives the old heuristic produced.
