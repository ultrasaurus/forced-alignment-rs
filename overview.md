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

## Memory and long audio

Two stages use memory differently:

- **Model inference** runs in 20 s chunks (`CHUNK_SECONDS` in `model.rs`),
  since self-attention is quadratic in length. Its memory doesn't grow with
  the audio. It sets the peak on every audio recording we've run: 1.6–1.9 GB on
  an M1, measured at 1.5, 3 and 5 minutes.
- **The Viterbi DP** (`ctc_forced_align` in `ctc.rs`) runs on the whole
  recording at once. It keeps four full tables of frames × tokens: two of
  scores (`f32`) and two of back-pointers (1 byte each), about 10 bytes per
  cell.
  - Frames: 50 per second (320 samples per frame at 16 kHz).
  - Tokens: the text's letters plus one separator per word. Read English
    runs at about 16 per second of audio.
  - So the tables grow with the square of the length: about 8,000 bytes per
    second², or roughly 30 MB at one minute and 0.75 GB at five (room-one:
    15,300 frames × 4,983 tokens).
  - They pass inference's peak at around 7–8 minutes. A 25-minute audio 
    recording would need about 18 GB.
  - DP time grows the same way: room-one (5 min) takes 56 s, against 29 s
    for a 3-minute audio recording.

So align long audio in pieces of a few minutes or less: call `align` on
each piece with its own text, and add the piece's start time to its word
times. Cut where you know which text goes with each piece, and at a pause
(sentence or paragraph ends are the usual places). A word whose audio
crosses the cut, or whose text is on the wrong side of it, is crushed into
too few frames or reported as an insertion. Memory and time then stay at the
piece's size. When the audio is made in pieces (TTS per segment), align
each one as it is. One long recording with no known boundaries can be
aligned whole up to about 7–8 minutes before the DP sets the peak.

## Alternatives considered

**ONNX-exported wav2vec2/MMS CTC model** — one-time export from Python,
   then run inference in Rust via `ort` (ONNX Runtime bindings). Avoids
   Python in the runtime pipeline. -- chose all-rust approach to start

**MMS** (chose wav2vec to start, English only)
   * The only MMS checkpoints with a CTC head (i.e., ready for forced alignment) are facebook/mms-1b-all and similar — 1B params, same weight class as the VibeVoice model your plan already ruled out as too heavy for M1.
   * facebook/mms-300m is the pretrained backbone only — no vocab/CTC head, needs fine-tuning to be usable.
   * By contrast, facebook/wav2vec2-base-960h (95M params, English CTC) uses a simpler architecture (group-norm feature extractor + post-norm encoder) — much lighter to implement and run.