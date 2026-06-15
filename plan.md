# Forced Alignment Plan (Rust)

## Goal
Get sentence/word-level timestamps for audio where we already have the
ground-truth text (human-read or TTS-generated), without relying on ASR
transcription accuracy.

## Why not ASR
- Whisper transcription errors make forced alignment against known text
  unreliable.
- VibeVoice-ASR (9B params) is too heavy for M1, and only gives
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

## Forced Alignment Plan
No drop-in Rust crate exists. Combine:
1. `candle` (HF's Rust ML framework) has wav2vec2 model
     implementations and can run CTC inference directly, no ONNX export
     needed.
2. **Hand-written trellis/Viterbi alignment routine** (~100 lines) following
   the CTC-segmentation algorithm from the torchaudio CTC forced alignment
   tutorial.
   - https://docs.pytorch.org/audio/main/tutorials/ctc_forced_alignment_api_tutorial.html

## Additional concerns
- Sometimes readers add words.  Like for "As We May Think", the reader said "Chapter 6" but the original text just reads "6" -- happens with lists, footnotes, etc.  To handle human readers where the added words often make the text easier to listen to, there will have to be a way to flag those as not in the text and still do the forced alignment.
- normalization/expansion pass before alignment (e.g. → "for example", numerals → words or vice versa, footnote markers → skippable?)

## Alternatives considered

Step 1. 

**ONNX-exported wav2vec2/MMS CTC model** — one-time export from Python,
   then run inference in Rust via `ort` (ONNX Runtime bindings). Avoids
   Python in the runtime pipeline. -- chose all-rust approach to start

**MMS** (chose wav2vec to start, English only)
   * The only MMS checkpoints with a CTC head (i.e., ready for forced alignment) are facebook/mms-1b-all and similar — 1B params, same weight class as the VibeVoice model your plan already ruled out as too heavy for M1.
   * facebook/mms-300m is the pretrained backbone only — no vocab/CTC head, needs fine-tuning to be usable.
   * By contrast, facebook/wav2vec2-base-960h (95M params, English CTC) uses a simpler architecture (group-norm feature extractor + post-norm encoder) — much lighter to implement and run.