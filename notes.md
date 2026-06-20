# Notes

Observations that didn't fit neatly into the main docs.

## Score is not a quality score

Word score is the mean CTC token probability across frames assigned to that
word — it measures how well the acoustic model found each letter in the audio
frames, not pronunciation quality. A word can score low because of
silence or noise in adjacent frames, not because it was mispronounced or
missing. Treat low scores as a signal to investigate, not a verdict.

## Short common words can score well in silence (false negatives)

Words with high-frequency letters (e.g. "the" — T, H, E) can score well in
silence or noise at the end of truncated audio, because the CTC model happens
to emit those letter probabilities for ambient sound. Observed: "the" scored
0.658 in silence after the audio ended, while "at" (also absent) scored 0.001.

Truncation of a single word is not reliably detectable. A *run* of
low-scoring tail words is the reliable signal — the more consecutive suspects
near the end of the audio, the more confident the truncation diagnosis.
