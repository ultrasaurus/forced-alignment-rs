# TTS artifact theory — reference

Working explanation for the leaked/wrong-audio artifacts found via
`AlignReport::insertions` and, later, free transcription. See
`findings-table.md` for the raw per-sentence data this is built from, and
the per-incident `artifacts/*/notes.md` directories for full detail on
individual cases.

## Summary

Four distinct failure mechanisms have been found under the umbrella of
"forced-alignment flagged this sentence's audio as suspect," not one:

1. **Reference-clip bleed** — a fragment of the voice's own cloning
   reference audio leaks into the generated output.
2. **Model hallucination** — the model invents content that isn't in the
   reference text and isn't traceable to any real source, ranging from
   register-flavored nonsense to a verbatim repeat of a *recent*
   utterance.
3. **Batch/pipeline misrouting** — weaker evidence than 1 and 2; mostly
   superseded by mechanism 2 (see below), kept for completeness.
4. **TTS synthesis truncation** — the TTS engine stops generating partway
   through a sentence; the audio just ends early into real trailing
   silence. Found via free transcription, not by ear or by forced
   alignment — see "Mechanism 4" below.

These aren't competing explanations for one root cause — they're
different things the same pipeline does under different conditions, and
should be reasoned about separately, not collapsed into a single "the
bug is X" story.

## Mechanism 1: reference-clip bleed

**Confirmed structurally, 5-for-5 across every short heading-only
sentence checked** (`"Note"`, `"1.3.2."`, `"Basic properties of waves"`,
`"Example (Pulse train)"`, `"Definition 1.2 (Fundamental frequency)"`).
With almost no real target text to anchor generation, the voice's
reference-clip conditioning proportionally dominates more of the output.

- Confirmed by ear and by independent Parakeet transcription: sentence
  12's audio ("Note") is `"do you want me"` — an exact fragment of
  Jack's reference clip (`odoru/vibe/voices/Jack/ref.wav`, transcript
  *"Do you want me reading like a specific line of text or will this
  do?"*). Same clip's text also explains `"you wa"`, `"re"`
  (→ "reading"), `"stesc"` (→ "specific").
- Severity scales with how little real target text there is to compete
  with the reference conditioning — a one-word sentence can be *almost
  entirely* reference-clip audio (sentence 12: 1.71s clip, the aligned
  "Note" word itself force-matched onto reference-clip audio, scoring a
  confident 0.74 despite being completely wrong content — see "detection
  gaps" below).
- Not confined to headings by definition, just reliably triggered by
  them — short/sparse target text is the enabling condition, not
  "heading" per se.

## Mechanism 2: model hallucination

Two sub-shapes, both confirmed:

- **Invented, register-flavored nonsense.** Sentence 14's audio: *"Mote
  by mechanical notation standard conventions..."* — not real speech
  from any identifiable source, but built from vocabulary that fits the
  document's technical register (confirmed both by ear and by Parakeet's
  independent transcription, which also caught a reference-clip
  fragment fused into the same clip — mechanisms 1 and 2 aren't mutually
  exclusive within one clip).
- **Verbatim repeat of recent content.** Sentence 21's text is `"What is
  a signal?"`; the audio evokes sentence 15's sentence (`"This is not
  the same thing as being easy."`) blended with fragments of sentence 16
  (`"many"`). Originally read as a literal misrouted/duplicated audio
  file (sentence 15's clip reused under sentence 21's slot) — **ruled
  out**: sentence 21's clip is 2.31s, sentence 15's is 6.66s, not the
  same file. Better explained as **repetition/attention collapse**, a
  known autoregressive-generation failure mode where the model
  reproduces something it recently generated instead of the current
  target — a fresh, differently-timed generation that *echoes* recent
  content, not a pipeline bug reusing a stored file.

Mechanism 3 (misrouting) was the original explanation for the sentence
21 case, before the duration mismatch ruled out a literal file swap in
favor of the collapse explanation above — see "Mechanism 3" for why it's
downgraded rather than deleted.

## Mechanism 3: batch/pipeline misrouting (weak, mostly superseded)

Originally the leading theory after the sentence 21 case looked like a
whole correct sentence's audio filed under the wrong text — a plausible
indexing/routing bug in the batch pipeline between vibe-service's
response and odoru's per-sentence caching. The duration-mismatch finding
above better explains that specific case as mechanism 2's repetition
collapse instead. Not fully ruled out as a contributing factor elsewhere
(a real routing bug and a hallucination-collapse failure aren't mutually
exclusive), but there's no case in the current data that specifically
requires it over mechanism 2.

## Correlation with content type

Numeric/math/notation-heavy sentences show up disproportionately among
the artifacts (the physics-of-sound/DSP textbook corpus this was first
investigated on is full of them: `x of t`, `theta`, headings like
"1.3.2.", unit/symbol notation). Two non-exclusive explanations:

1. **Normalization-side**: unusual notation could produce oddly-shaped
   normalized text feeding the TTS call, which — if batch buffer
   sizes/chunking depend even loosely on input shape — could make
   whichever failure mode more likely for those specific sentences.
2. **Model-side difficulty**: unusual vocabulary may simply be harder
   for the TTS model to render well, independent of any pipeline issue
   — mechanisms 1 and 2 could both fire more often on this content
   without a routing bug being involved at all.

A second, harder-technical-content document (legal/citation-heavy, not
math-heavy) also produced free-transcription noise, but of a different
character — see "Free-transcription QA" below; that document's problems
turned out to be dominated by Parakeet's own ASR limitations on
acronyms/citations, not the artifact mechanisms above.

## Detection gaps in forced-alignment's own scoring

Independent of *why* these artifacts happen, the investigation surfaced
real, specific blind spots in what per-word CTC scoring can detect:

- **"Confidently wrong" matches.** The Viterbi DP never leaves a
  reference word unassigned — it always force-matches *something*,
  however badly. A short/ambiguous word can force-match onto unrelated
  audio and still score confidently (sentence 12's "Note" scored 0.74
  while being entirely reference-clip audio). No score threshold catches
  this category by construction.
- **Whole-sentence wrongness hides as pervasive low scores, not a
  flaggable shape.** If most/all of a sentence's audio is wrong, the DP
  still assigns spans to every word — it just scores them all badly. The
  `Insertion` mechanism only catches an *isolated* filler burst; a
  uniformly-bad sentence has no such burst for it to key on. Confirmed
  as a real, silent miss on sentence 13 (`missing-flag-basic-ideas`) and
  sentence 21 — both fully unflagged despite being genuinely bad.
- **A confidently-wrong match can also happen at full-sentence scale.**
  Sentence 15 was originally treated as a clean catch (good scores
  0.75-0.9999 throughout, one small flagged insertion) — Parakeet later
  revealed the entire sentence's real content has nothing to do with its
  reference text. Two unrelated, similar-length, ordinary-English
  sentences can coincidentally force-align well throughout purely from
  common-word overlap, without the content actually being right.
- **A flagged `Insertion`'s `decoded_text` can be a real (if garbled)
  transcription of genuinely leaked audio, not a false-positive
  misdecode.** Originally theorized (as "Class 1b") that fragments like
  `"sig"`/`"frequenc"`/`"prst"` were low-confidence CTC misdecodes of a
  real word already in the sentence (e.g. "sig" ≈ "signal"), meaning no
  real extra audio existed. Parakeet disproved this: each case is a
  genuine garbled prefix — a real, audible leak — that happens to
  precede the sentence's otherwise-correct content, up to and including
  one case where a prefix fused the *entire reference clip* and the
  *previous sentence's full heading* before the real content began. The
  original `Insertion` detection was more correct than the "Class 1b"
  theory gave it credit for; the confusion was purely about
  `decoded_text`'s reliability, not the detection itself.
- **A small localized insertion inside an otherwise-perfect sentence is
  invisible to free-transcription overlap scoring, the mirror-image
  gap.** `Insertion` catches exactly this case (a few garbled words like
  `"*"` → `"ir"`, or a stray `"point home"` appended) that free
  transcription's word-overlap check misses entirely, since every
  reference word still shows up somewhere in the transcription. Neither
  detection method subsumes the other — see "Free-transcription QA."

There's also a distinct, non-mystery category worth naming: **expected,
working-as-designed catches** — a short heading read aloud plus a small
extra bit, or the TTS naturally expanding written form into spoken words
that add something not literally in the text (the original motivating
use case, same as a human reader saying "Chapter" before a number).
These aren't artifacts at all.

## Free-transcription QA

A CPU-only free transcription pass, compared against normalized
reference text, was prototyped via an exploratory CLI tool
(`dl transcribe-check <doc_id> --voice <voice>` in odoru's `cli` crate)
— not wired into the live `align_warnings` pipeline yet. First built
against Parakeet, since retired — see `asr-engine-comparison.md` for
engine-specific findings and the current WhisperX direction. The
methodology below (what to compare against, how to tokenize/score) is
engine-agnostic and still applies.

**Validated on the physics-of-sound corpus**: 83 sentences, clean
separation between real problems and clean audio (worst clean score
0.89 after fixing normalization false positives — see below). Directly
confirmed the hardest-to-prove claim in this whole investigation
(sentence 13's silent miss) and found genuine new problems forced-
alignment's own signals had missed entirely. Combined with forced-
alignment's own `Insertion` detection, catches strictly more than either
method alone (10 sentences via transcription-overlap, 9 via `Insertion`,
overlapping on 5 — 3 catches are `Insertion`-only, 5 are transcription-
only).

**Comparison methodology and its known limitations:**

- Compares Parakeet's transcription against the *normalized* reference
  text (what was actually sent to TTS, e.g. "1.1." → "One point one."),
  not raw document text — matches forced-alignment's own documented
  input-preprocessing requirement.
- Word-level overlap (fraction of reference words found anywhere in the
  transcription, order-insensitive), tokenized on any non-alphanumeric
  boundary (not just whitespace — a comma/paren-joined cluster like
  `"D,xxx,bb)?"` needs splitting there too, or it collapses into one
  unmatchable token).
- Acronyms and number-sequences are exempted from the required-match set
  entirely, rather than fuzzy-matched — Parakeet's own transcription of
  these is unreliable in ways unrelated to whether the audio is correct
  (`"SID"` heard as one word `"sid"` instead of spelled-out letters,
  `"AFIPS"` heard as `"a phipps"`). Deliberately trades detection power
  on acronym/number-only errors for much lower noise everywhere else —
  the right trade if this is ever used to gate automated re-synth, where
  false positives are far more costly than an occasional missed
  acronym-only error.
- A naive "give Parakeet more surrounding context" test (concatenating
  adjacent cached sentence clips) did **not** improve two confirmed-real
  Parakeet-side transcription errors, and made one worse — likely a
  splice artifact from concatenating separately-cached clips rather than
  evidence against a real continuous-segment-audio approach, but no
  positive signal for restructuring the pipeline around segment-level
  (pre-slice) transcription either. Not pursued further without a
  cleaner test.

## Mechanism 4: TTS synthesis truncation (found via free transcription)

A citation-heavy sentence's real audio genuinely stops partway through —
confirmed by ear, and independently by *both* Parakeet and WhisperX
transcribing identically up to the same point (`"...Montvale, New
Jersey."`) and no further, despite the cached clip's buffer running
16.41s (per-word data shows the trailing ~5s isn't a short buffer forcing
compression — the remaining unspoken reference words get spread across
real trailing silence at normal-to-elongated pace, not crushed). The TTS
engine stopped generating before finishing the sentence; this is a real
synthesis bug, not an ASR limitation. (Earlier framing in this doc
mischaracterized this as "Parakeet transcription truncation, a
concerning limitation" — corrected: the ASR engines were both doing
exactly the right thing, reporting where real speech actually ended.)

**Confirmed as another silent miss for forced-alignment's own
detectors**: `vibe_sync.json` has no `align_warnings` at all for this
document, so neither `Truncated` nor `Insertion` caught it. Root cause is
now precisely understood — `Truncated` requires the DP to be forced into
*compressing* remaining words into a too-small leftover frame budget; TTS
truncation with a full trailing-silence buffer produces low-scoring words
at *normal or longer* duration instead, which doesn't match `Truncated`'s
pace-collapse signature at all. This is the concrete, now-understood
version of the "third, coarser signal" gap this doc has been circling —
a run of low-confidence words with no isolable anomaly and no pace
collapse either.

**A cheap, reference-text-free detection heuristic falls out of this
directly**: compare the free transcription's last recognized word's end
time against the clip's total duration. A large unexplained gap is
itself a truncation signal, without needing word-overlap comparison
against reference text at all.

## Engine choice: Parakeet retired, WhisperX current direction

See `asr-engine-comparison.md` for the full head-to-head findings
(timestamps, confidence, acronym handling) and why Parakeet was retired
in favor of WhisperX. Short version: three of four Parakeet weaknesses
found in this investigation are solved by WhisperX; the fourth
(sentence-332 truncation) turned out not to be an engine weakness at all
— see mechanism 4 above.

## Open items

- **`decoded_text` reliability**: forced-alignment's own `Insertion`
  decode is an unconstrained greedy CTC read and is often garbled
  (`"gemi"`, `"prst"`, `"anhor"`) — a natural, not-yet-built improvement
  is decoding just the flagged filler span with an external ASR (Parakeet
  or WhisperX) instead of the crate's own greedy decode, without needing
  to solve the harder whole-sentence-comparison problem at all.
- **Batch-position correlation** — never checked: does mechanism 2
  (hallucination/repetition-collapse) correlate with position within a
  synthesis batch? `vibe_align_reports.jsonl` has `batch_id`/`job_id`
  per segment; would need whatever recorded each batch's segment
  ordering to answer this.
- **How common is mechanism 4** (TTS truncation)? Found once so far; not
  yet checked systematically across other long/citation-heavy sentences.
- **Wiring WhisperX into the actual tooling** — tested via raw CLI calls
  so far, not yet integrated as a Rust subprocess call the way
  `dl transcribe-check` wraps Parakeet.
- Whether to wire free-transcription QA into the live `align_warnings`
  pipeline, and at what score threshold (and using which engine), is
  still an open decision pending more documents' worth of data.
