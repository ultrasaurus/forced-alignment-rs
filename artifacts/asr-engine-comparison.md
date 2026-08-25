# ASR engine comparison for free-transcription QA

Reference for choosing a free-transcription engine (independent ASR pass
compared against reference text — see `theory.md`'s "Free-transcription
QA" section for why this is worth having at all). Parakeet was tried
first, tested thoroughly, and retired in favor of WhisperX after real,
confirmed limitations — recorded here so the next attempt (whether at
WhisperX or a different Parakeet model/version) starts from what's
already known rather than re-discovering it.

## Parakeet — tried and retired

**What was tried**: `altunenes/parakeet-rs` v0.3.7 (crates.io, MIT/Apache-2.0,
385 stars, 348 commits — the most mature of the Rust-native Parakeet
options evaluated; a Candle-based alternative, `gpu-cli/parakeet-rs`, was
also considered and rejected first — 1 star, 9 commits, Metal/Apple-Silicon-only,
no clear CPU support, CLI-only with no real library API). Model:
`onnx-community/parakeet-ctc-0.6b-ONNX` (the CTC variant — the crate
also bundles a separate Nemotron/TDT model for other use cases, not
evaluated here), ONNX Runtime, CPU only.

Integrated into `forced-alignment` as `transcribe()`/
`transcribe_with_timestamps()` behind an opt-in `transcribe` Cargo
feature; since retired (removed from `src/`, `Cargo.toml`, `README.md`,
`report.md`) — this doc is what's left of that work.

**Real limitations found, all confirmed on production audio, not
hypothetical:**

1. **Word timestamps are broken.** Confirmed crate bug in
   `decoder.rs::decode_with_timestamps`: converts the encoder's own
   output frame index to seconds using only the mel-spectrogram hop
   length (`160` samples @ 16kHz), without multiplying by FastConformer's
   8x conv-subsampling factor first. Every timestamp comes out ~8x too
   small — measured directly: a 14.73s clip's last word reported ending
   at 1.85s (14.73 / 1.85 ≈ 7.96). Not a caller-side units mistake; the
   crate's own source does the conversion internally and gets it wrong.
2. **No per-word confidence exposed on the CTC model.** `TimedToken` only
   carries `text`/`start`/`end`. Per-token `logprob` exists in the crate
   (`TokenInfo` in `nemotron.rs`) but only on the separate Nemotron/TDT
   model — a different model entirely, not a flag on the CTC path used
   here.
3. **Mishandles acronyms and unusual capitalized tokens.** Confirmed on
   real audio, both cases verified by ear against the actual TTS output:
   - `"S I D"` (spelled-out letters, normalizer's expansion of "SID")
     transcribed as the single word `"sid"`, collapsing the letters.
   - `"AFIPS"` transcribed as `"a phipps"`.
4. **Silently drops the tail of long/complex sentences in some cases.**
   Originally logged as a Parakeet-specific limitation — **turned out not
   to be one**. WhisperX did the same thing on the identical sentence;
   both engines were correctly reporting that the TTS audio itself
   stopped early (a real synthesis bug, not an ASR truncation issue —
   see `theory.md`'s "Mechanism 4"). Recorded here only so this isn't
   mistakenly re-flagged as a Parakeet weakness on a future attempt.

**Not a limitation, worth noting anyway**: transcription speed was good
— roughly 7x real-time on CPU (M1 Mac) in informal testing across 16
sentences, and a full 335-sentence, ~53-minute document completed in a
few minutes. If a future model/version fixes the above, performance
isn't a concern.

## WhisperX — current direction

**What was tried**: `whisperx` (Python, `pip install
git+https://github.com/m-bain/whisperx.git`), invoked as a subprocess
(same pattern as `/Users/sallen/src/discr/altwebgen/src/web/audio/whisperx.rs`
from an earlier project — `Command::new("whisperx")`, JSON output,
parsed on the Rust side). Existing local conda env (`whisperx`,
`/opt/homebrew/Caskroom/miniconda/base/envs/whisperx`) from that project
reused directly — already working, no setup needed this session.

**Confirmed benefits, on the exact same test cases Parakeet failed:**

1. **Correct word timestamps**, real seconds, no scale bug — verified
   against actual clip duration.
2. **Per-word confidence exposed by default** — every word in the JSON
   output carries a `score`. On the physics-doc insertion case (a
   leaked `"*"` → `"IRN"`), the confidence dips exactly where the real
   insertion is (`"IRN"` 0.682, the following `"and"` 0.329 — the lowest
   score in the whole transcript), directly localizing the problem
   without needing any reference-text comparison at all.
3. **Correct acronym handling** on both confirmed Parakeet failures:
   `"S-I-D."` (spelled out correctly) and `"AFIPS"` (kept intact).
   `set_initial_prompt()` (vocabulary/context biasing) is also available
   if needed, though not yet used in testing — Parakeet has no
   equivalent.
4. **Same truncation behavior as Parakeet on the one case tested** — not
   a WhisperX advantage over Parakeet specifically, but confirms mechanism
   4 (TTS truncation) is a real synthesis-side bug both engines correctly
   surface.

**Known downside**: Python/conda, not a native Rust crate — but the
actual integration is a subprocess call + JSON parse, not a deep
embedding (no PyO3, no Python runtime linked into the Rust binary), so
the "day-to-day Python hassle" concern from early in this investigation
applies much less here than it would to, say, embedding via PyO3.

**Known maintenance-health caveat** (about the underlying whisper.cpp
ecosystem more broadly, relevant if a native-Rust path is ever
reconsidered): `whisper-rs` (Rust bindings to whisper.cpp) was also
researched as a possible native-Rust alternative to a Python subprocess —
~1M crates.io downloads, far more than either Parakeet crate, but a
year-old open performance-regression issue and several stalled PRs (last
merge 5+ months old at research time). "Most downloaded/mature of the
Rust options" and "actively maintained" are separate claims; not
pursued further once the existing, already-working WhisperX setup made a
Python subprocess the lower-risk near-term choice.

**Not yet done**: wiring WhisperX into actual Rust tooling (tested via
raw `whisperx` CLI calls so far, not yet a Rust subprocess wrapper);
building the confidence-based and truncation-detection signals described
above into real code; deciding whether any of this becomes a production
`align_warnings` signal.
