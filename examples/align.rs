//! Align an audio file against its text and write the raw result: the
//! `Transcript` (per-word times and scores) and the `AlignReport`, as JSON
//! `{"transcript": ..., "report": ...}`.
//!
//! ```sh
//! cargo run --release --example align -- <audio> <text> [-o out.json]
//! ```
//!
//! The text goes to `align` as it is, with no normalizing, so this shows
//! exactly what the aligner does with it. Without `-o` the JSON goes to
//! stdout. A one-line summary of the report goes to stderr.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

const USAGE: &str = "usage: align <audio> <text> [-o out.json]";

fn main() -> Result<()> {
    let mut positional: Vec<PathBuf> = Vec::new();
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-o" || arg == "--output" {
            output = Some(args.next().context(USAGE)?.into());
        } else if arg == "-h" || arg == "--help" {
            println!("{USAGE}");
            return Ok(());
        } else {
            positional.push(arg.into());
        }
    }
    let [audio, text] = positional.as_slice() else {
        bail!(USAGE)
    };

    let text = std::fs::read_to_string(text).with_context(|| text.display().to_string())?;
    let samples = forced_alignment::audio::load_audio(audio, forced_alignment::SAMPLE_RATE)
        .with_context(|| audio.display().to_string())?;
    let (transcript, report) = forced_alignment::align(&samples, &text)?;

    let json = serde_json::to_string_pretty(
        &serde_json::json!({ "transcript": transcript, "report": report }),
    )?;
    match &output {
        Some(path) => {
            std::fs::write(path, json + "\n").with_context(|| path.display().to_string())?
        }
        None => println!("{json}"),
    }
    let words: usize = transcript.segments.iter().map(|s| s.words.len()).sum();
    eprintln!(
        "{words} words aligned; report: {} insertions, {} suspect, {} filtered",
        report.insertions.len(),
        report.suspect.len(),
        report.filtered.len(),
    );
    Ok(())
}
