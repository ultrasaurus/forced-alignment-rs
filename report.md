# AlignReport

`align()` returns `(Transcript, AlignReport)`. The report surfaces three
categories of anomaly without any extra overhead — all are byproducts of the
Viterbi pass that already runs.

See `src/transcript.rs` and `src/lib.rs` for full API docs (`cargo doc --open`).

## Filtered words

Words dropped before alignment because they contain no characters in the
wav2vec2 vocabulary (e.g. punctuation-only tokens, markdown artifacts like
`##`). Recorded with their position in the original whitespace-split sequence
so callers can splice `[word]` annotations back into context.

## Suspect words

Words aligned with a shape worth reviewing — either low confidence, or a
duration that doesn't fit the segment's pace even though the score looks
fine. Three sub-classes:

- **`LowScore`** — low confidence (`score < 0.3`) anywhere in the audio;
  could be a mispronunciation, hallucinated silence, or a preprocessing
  mismatch
- **`Truncated`** — belongs to a trailing run of at least
  `MIN_TRUNCATION_RUN_WORDS` consecutive words, ending at the last word, that
  are both low confidence and pace-collapsed relative to the segment's median
  (see "Truncation heuristic" below); strong signal the audio ended before
  the text did. A single late low-score word without the pace collapse is
  `LowScore` instead.
- **`AnomalousDuration`** — duration far exceeds the segment's typical
  per-character pace (`ANOMALOUS_DURATION_RATIO`, 4x the median) or an
  absolute cap (`ANOMALOUS_DURATION_ABS_SECS`, 1s), even though the score is
  *above* threshold — signals that extra audio (e.g. a leaked fragment) got
  absorbed into this word's span rather than being left unaligned. The
  opposite failure shape from `Truncated`'s pace *collapse*.

Scores are mean CTC token probabilities in `[0.0, 1.0]`. Clean speech scores
`0.8` and above; forced/truncated words score near `0.0`. A word can be
confidently scored and still wrong — see "Known limitations" below.

## Insertions

Speech found in the audio that isn't in the reference text at all — e.g. a
reader saying "Chapter" before a numeral, or a TTS engine leaking a fragment
of another clip. Detected via an optional "filler" state in the same Viterbi
pass: frames that don't match blank *or* the current/next reference token get
routed there instead of corrupting a neighboring word's span or score.

Each `Insertion` carries:

- `before_word_index` — index of the word this insertion precedes (equal to
  `words.len()` for an insertion trailing the last word)
- `start`/`end` — timestamps in seconds
- `decoded_text` — a best-effort, **unconstrained greedy CTC decode** of the
  filler frames. Low confidence by construction (no reference text to
  constrain the decode against) — treat it as a diagnostic hint for a human
  to eyeball or fuzzy-match against a reference corpus, not a trustworthy
  transcript. A real, audible leak reliably produces *an* `Insertion`, but
  `decoded_text` itself can be badly garbled even when the detection is
  correct.
- `score` — mean probability of the decoded characters; not comparable to a
  `Word`'s or `SuspectWord`'s score, since those score confidence in a
  *known* word and this scores confidence in an unconstrained decode of
  unknown content.

Insertions and suspects are independent signals — a segment can have either,
both, or neither. A small localized insertion inside an otherwise-clean
sentence, and a sentence whose content is *substantially* wrong throughout,
have different failure shapes and aren't caught by the same mechanism (see
"Known limitations").

## Annotated output format

Callers can render the report inline with the original text, per sentence:

```
the author's [##] *contribution* {chapter} to the work
```

- `[word]` — filtered (dropped before alignment)
- `*word*` — suspect (aligned but low-confidence)
- `{word}` — insertion (audio present that isn't in the reference text),
  spliced in before the word it precedes, using `decoded_text`

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

## Known limitations

Forced alignment assumes rough correspondence between the audio and the
reference text and finds *when* each word was spoken — it has no way to
express "none of this audio belongs to any of these words." Found via real
production artifacts (see `artifacts/theory.md` for the full investigation):

- **A wrong match can be confident, not just low-scoring.** The Viterbi DP
  always assigns every reference word *some* span, however badly it actually
  matches — a short/ambiguous word can force-match onto completely unrelated
  audio and still score above the suspect threshold. No score-based signal
  catches this by construction.
- **This can happen at whole-sentence scale.** Two different, similar-length,
  ordinary sentences can coincidentally force-align well throughout purely
  from common-word overlap (mean CTC score well above 0.7 across nearly every
  word), without the content actually being right. `Insertion` won't catch
  this either — the audio isn't a filler burst inside otherwise-correct
  speech, it's wrong throughout, so there's no localized anomaly for either
  mechanism to key on.
- **A sentence whose content is substantially or entirely wrong throughout
  produces no flaggable signal at all** — not `Truncated` (no trailing pace
  collapse), not `Insertion` (no isolated burst distinct from the
  surrounding low-confidence frames), just uniformly low scores that don't
  fit either detector's shape. Confirmed as a real, silent miss in
  production.

The one thing that reliably catches whole-sentence content mismatches is
comparing an independent free transcription of the audio against the
reference text — a fundamentally different signal ("what did the audio
actually say") from anything forced alignment itself can produce. Not part
of this crate — see `artifacts/asr-engine-comparison.md` and
`artifacts/theory.md`'s "Free-transcription QA" section for the
investigation and what was tried.
