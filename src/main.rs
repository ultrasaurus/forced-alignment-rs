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
    let samples = forced_alignment::audio::load_audio(&args.audio, forced_alignment::SAMPLE_RATE)?;
    let (transcript, report) = forced_alignment::align(&samples, &text)?;
    std::fs::write(&args.output, serde_json::to_string_pretty(&transcript)?)?;
    if !report.filtered.is_empty() || !report.suspect.is_empty() {
        eprintln!("{} filtered word(s), {} suspect word(s)", report.filtered.len(), report.suspect.len());
        for w in &report.suspect {
            eprintln!("  suspect [{:?}] {:?} score={:.3}", w.reason, w.word, w.score);
        }
    }
    Ok(())
}
