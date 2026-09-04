# leaked-audio-empty-decode

v0.3.2 - fixes false-positive "leaked audio" warnings with empty string where 
the aligner's greedy decode of the inserted frame span was empty or
whitespace-only. 

Gathered to validate the forced-alignment fix (ctc.rs) that now drops such 
empty-decode insertions entirely rather than surfacing them as warnings.

Each entry: audio/sentence-<id>.mp3 + sentence-<id>.json (Meta-shaped
sidecar: `text` is the normalized/spoken form, `original` the raw source
sentence, `normalized_hash` verified against the live audio-cache entry's
own hash before writing).

| sentence | doc | warning |
|---|---|---|
| 1200 | The Souls of Black Folk | `leaked audio: "" (37.85s-37.97s)` |
| 1999 | The Souls of Black Folk | `leaked audio: "" (65.33s-65.47s)` |
| 2322 | The Souls of Black Folk | `leaked audio: "" (82.92s-83.04s)` |
| 2621 | The Souls of Black Folk | `leaked audio: "" (57.88s-58.00s)` |
| 1438 | A Room of One's Own | `leaked audio: "" (48.95s-49.07s)` |
