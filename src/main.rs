mod align;
mod audio;
mod model;
mod output;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// Path to audio file (mp3/wav).
    audio: PathBuf,
    /// Path to text file with the known transcript.
    text: PathBuf,
    /// Output JSON path.
    #[arg(short, long, default_value = "out.json")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let text = std::fs::read_to_string(&args.text)?;

    let samples = audio::load_audio(&args.audio, 16_000)?;
    let duration_secs = samples.len() as f32 / 16_000.0;
    let emissions = model::run_inference(&samples)?;
    let words = align::align(&emissions, &text, duration_secs)?;

    let result = output::AlignmentResult {
        segments: vec![output::Segment {
            start: words.first().map(|w| w.start).unwrap_or(0.0),
            end: words.last().map(|w| w.end).unwrap_or(0.0),
            text,
            words,
        }],
    };

    std::fs::write(&args.output, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}
