# leaked-audio-yu-wa

## Reported (2026-08-24)

- Insertion warning: leaked audio `"yu wa"`.
- Repro: `cargo run -- cache-path 0d594437-9810-45af-aed9-c73cf964d84a 12 --voice "vibe:Jack"`
- text: `"Note"`
- cache key: `53a068141d08465c21d347cd6fed31987212672222226b5868558afb1cc6252c`
- mp3: `/Users/sallen/.odoru/audio/53a068141d08465c21d347cd6fed31987212672222226b5868558afb1cc6252c.mp3`
- meta: `/Users/sallen/.odoru/audio/53a068141d08465c21d347cd6fed31987212672222226b5868558afb1cc6252c.json`
- exists: yes (invalid=false, duration=1.71s)

## Notes

- `meta.json`: sentence text is just `"Note"` (a single word, likely a
  heading/label). The aligned word "Note" spans only 0.68s-0.90s of a
  1.71s clip — over a second of audio (roughly 0.68s before + 0.81s
  after) isn't accounted for by the one reference word at all.
- Score for "Note" itself is 0.74 — decent, not flagged as suspect on
  its own.
- Likely pattern: very short single-word/short-label sentences are a
  known TTS hallucination trigger — the model has very little real
  content to anchor on and can pad the clip with extra
  (garbled/nonsense) speech. `"yu wa"` reads like exactly that kind of
  hallucinated filler, not a real leaked word from elsewhere in the
  document.
- Compare with `leaked-audio-conven`: that one was a full, normal-length
  sentence, not a short label — so if this "short sentence → hallucinated
  padding" theory holds, it wouldn't explain that case, meaning there may
  be more than one distinct root cause behind these leaked-audio
  insertions. Keep both open until we have more samples to correlate.
- Next step: listen to the actual mp3 to see what's really in the padding
  before/after "Note", and check whether other very-short sentences in
  this document show the same pattern.

## Update (2026-08-24)

`index.md` (a *different* document, `1.1. Preliminaries`) logs a
near-identically-decoded leak: `"you wa"` at 41.45s-41.73s, confirmed by
ear to be **Jack's reference clip** bleeding through ("Do you want me
reading..." → "you wa[nt]"). See `theory.md`.

Given how close `"yu wa"` and `"you wa"` are as CTC greedy-decode
strings, this is very likely the *same underlying leak* (reference clip
saying "you want"), not a coincidence — just with a much bigger acoustic
footprint here (~1s vs. ~0.3s) because there's almost no real target
text ("Note" is one word) to compete with/drown out the reference
audio's influence. Reframes the original "short-text hallucination"
theory in this file: it's not that short text causes *generic*
hallucinated padding, it's that short text lets the *same* reference-clip
leak mechanism dominate a much larger fraction of the clip. See
`theory.md`'s "confirmed by ear" section for the fuller picture (this
plus document-content bleed, not just one source).

## Update (2026-08-24) — confirmed by ear: the aligned "Note" word itself is wrong audio

User, listening directly: the audio the aligner matched to the
reference word **"Note"** (0.68s-0.90s, scored 0.74) actually sounds
like **"want me"** — soft and clipped at the end, unmistakably from
Jack's reference clip, not a synthesis of the word "Note" at all.

This is stronger than "a fragment leaked in near the real word" — it
means essentially the **entire clip is reference-clip audio**, with no
real TTS output of "Note" anywhere in it. The `Word` span for "Note"
only exists because the Viterbi DP is never allowed to leave a
reference word unassigned (see `theory.md`'s "this isn't a brief blip"
correction) — it found the frames in "want me" that best matched the
letters N-O-T-E acoustically, however weak that match really was, and
called that "Note."

**New, important calibration point**: the 0.74 score here is *not* low
— it would not have tripped `LowScore`, let alone `Truncated`. A
reference word getting force-matched onto the wrong audio can still
score reasonably well if the wrong audio happens to contain
letters/phonemes that overlap decently with the reference word's
spelling — score alone doesn't distinguish "this word was actually
said" from "this word's letters happened to acoustically resemble
whatever wrong audio was here." That's a real blind spot, distinct from
the `Insertion`/`Truncated` gap already noted — a confidently-wrong
match, not a low-confidence one.
