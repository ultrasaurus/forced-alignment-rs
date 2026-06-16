use anyhow::Result;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decode an audio file to mono f32 samples at the given target sample rate.
pub fn load_audio(path: &Path, target_sample_rate: u32) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow::anyhow!("no supported audio track found"))?;
    let track_id = track.id;
    let source_sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow::anyhow!("unknown sample rate"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let mut mono: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        let spec = *decoded.spec();
        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        let channels = spec.channels.count();
        let samples = sample_buf.samples();
        if channels == 1 {
            mono.extend_from_slice(samples);
        } else {
            for frame in samples.chunks(channels) {
                let sum: f32 = frame.iter().sum();
                mono.push(sum / channels as f32);
            }
        }
    }

    if source_sample_rate == target_sample_rate {
        return Ok(mono);
    }
    resample(&mono, source_sample_rate, target_sample_rate)
}

fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1,
    )?;

    let waves_in = vec![samples.to_vec()];
    let waves_out = resampler.process(&waves_in, None)?;
    Ok(waves_out[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_short_sentence() {
        let samples = load_audio(Path::new("validation-samples/short-sentence.mp3"), 16_000).unwrap();
        assert!(!samples.is_empty());
        println!("decoded {} samples ({:.2}s)", samples.len(), samples.len() as f32 / 16_000.0);
    }
}
