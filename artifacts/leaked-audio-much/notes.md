# leaked-audio-much

## Reported (2026-08-24)

- Insertion warning: leaked audio `"much"`.
- Repro: `cargo run -- cache-path 0d594437-9810-45af-aed9-c73cf964d84a 15 --voice "vibe:Jack"`
- text: `"This is not the same thing as being easy."`
- cache key: `b9d3f2d552dfd51bd15ce50e20c72b686a68ce24773d6180ec981cabc907cd2f`
- mp3: `/Users/sallen/.odoru/audio/b9d3f2d552dfd51bd15ce50e20c72b686a68ce24773d6180ec981cabc907cd2f.mp3`
- meta: `/Users/sallen/.odoru/audio/b9d3f2d552dfd51bd15ce50e20c72b686a68ce24773d6180ec981cabc907cd2f.json`
- exists: yes (invalid=false, duration=6.66s)

## Notes

- Different signature from the other three so far — this one is
  *correctly* flagged, and the per-word scores are mostly good
  (0.75-0.9999), only "easy." is low (0.25). Not the "pervasively
  low-scoring sentence" pattern seen in `leaked-audio-conven` and
  `missing-flag-basic-ideas`.
- Instead, this sentence has several unusually large gaps between
  consecutive aligned words, for only 9 words across 6.66s total:
  - "This" (ends 0.44) → "is" (starts 1.58): **1.14s gap**
  - "not" (ends 2.08) → "the" (starts 2.64): 0.56s gap
  - "the" (ends 2.74) → "same" (starts 3.18): 0.44s gap
  - "same" (ends 3.54) → "thing" (starts 4.47): **0.93s gap**
  - "thing" (ends 5.09) → "as" (starts 5.29): 0.20s gap
  One of the two largest gaps (1.14s after "This", or 0.93s between
  "same" and "thing") is the most likely home for the leaked "much" —
  haven't listened yet to confirm which.
- This looks like the "extra audio in a real gap" case the insertion
  detector was actually designed for, unlike the previous three reports.
  Good confirming data point that the mechanism works when there's a
  genuine anomalous gap, as opposed to the pervasive-low-score pattern
  the detector doesn't currently address at all.
- Next step: listen to the mp3, focusing on the two largest gaps, to
  confirm exactly where "much" sits and what it sounds like.
