# Findings table — authoritative, from `vibe_sync.json`'s `align_warnings`

All sentence ids below are ground truth (from `~/.odoru/documents/<doc>/vibe_sync.json`'s
`align_warnings` map, not guessed/timestamp-matched). Voice: Jack throughout.

## `0d594437-9810-45af-aed9-c73cf964d84a` — "1.1. Preliminaries" (`preliminaries-doc/`)

| sentence | decoded | sentence text | classification |
|---|---|---|---|
| 12 | "you wa" | `"Note"` | **1a: heading-bleed** (ref clip "you want", confirmed by ear) |
| 14 | "conven" | `"By this, I mean that these ideas are the base level upon which we will build."` | **class 2c: pure hallucination** — whole clip is nonsense ("Mote by mechanical notation standard conventions...") echoing document-register vocabulary, not traceable to ref clip or any real source |
| 15 | "much" | `"This is not the same thing as being easy."` | **2: whole-sentence swap victim** — this sentence's own audio was found reused as sentence 20's wrong audio elsewhere in the doc; own scores good (0.75-0.9999) |
| 16 | "u" | `"Many basic ideas can be complicated, and take quite some time to thoroughly understand."` | possible 1b (fragment of "understand"?) |
| 18 | "ideas" | `"Just take it slow, and don't move on to the next section too soon."` | **likely document-repeat**: doesn't contain "ideas" itself, but sentences 13/16/19 nearby all do |
| 19 | "ch we" | `"None of the contents here are magical: it all builds on basic ideas."` | unresolved; also near the "ideas" cluster |
| 48 | "ire" | long sentence re: `*` operator / convolution | unresolved |
| 56 | "hum" | `"y equals g of x ."`  (very short, math notation) | possible 1b |
| 67 | "ea" | sentence re: signals being 0-valued for negative t/n | possible 1b (math-notation-heavy) |

(sentence 13, `"I sometimes use the term basic when describing certain ideas."` — **no warning at all**, despite bad audio; see `missing-flag-basic-ideas`. Note it also contains "ideas.")

## `f8aaa0e1-7702-4fd7-b5de-8d3eaf99e9fe` — labeled "1.3. Units and dimensional analysis" in `index.md` (`units-dimensional-analysis-doc/`)

| sentence | decoded | sentence text | classification |
|---|---|---|---|
| 42 | "int" | `"1.3.2."` (a heading) | **1a: heading-bleed**, same shape as sentence 12. **Correction**: earlier notes analyzed the wrong sentence here (pulled from doc `0d594437` by mistake, got unrelated math-notation text) — that analysis is void, see below. |
| 54 | "plus" | Java/C++ paragraph re: units/semantics of variables | user confirmed "sounded fine" on listen — likely benign |

## `f40578d5-d9c5-4796-80b9-b11d0f5b4e4d` — "1.2. Periodicity and waves" (`periodicity-waves-doc/`)

| sentence | decoded | sentence text | classification |
|---|---|---|---|
| 10 | "re" | `"Basic properties of waves"` (heading) | **1a: heading-bleed** |
| 18 | "sig" | `"...period of a signal..."` | **1b: real-word fragment** — sentence contains "signal" |
| 19 | "stesc" | `"Example (Pulse train)"` (heading) | **1a: heading-bleed** |
| 53 | "over" | `"Definition 1.2 (Fundamental frequency)"` (heading) | **1a: heading-bleed** |
| 54 | "frequenc" | `"...its fundamental frequency is defined as"` | **1b: real-word fragment** — sentence contains "frequency" |
| 110 | "gemi" | `"x of t equals cosine of theta of t, y of t equals sine of theta of t."` | notation-heavy, unresolved |
| 111 | "prst" | `"This process is illustrated by Fig. 1.8."` | **possible 1b** — sentence contains "process" |
| 166 | "zon" | trig identity sentence (sine/cosine, repetitive) | notation-heavy, unresolved |

## `cb32fa2a-34ef-4384-b6e4-cf4a6de56ad5` — "1.4. Audio and signals" (`leaked-audio-freight/`)

| sentence | decoded | sentence text | classification |
|---|---|---|---|
| 59 | "iece" | `"We'll often use x of t and y of t..."` | notation-heavy, "seems fine" per user |
| 113 | "freight" | `"...divide each semitone range evenly into 100 pieces..."` | **confirmed real TTS artifact**; contains "pieces" (not an obvious match, but notation-heavy) |
| 114 | "anhor" | `"A change in frequency of 1¢...multiplicative factor..."` | heavily notation-heavy |

## Correction to `dimensional-analysis-doc-review/sentence-42-54-59.md`

That file's analysis of sentences 42 and 54 used the wrong doc id
(`0d594437` instead of `f8aaa0e1`) and so analyzed the wrong sentences
entirely — coincidence made the wrong sentence 42 *also* look
math-notation-heavy, which is why the mistake wasn't obvious at the
time. Superseded by this table. Sentence 59's analysis (doc `cb32fa2a`)
was correct.
