# forced-alignment

Word-level forced alignment in Rust: given an audio recording and the known
transcript text, produces start/end timestamps for each word.

Unlike ASR-based timestamping, this doesn't rely on transcription accuracy —
the text is already known (e.g. a human reading or TTS narration of a
document), and the model only needs to find *when* each word was spoken.

For more details, see [overview.md](overview.md) and [report.md](report.md).

## How it works

1. Decode audio to 16kHz mono (`symphonia` + `rubato`).
2. Run a `wav2vec2-base-960h` CTC model via `candle` to get per-frame letter
   probabilities. Long audio is processed in overlapping 20s chunks (wav2vec2
   attention is O(T²)).
3. Run a Viterbi forced-alignment DP between the CTC output and the known
   text to find each word's frame span.
4. Return a `Transcript` with per-word `start`/`end`/`score` (seconds, 0.0–1.0)
   and an `AlignReport` describing filtered and suspect words.

Currently English only.

## Build

```sh
cargo build --release
```

The wav2vec2 model weights (~360MB) are downloaded from Hugging Face Hub on
first run and cached locally.

## CLI

```sh
./target/release/forced-alignment <audio> <text> -o <output.json>
```

Suspect words and filtered word counts are printed to stderr. Example:

```sh
./target/release/forced-alignment validation-samples/short-sentence.mp3 \
  validation-samples/short-sentence.md -o out.json
```

## Library API

```rust
let samples = forced_alignment::audio::load_audio(&path, forced_alignment::SAMPLE_RATE)?;
let (transcript, report) = forced_alignment::align(&samples, &text)?;
```

See `cargo doc --open` for full API documentation, and [report.md](report.md)
for preprocessing requirements and score interpretation.
