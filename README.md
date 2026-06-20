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

## GPU acceleration

Build with the devices you want available:

```sh
cargo build --release --features metal          # M1/M2 Mac
cargo build --release --features cuda           # NVIDIA GPU
cargo build --release --features metal,cuda     # both
```

Device is selected at runtime via `FORCED_ALIGNMENT_DEVICE=cpu|metal|cuda`.
If unset, auto-selects the best available: CUDA → Metal → CPU.

```sh
FORCED_ALIGNMENT_DEVICE=cpu ./target/release/forced-alignment ...
```

**On M1 Mac, Metal does not meaningfully speed up inference** —
wav2vec2-base (95M params) is small enough that M1 CPU SIMD is competitive with
Metal kernel dispatch overhead. Benchmarked at ~12s CPU vs ~12s Metal for a
100s audio segment. CUDA on a dedicated GPU (e.g. RunPod) is expected to help
and has not yet been benchmarked.

## Library API

```rust
let samples = forced_alignment::audio::load_audio(&path, forced_alignment::SAMPLE_RATE)?;
let (transcript, report) = forced_alignment::align(&samples, &text)?;
```

See `cargo doc --open` for full API documentation, and [report.md](report.md)
for score interpretation.

Note: Preprocessing is necessary. The aligner scores each word by how well
the acoustic model finds its letters in the audio frames — scores are only
meaningful when the text matches what was actually spoken.

- **Use the text that was sent to the TTS engine**, not the original source.
  If synthesis ran a normalizer ("e.g." → "for example", numerals → words,
  etc.), the audio reflects the normalized form and the aligner must too.
- **Strip speaker directives** — prefixes like `Speaker 1:` are instructions
  to the TTS engine, not spoken words; they will score near zero and create
  spurious suspects.
- **Strip leading/trailing punctuation per token** — the wav2vec2 vocab
  contains only letters and `|`; punctuation attached to a word token
  (e.g. `"Or,"`) deflates its score even when the word itself is present.
