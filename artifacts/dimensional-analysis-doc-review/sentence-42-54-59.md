# Sentences 42, 54, 59 — pulled via `dl cache-path`

Doc `0d594437-9810-45af-aed9-c73cf964d84a`, voice `vibe:Jack`.

## Sentence 42 — `"int"` (44.83s-44.99s)

Text: `"x at index n denotes a - discrete-timesignal. n must be an
integer: 0, 1, 2, negative 1, negative 2, etc. We read this as "the nth
sample of signal x.""`

Heavily notation/number-laden (`x at index n`, `0, 1, 2, negative 1,
negative 2, etc.`), and the sentence's own real vocabulary literally
includes **"integer"** — `"int"` is very plausibly just a low-confidence
partial decode of the real word "integer" itself (already present in
the text), not extraneous leaked content. Per-word scores in `meta.json`
are otherwise high (0.89-0.9999) — this is a normal, well-aligned
sentence with one rough patch on unusual technical phrasing, not a
Class 2 (mystery) case. **Fix: `tts_overrides.txt` entry for how to
read this kind of index/integer notation.**

## Sentence 54 — `"plus"` (31.57s-31.85s)

Text: `"Most of what we do in signal processing amounts to modifying a
signal in some way, for example, applying a low-pass filter to remove
high-frequency content."`

Ordinary prose, no math notation. No obvious source for "plus" in the
text. Matches the earlier "sounded fine on listen" judgment — likely
harmless/expected natural-reading variance, not evidence of anything
broken.

## Sentence 59 — `"iece"` (81.25s-81.37s, marked "seems fine")

Text: `"We'll often use x of t and y of t to generally refer to input
and output signals, respectively."`

Also notation-heavy (`x of t`, `y of t` — math functions read as
words). Same shape as sentence 42: unusual technical phrasing, brief
rough patch, user judged it "seems fine" on listen. **Same
`tts_overrides.txt`-style fix candidate** if it turns out to need one at
all.

## Sentence 113 — not found in this doc

`dl cache-path 0d594437-9810-45af-aed9-c73cf964d84a 113` errored:
"sentence 113 not found in document". The "1.4. Audio and signals"
section (where `"freight"`/113 and `"iece"`/59... wait 59 *is* found
here, so 113 belongs to a different document than 42/54/59, despite
being logged under the same `index.md` list). Need the correct doc id
for "1.4. Audio and signals" before pulling sentence 113/`leaked-audio-freight`.

## Takeaway

42 and 59 both strongly confirm the numeric/math-vocabulary correlation
theory — both are heavily notation-laden sentences with otherwise-clean
alignment and one rough patch, i.e. **Class 1, not Class 2**. 54 doesn't
fit that pattern (ordinary prose) but was independently judged harmless.
None of these three are evidence of the batch-misrouting mystery —
useful negative data point, and a concrete, mundane fix path
(`tts_overrides.txt`) for at least the notation-heavy ones.
