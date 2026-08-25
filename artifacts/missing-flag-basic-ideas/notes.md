# missing-flag-basic-ideas

## Reported (2026-08-24)

- No flag was raised, but should have been.
- Repro: `cargo run -- cache-path 0d594437-9810-45af-aed9-c73cf964d84a 13 --voice "vibe:Jack"`
- text: `"I sometimes use the term basic when describing certain ideas."`
- cache key: `1dc7302149afe0b7aa8f587f3b674d8cfe36b73f822e6de1ea375c428583cba7`
- mp3: `/Users/sallen/.odoru/audio/1dc7302149afe0b7aa8f587f3b674d8cfe36b73f822e6de1ea375c428583cba7.mp3`
- meta: `/Users/sallen/.odoru/audio/1dc7302149afe0b7aa8f587f3b674d8cfe36b73f822e6de1ea375c428583cba7.json`
- exists: yes (invalid=false, duration=2.16s)

## Notes

- `meta.json` per-word scores are almost all below `SUSPECT_THRESHOLD`
  (0.3): I=0.00005, use=0.00007, the=0.24, term=0.02, basic=0.04,
  when=0.11, certain=0.27, ideas.=0.002 — only "sometimes" (0.22, still
  below threshold) and "describing" (0.33, just above) are anywhere
  close to clean-speech range (0.8+ per `report.md`). No large timing
  gaps between words, so this doesn't look like the "Note"-style
  padding case.
- **Likely explanation for the missing flag**: odoru's `align_warnings`
  (vibe_sync.json) currently only records `SuspectReason::Truncated` and
  `Insertion` — plain `LowScore` suspects are deliberately excluded (see
  `ingest/src/audio.rs`'s `align_warnings_for_sentences`), because we
  scoped the odoru-side feature to "Truncated + insertions only" to
  avoid noise (LowScore fires very often on normal text, per the
  As-We-May-Think validation run). This sentence is a run of ~8
  low-scoring words with **no trailing pace-collapse** (so it doesn't
  qualify as `Truncated`) and **no detected filler span** (so it's not
  an `Insertion` either) — it falls into the gap between the two
  categories we actually surface.
- Same "near-universal low scores, no single flaggable reason" pattern
  as `leaked-audio-conven`'s sentence. Between that one and this one,
  there may be a real, recurring class of audio problem (rushed/garbled
  TTS pacing?) that our current Truncated/Insertion-only scoping is
  blind to by design — worth deciding whether `align_warnings` needs a
  third category (e.g. "many low-score words, no single anomaly") once
  we have enough samples to see the shape of it.
- Next step: listen to the mp3 — does it actually sound wrong throughout,
  or wrong in one specific spot? That'll tell us whether this is a
  genuinely bad *whole clip* (matching the low scores everywhere) or a
  narrower problem the current per-word scoring is just bad at
  localizing.
