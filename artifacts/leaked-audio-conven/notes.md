# leaked-audio-conven

## Reported (2026-08-24)

- Insertion warning: leaked audio `"conven"` — nothing in the whole sentence matches.
- mp3: `/Users/sallen/.odoru/audio/296b2d090104983fbd9209fb9dd7c858e61cab8bfe21b6ba948a0fcc72c84ac5.mp3`
- meta: `/Users/sallen/.odoru/audio/296b2d090104983fbd9209fb9dd7c858e61cab8bfe21b6ba948a0fcc72c84ac5.json`

## Notes

- Sentence text (from `meta.json`): "By this, I mean that these ideas are
  the base level upon which we will build." — confirmed, no substring
  resembling "conven" anywhere in it.
- Oddity in `meta.json`'s per-word scores: almost every word in this
  sentence scores very low (e.g. "By" 0.0001, "I" 0.0000077, "we"
  0.0002, "will" 0.00002) — only "the" scores high (0.9989). That's not
  what a normal, well-aligned TTS sentence looks like (per
  `report.md`, clean speech should consistently score 0.8+). Worth
  checking whether this whole sentence's audio is itself suspect
  (e.g. quiet/rushed TTS output), independent of the leaked-"conven"
  finding — a systematically low-confidence sentence could also produce
  spurious insertions if the filler state ends up looking relatively
  more attractive than a very weak match to the real word.
- Next step: listen to the actual mp3 to see what's audible around
  where "conven" was reported, and check whether the whole sentence's
  audio sounds normal or degraded.
