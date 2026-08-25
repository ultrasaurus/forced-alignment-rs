# wrong-sentence-audio-signal-heading

## Reported (2026-08-24)

- **Correction (2026-08-25): this is sentence 21, not 20.** Doc
  `0d594437-9810-45af-aed9-c73cf964d84a`, sentence 21, text: `"What is a
  signal?"` (the earlier report's "1.1.1." prefix is a separate heading
  sentence, not part of 21's own text). Archived to
  `audio/sentence-21.{mp3,json}`.
- Audio actually spoken: **"This is not the same thing as being easy."**
  — this is not garbled or partial, it's a complete, correct sentence...
  just the wrong one. It's the exact text of `leaked-audio-much`'s
  sentence (cache key `b9d3f2d552dfd51bd15ce50e20c72b686a68ce24773d6180ec981cabc907cd2f`).

## Correction (2026-08-25) — durations don't match; not a literal file swap

Sentence 21's own clip is only **2.31s**, with quite low scores
throughout (`What`=0.25, `is`=0.47, `a`=0.00002, `signal?`=0.33) and
~0.83s of trailing dead time after "signal?" ends at 1.48s. `leaked-
audio-much`'s sentence 15 clip is **6.66s**. These are not the same
audio file — so "the complete, correct audio for sentence 15 got filed
under sentence 21's cache entry" (the framing below) is **not what
happened**; the durations rule out a literal duplicate/misrouted file.

What's more consistent with this new data: sentence 21's own ~2.31s
generation produced content that, when listened to, evokes/echoes
sentence 15's sentence — but re-rendered at a different length, not
byte-identical reused audio. That fits the "repetition-collapse
hallucination" reframing in `theory.md` much better than a literal
pipeline routing/indexing bug: the model *echoing* recently-generated
content into a new generation call, not a cache/indexing system
literally reusing the wrong audio file. The "routing bug" framing below
is now the less likely explanation — kept for the record, but see the
correction.

## Notes

This is a different, more direct kind of failure than anything logged so
far: not a brief leak, not reference-clip bleed, not "mostly wrong audio
with no clean match" — **the complete, correct audio for a different
sentence got filed under this sentence's slot.** `leaked-audio-much`'s
audio is a real, normally-synthesized sentence (per its own notes: good
scores throughout, 0.75-0.9999, aside from the "much" insertion) — it's
not broken audio, it's just attached to the wrong text.

**This significantly shifts the root-cause theory** (see `theory.md`):
away from "TTS model attention/conditioning leak" and toward **a
sentence-audio routing/indexing bug somewhere in the batch pipeline** —
audio generated for one sentence ending up stored/assigned to a
different sentence's cache entry. That's a much more mundane, and much
more directly fixable, class of bug than a model-level hallucination —
an off-by-N ordering issue, a race in async batch fetch/store, or a
sentence_id/segment-position mismatch somewhere between vibe-service's
batch response and odoru's per-sentence audio slicing/caching
(`ingest::audio::slice_segment_audio`, `audio_store::audio_cache`).

**Confirmed: sentence 20 was NOT flagged at all.** Another silent miss —
same shape as `missing-flag-basic-ideas`. This is now the second
confirmed case of a genuinely bad sentence producing zero warning,
strongly reinforcing the "needs a third, coarser signal" open question
in `theory.md`: a full wrong-sentence audio swap produces uniformly bad
scores against the wrong reference text (no isolated anomaly for
`Truncated`/`Insertion` to key on), so it's currently invisible by
design, not by bug — the detector was never built to catch "this is
someone else's audio entirely," only "an otherwise-correct sentence has
one bad patch."

**Next step**: check whether other "leaked audio" / low-score reports so
far are actually *this same failure* — a full wrong-sentence swap, not a
partial leak — by checking whether their audio, if listened to in full,
matches some *other* sentence's text elsewhere in the document (the way
this one does), rather than assuming each is its own isolated glitch.
