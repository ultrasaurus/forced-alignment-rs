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
- **`Truncated`** — low confidence AND start time in the final 10% of audio
  duration; strong signal the audio ended before the text did

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

The 10% boundary is a starting point — tune after more listen-test runs.
The signal is reliable for a run of truncated tail words but individual
false negatives are possible: a word in silence can score well by chance if
the acoustic noise happens to activate its letter tokens (observed: `"the"`
scoring 0.658 in silence after audio end).
