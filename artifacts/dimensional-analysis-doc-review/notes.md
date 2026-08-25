# dimensional-analysis-doc-review

## Reported (2026-08-24)

Doc "1.3. Units and dimensional analysis" — only 2 flags total (already
in `index.md`: `"int"` at 44.83s-44.99s, `"plus"` at 31.57s-31.85s).

- `"int"` flag: reference text is the heading `"1.3.2."`. Audio sounds
  like **"hoip one point three point two"** — i.e. the TTS *did* read
  the numbering aloud as "one point three point two" (a reasonable,
  expected way to speak "1.3.2."), plus an extra garbled `"hoip"`.
- `"plus"` flag: listened — **sounded fine**, nothing actually wrong.

## Notes

Both of these are a *different, simpler* class than the mystery cases
(`wrong-sentence-audio-signal-heading`, `leaked-audio-conven`,
`missing-flag-basic-ideas`) — not the batch-misrouting/wrong-audio
theory from `theory.md`. Both fit squarely within what this feature was
originally built to catch:

- **`"int"`/heading-1.3.2 case**: same shape as `leaked-audio-yu-wa`'s
  "Note" — a short, numbering-only heading gets read aloud (correctly,
  as digits/words) plus a small garbled extra bit. This is "of the class
  we expected with this implementation" — heading/short-text sentences
  producing some kind of extra content is the known, already-understood
  pattern, not a new mystery. (Whether the "hoip" garble itself is
  reference-clip bleed the way "Note" was, or something else, is still
  open — but the *overall shape* — short heading, TTS reads the number
  aloud, plus something extra — isn't surprising anymore.)
- **`"plus"` case**: this may not be a bug at all. If the TTS naturally
  expanded some written form (e.g. a symbol, a compound expression) into
  spoken words that happen to include "plus," that's exactly the
  original motivating use case for insertion detection — the reader/TTS
  legitimately adding a word not in the literal reference text, the
  same way a human reader says "Chapter" before a chapter number. A
  correctly-functioning detection of expected behavior, not a defect.
  Worth double-checking what's actually in the reference text right
  around 31.57s-31.85s to confirm this reading before closing it out.

This doc is a useful contrast case: it shows the detector doing exactly
what it was designed to do, on a document that (unlike the earlier
examples) doesn't seem to be hitting the batch-misrouting/wrong-sentence-
audio problem much, if at all. Good data point for isolating which
documents/voices/conditions actually trigger the mystery failure vs.
which just show ordinary, expected reader-added content.
