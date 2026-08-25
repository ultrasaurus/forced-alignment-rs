# leaked-audio-freight

## Reported (2026-08-24)

- Doc "1.4. Audio and signals", leaked audio: `"freight"` (12.83s-13.05s).
- **Confirmed: a real TTS artifact, caught exactly as intended.**
- Action item: double-check the reported timestamp (12.83s-13.05s)
  actually lines up with where "freight" is audible.

## Notes

A clean success case — no mystery, no batch-misrouting pattern, just
the detector doing its job. Earlier theory (`theory.md`) speculated
`"freight"` might be the same underlying leak as `"frequenc"` (both
share the "FRE-" prefix) — worth re-checking once the timestamp is
verified, since if both really do trace back to "frequency" bleeding in
at different points, that's a useful confirming detail for whichever
root-cause theory ends up correct.

## Sentence pulled (2026-08-24)

Doc `cb32fa2a-34ef-4384-b6e4-cf4a6de56ad5`, sentence 113, voice
`vibe:Jack`. Text: `"This motivates the use of cents (denoted ¢), which
divide each semitone range evenly into 100 pieces; or, equivalently,
each octave into 1200 pieces."` — more numeric/notation-heavy content
(¢ symbol, "100", "1200"), consistent with the math-vocabulary
correlation theory. Archived to `audio/sentence-113.{mp3,json}` before
the re-synth overwrites the cache.

Word scores are almost all high (0.94-0.9999) — clean, well-aligned
sentence, Class 1 (isolated artifact in otherwise-good audio), except
`"cents"` (0.60) and `"octave"` (0.83), still not low enough to be
flagged on their own.

**Note on the reported timestamp (12.83s-13.05s)**: this sentence's own
clip is only 10.93s long, so 12.83s can't be a position *within this
clip* — confirms (same as sentence 42 earlier) that `index.md`'s
timestamps are relative to the multi-sentence **segment**'s own audio
timeline (the TTS call spans several sentences at once), not each
sentence's individually-sliced clip. Can't directly verify the exact
audible spot from this file alone without the segment-level audio/
timing. One candidate gap worth a listen regardless: `"denoted"` ends
at 2.30s, `"which"` starts at 3.16s — an 0.86s gap, unusually large
among otherwise tight word-to-word spacing in this sentence.
