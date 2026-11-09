# Forced Alignment in Rust

## Goal
Get sentence/word-level timestamps for audio where we already have the
ground-truth text (human-read or TTS-generated), without relying on ASR
transcription accuracy.

Current implementation English-only, low footprint.

Future: multilingual

## Additional concerns
- Sometimes readers add words. For "As We May Think", the reader said "Chapter 6" but the original text just reads "6" -- this happens with lists, footnotes, etc.  To support human listeners where the added words often make the text easier to listen to, there will have to be a way to flag those as not in the text and still do the forced alignment.
- Consider normalization/expansion pass before alignment (e.g. → "for example", numerals → words or vice versa, footnote markers → skippable?)


## Why not ASR
- Whisper transcription errors make forced alignment against known text
  unreliable.
- VibeVoice-ASR (9B params) is too heavy for target dev env (M1 Mac), and only gives
  speaker/segment-level timestamps anyway, not word-level, and doesn't do
  forced alignment against known text.

## Options considered

### ctc-forced-aligner (Python)
- https://github.com/MahmoudAshraf97/ctc-forced-aligner
- Takes known text + audio directly, aligns via CTC segmentation using
  wav2vec2/MMS models through torchaudio.
- Well-tested, handles long audio, multilingual via MMS.
- No Rust port exists.

### wav2vec2-rs
- No maintained published crate with alignment support found.

# Forced Alignment Approach
No drop-in well-maintained, published Rust crate exists. Combine:
1. `candle` (HF's Rust ML framework) has wav2vec2 model
     implementations and can run CTC inference directly, no ONNX export
     needed.
2. **Hand-written trellis/Viterbi alignment routine** (~100 lines) following
   the CTC-segmentation algorithm from the torchaudio CTC forced alignment
   tutorial.
   - https://docs.pytorch.org/audio/main/tutorials/ctc_forced_alignment_api_tutorial.html

## Speech that isn't in the text

A reader adds words, a recording has an intro or outro, and a TTS clip can
leak audio from its reference voice. The DP has a filler state for such
speech: frames in it match no text, are reported as an insertion, and
don't stretch the neighboring words.

`align` treats every point between words alike. `align_with` takes
`AlignOptions::sentence_starts` (each sentence's first word index), and
then prefers to place extra speech between sentences:

- Entering filler anywhere but between sentences costs
  `MID_SENTENCE_FILLER_ENTRY_PENALTY` once per entry. Without it, a short
  word in a preamble ("the") matched a text word and put the rest of the
  preamble inside the first sentence.
- Filler before the first word costs `LEAD_IN_FILLER_DISCOUNT` less per
  frame. A lead-in often says the opening words too (LibriVox reads the
  title in its preamble), and either match costs about the same; the
  discount makes the opening sentence take the last match.

Without sentence starts, entering filler costs the same everywhere, which
suits a TTS clip checked for leaked audio inside a sentence.

## Text that isn't in the audio

The DP must give every letter at least one frame, so text the audio doesn't
contain still gets times: a reader who skips a passage, or a recording that
stops before the text does. The unsaid letters are crushed into the speech
around them, at 350–500 words a minute, and the sentences around the gap
are pulled toward it too. On one reading that left out nine sentences, the
four sentences before the gap were timed up to 15 seconds early.

With `sentence_starts`, the DP may instead skip a whole sentence: a
transition from one sentence's first letter to the next one's, within the
same frame, costing `SKIP_PENALTY_PER_LETTER` per letter skipped. Skips
chain, so a run of sentences is skipped together, and they're allowed at
the start and the end.

- The cost sits between the two alternatives. An unsaid letter crushed onto
  a frame of other speech scores about 0.01 (log ≈ −4.6). A said letter
  pushed into filler costs `FILLER_PENALTY` (1.5). At 2.0 per letter,
  skipping beats crushing, and aligning said text beats skipping it.
- Skipped words stay in the transcript in place, with no `start`, `end` or
  `score`, and `AlignReport::skipped` lists each run with where in the
  audio it happens.
- Only whole sentences are skipped. A few skipped words inside a sentence
  are still crushed, and show as low-score suspects.

Without sentence starts nothing is skipped, and audio that stops early
shows as `Truncated` suspects instead.

## Memory and long audio

Two stages use memory differently:

- **Model inference** runs in 20 s chunks (`CHUNK_SECONDS` in `model.rs`),
  since self-attention is quadratic in length. Its memory doesn't grow with
  the audio. It sets the peak on every audio recording we've run: 1.6–1.9 GB on
  an M1, measured at 1.5, 3, 5 and 17 minutes.
- **The Viterbi DP** (`ctc_forced_align` in `ctc.rs`) runs on the whole
  recording at once, over a grid of frames × tokens.
  - Frames: 50 per second (320 samples per frame at 16 kHz).
  - Tokens: the text's letters plus one separator per word. Read English
    runs at about 16 per second of audio.
  - Scores are kept for two frames only, since each frame's scores depend
    only on the frame before. The back-pointers the backtrace walks are kept
    for every cell, packed two cells per byte (`BackPointers`).
  - So memory grows with the square of the length: about 400 bytes per
    second², or roughly 40 MB at room-one's 5 minutes (15,300 frames ×
    4,983 tokens), 0.4 GB at 17 minutes, 5 GB at 60 and 21 GB at two hours.
  - It passes inference's peak at around 35 minutes. On a 16 GB machine,
    a recording of up to about 80 minutes aligns whole.
  - DP time grows with the square of the length too: the whole alignment
    takes 67 s for room-one and 224 s for a 17-minute recording.

Past that, or to save time, align long audio in pieces: call `align` on
each piece with its own text, and add the piece's start time to its word
times. Cut where you know which text goes with each piece, and at a pause
(sentence or paragraph ends are the usual places). A word whose audio
crosses the cut, or whose text is on the wrong side of it, is crushed into
too few frames or reported as an insertion. Memory and time then stay at the
piece's size. When the audio is made in pieces (TTS per segment), align
each one as it is.

## Alternatives considered

**ONNX-exported wav2vec2/MMS CTC model** — one-time export from Python,
   then run inference in Rust via `ort` (ONNX Runtime bindings). Avoids
   Python in the runtime pipeline. -- chose all-rust approach to start

**MMS** (chose wav2vec to start, English only)
   * The only MMS checkpoints with a CTC head (i.e., ready for forced alignment) are facebook/mms-1b-all and similar — 1B params, same weight class as the VibeVoice model your plan already ruled out as too heavy for M1.
   * facebook/mms-300m is the pretrained backbone only — no vocab/CTC head, needs fine-tuning to be usable.
   * By contrast, facebook/wav2vec2-base-960h (95M params, English CTC) uses a simpler architecture (group-norm feature extractor + post-norm encoder) — much lighter to implement and run.