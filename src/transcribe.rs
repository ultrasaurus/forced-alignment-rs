//! Independent free-transcription check (Parakeet CTC, via `parakeet-rs`/
//! ONNX Runtime, CPU only for now) — a secondary QA signal alongside the
//! wav2vec2 CTC forced-alignment in [`crate::model`], for catching
//! whole-sentence content mismatches that per-word alignment scoring can't
//! see (see `project-odoru/forced-alignment/artifacts/theory.md`: a
//! wrong-but-real sentence can force-align *confidently*, not just badly).
//!
//! Deliberately minimal: this module answers "what did the audio actually
//! say," full stop. Comparing that against reference text, normalizing
//! numbers/notation, and deciding what counts as a mismatch is a caller
//! concern — those decisions depend on how the reference text was itself
//! normalized before synthesis, which this crate doesn't know about.

use anyhow::{Context, Result};
use parakeet_rs::{Parakeet, Transcriber as _};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const HF_REPO: &str = "onnx-community/parakeet-ctc-0.6b-ONNX";
/// Files needed by `Parakeet::from_pretrained`, which requires a flat
/// directory (all files as direct siblings) — the HF repo itself nests the
/// two largest under `onnx/`, so each entry here is `(repo path, flat
/// filename)`.
const MODEL_FILES: &[(&str, &str)] = &[
    ("onnx/model.onnx", "model.onnx"),
    ("onnx/model.onnx_data", "model.onnx_data"),
    ("tokenizer.json", "tokenizer.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("config.json", "config.json"),
];

/// Downloads (if needed) and returns the flat local directory
/// `Parakeet::from_pretrained` expects. `hf_hub`'s own cache is
/// content-addressed and not flat, so files are copied into a dedicated
/// sibling directory once; subsequent calls are a cheap existence check.
fn ensure_model_dir() -> Result<PathBuf> {
    let cache_root = hf_hub::Cache::from_env().path().clone();
    let flat_dir = cache_root.join("parakeet-ctc-0.6b-flat");
    let marker = flat_dir.join("model.onnx_data");
    if marker.exists() {
        return Ok(flat_dir);
    }

    std::fs::create_dir_all(&flat_dir).with_context(|| format!("creating {}", flat_dir.display()))?;
    let api = hf_hub::api::sync::ApiBuilder::from_env().build()?;
    let repo = api.model(HF_REPO.to_string());
    for (repo_path, flat_name) in MODEL_FILES {
        let downloaded = repo.get(repo_path).with_context(|| format!("downloading {repo_path}"))?;
        let dest = flat_dir.join(flat_name);
        if !dest.exists() {
            std::fs::copy(&downloaded, &dest)
                .with_context(|| format!("copying {} to {}", downloaded.display(), dest.display()))?;
        }
    }
    Ok(flat_dir)
}

/// Process-wide cached model, same rationale as [`crate::model`]'s shared
/// wav2vec2 instance — loading is expensive (device init + weights), so
/// it's done once and shared. `Parakeet::transcribe_samples` takes `&mut
/// self` (no documented thread-safety for concurrent calls on one
/// instance), so callers share one instance serialized behind a `Mutex`
/// rather than each holding their own — acceptable to start with given
/// transcription itself runs well under real-time on CPU (~7x in informal
/// testing); revisit with a small instance pool only if serialization
/// actually shows up as a bottleneck under real concurrency.
static PARAKEET: OnceLock<Mutex<Parakeet>> = OnceLock::new();

fn shared_parakeet() -> Result<&'static Mutex<Parakeet>> {
    if let Some(p) = PARAKEET.get() {
        return Ok(p);
    }
    let model_dir = ensure_model_dir()?;
    let parakeet = Parakeet::from_pretrained(model_dir.to_str().context("model dir path is not valid UTF-8")?, None)
        .map_err(|e| anyhow::anyhow!("loading Parakeet model: {e}"))?;
    // `get_or_init` would double-load on a race; `set` only loses the race
    // gracefully (the other thread's instance wins, this one is dropped) —
    // fine here since loading has no side effects worth deduplicating
    // beyond avoiding the (still one-time) cost twice under a rare race.
    let _ = PARAKEET.set(Mutex::new(parakeet));
    Ok(PARAKEET.get().expect("just set"))
}

/// Transcribes 16kHz mono audio samples independently of any reference
/// text — a free-form ASR pass, not forced alignment. Returns the raw
/// recognized text; deciding whether it matches expected content (and
/// handling any text normalization needed to compare fairly) is the
/// caller's job.
pub fn transcribe(samples: &[f32]) -> Result<String> {
    let parakeet = shared_parakeet()?;
    let mut guard = parakeet.lock().map_err(|_| anyhow::anyhow!("Parakeet model mutex poisoned"))?;
    let result = guard
        .transcribe_samples(samples.to_vec(), crate::SAMPLE_RATE, 1, None)
        .map_err(|e| anyhow::anyhow!("transcribing: {e}"))?;
    Ok(result.text)
}
