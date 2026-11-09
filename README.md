# forced-alignment

Word-level forced alignment in Rust: given an audio file and the known
transcript text, produces start/end timestamps for each word.

Unlike ASR-based timestamping, this doesn't rely on transcription accuracy —
the text is already known (e.g. a human reading or TTS narration of a
document), and the model only needs to find *when* each word was spoken.

For more details, see [overview.md](overview.md) and [report.md](report.md).

Note: This repo is a library only. 

For a command line that times each
sentence (and optionally each word) of a markdown or plain-text document,
see `speakable align` and `speakable demo --audio` in
[speakable](https://github.com/ultrasaurus/speakable), whose
`speakable-align` crate normalizes the text, calls this library and maps
the times back to the document's words.

## How it works

1. Decode audio to 16kHz mono (`symphonia` + `rubato`).
2. Run a `wav2vec2-base-960h` CTC model via `candle` to get per-frame letter
   probabilities. Long audio is processed in overlapping 20s chunks (wav2vec2
   attention is O(T²)).
3. Run a Viterbi forced-alignment DP between the CTC output and the known
   text to find each word's frame span.
4. Return a `Transcript` with per-word `start`/`end`/`score` (seconds, 0.0–1.0)
   and an `AlignReport` describing filtered words, suspect words, and
   insertions (audio present that isn't in the reference text at all).

Currently English only.

## Build

```sh
cargo build --release
```

The wav2vec2 model weights (~360MB) are downloaded from Hugging Face Hub on
first run and cached locally.

The library aligns a whole recording in one pass, so see
[overview.md](overview.md) on memory for recordings longer than about
35 minutes.

## GPU acceleration

Build with the devices you want available:

```sh
cargo build --release --features metal          # M1/M2 Mac
cargo build --release --features cuda           # NVIDIA GPU
cargo build --release --features metal,cuda     # both
```

Device is selected at runtime via `FORCED_ALIGNMENT_DEVICE=cpu|metal|cuda`.
If unset, auto-selects the best available: CUDA → Metal → CPU.

**On M1 Mac, Metal does not meaningfully speed up inference** —
wav2vec2-base (95M params) is small enough that M1 CPU SIMD is competitive with
Metal kernel dispatch overhead. Benchmarked at ~12s CPU vs ~12s Metal for a
100s audio segment. CUDA on a dedicated GPU (e.g. RunPod) is expected to help
and has not yet been benchmarked.

## Library API

```rust,ignore
let samples = forced_alignment::audio::load_audio(&path, forced_alignment::SAMPLE_RATE)?;
let (transcript, report) = forced_alignment::align(&samples, &text)?;
```

The model loads on the first `align` call. To load it up front instead (e.g.
at service startup, before other models fill a shared GPU), call
`forced_alignment::preload()?`.

See `cargo doc --open` for full API documentation, and [report.md](report.md)
for score interpretation.

To see the raw result on a file, run the `align` example. It passes the
text to `align` as it is and writes `{"transcript": ..., "report": ...}`
as JSON (to stdout without `-o`):

```sh
cargo run --release --example align -- validation-samples/short-sentence.mp3 \
  validation-samples/short-sentence.md -o out.json
```

`align_with` takes `AlignOptions`. With `sentence_starts` (the index of
each sentence's first word), speech that isn't in the text is placed
between sentences in preference to inside one, and a whole sentence the
audio doesn't contain is skipped (left untimed, and listed in
`AlignReport::skipped`) rather than crushed into the speech around it;
`speakable-align` uses it. See [overview.md](overview.md).

Note: Preprocessing is necessary (`speakable-align` does it for you). The
aligner scores each word by how well
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

## Development

A pre-commit hook checks `cargo fmt` and `cargo clippy` (warnings fail it).
Turn it on once per clone:

```sh
git config core.hooksPath .githooks
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

The quoted texts in [validation-samples/](validation-samples/) keep their
own terms; see its [README](validation-samples/README.md).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
