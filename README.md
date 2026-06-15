# forced-alignment

Word-level forced alignment in Rust: given an audio recording and the known
transcript text, produces start/end timestamps for each word.

Unlike ASR-based timestamping, this doesn't rely on transcription accuracy —
the text is already known (e.g. a human reading or TTS narration of a
document), and the model only needs to find *when* each word was spoken.

## How it works

1. Decode audio to 16kHz mono (`symphonia` + `rubato`).
2. Run a `wav2vec2-base-960h` CTC model via `candle` to get per-frame letter
   probabilities. Long audio is processed in overlapping 20s chunks (wav2vec2
   attention is O(T²)).
3. Run a Viterbi forced-alignment DP between the CTC output and the known
   text to find each word's frame span.
4. Write a WhisperX-style JSON file with per-word `start`/`end`/`score`.

Currently English only.

## Build

```sh
cargo build --release
```

The wav2vec2 model weights (~360MB) are downloaded from Hugging Face Hub on
first run and cached locally.

## Run

```sh
./target/release/forced-alignment <audio> <text> -o <output.json>
```

Example, using the included sample:

```sh
./target/release/forced-alignment samples/short-sentence.mp3 samples/short-sentence.md -o out.json
```

`out.json`:

```json
{
  "segments": [
    {
      "start": 0.100273654,
      "end": 3.4895232,
      "text": "it may contain annotations, additions and footnotes",
      "words": [
        { "word": "it", "start": 0.100273654, "end": 0.14038312, "score": 0.9979633 },
        { "word": "may", "start": 0.22060205, "end": 0.3208757, "score": 0.99977773 },
        { "word": "contain", "start": 0.40109462, "end": 0.7019156, "score": 0.99519664 },
        ...
      ]
    }
  ]
}
```
