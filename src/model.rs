use anyhow::Result;
use candle_core::{DType, Device, Tensor, D};
use candle_nn::{
    conv1d_no_bias, group_norm, layer_norm, linear, ops::softmax_last_dim, Conv1d, Conv1dConfig,
    GroupNorm, LayerNorm, Linear, Module, VarBuilder,
};
use std::collections::HashMap;

const HIDDEN_SIZE: usize = 768;
const NUM_LAYERS: usize = 12;
const NUM_HEADS: usize = 12;
const HEAD_DIM: usize = HIDDEN_SIZE / NUM_HEADS;
const INTERMEDIATE_SIZE: usize = 3072;
const CONV_DIM: usize = 512;
const CONV_KERNELS: [usize; 7] = [10, 3, 3, 3, 3, 2, 2];
const CONV_STRIDES: [usize; 7] = [5, 2, 2, 2, 2, 2, 2];
const POS_CONV_KERNEL: usize = 128;
const POS_CONV_GROUPS: usize = 16;
const EPS: f64 = 1e-5;

/// CTC log-probabilities: shape [time_steps, vocab_size].
pub struct Emissions {
    pub log_probs: Vec<Vec<f32>>,
    pub vocab: Vec<String>,
}

struct FeatureExtractor {
    convs: Vec<Conv1d>,
    group_norm: GroupNorm,
}

impl FeatureExtractor {
    fn load(vb: VarBuilder) -> Result<Self> {
        let mut convs = Vec::with_capacity(7);
        for i in 0..7 {
            let in_dim = if i == 0 { 1 } else { CONV_DIM };
            let cfg = Conv1dConfig {
                stride: CONV_STRIDES[i],
                ..Default::default()
            };
            convs.push(conv1d_no_bias(
                in_dim,
                CONV_DIM,
                CONV_KERNELS[i],
                cfg,
                vb.pp(format!("conv_layers.{i}.conv")),
            )?);
        }
        let group_norm = group_norm(CONV_DIM, CONV_DIM, EPS, vb.pp("conv_layers.0.layer_norm"))?;
        Ok(Self { convs, group_norm })
    }

    /// samples: shape (1, 1, T) -> returns (1, T', conv_dim)
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let mut x = self.convs[0].forward(x)?;
        x = self.group_norm.forward(&x)?;
        x = x.gelu_erf()?;
        for conv in &self.convs[1..] {
            x = conv.forward(&x)?;
            x = x.gelu_erf()?;
        }
        Ok(x.transpose(1, 2)?.contiguous()?)
    }
}

struct FeatureProjection {
    layer_norm: LayerNorm,
    projection: Linear,
}

impl FeatureProjection {
    fn load(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            layer_norm: layer_norm(CONV_DIM, EPS, vb.pp("layer_norm"))?,
            projection: linear(CONV_DIM, HIDDEN_SIZE, vb.pp("projection"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let x = self.layer_norm.forward(x)?;
        Ok(self.projection.forward(&x)?)
    }
}

/// Weight-normalized depthwise conv1d used for positional embeddings.
struct PosConvEmbed {
    weight: Tensor,
    bias: Tensor,
    cfg: Conv1dConfig,
}

impl PosConvEmbed {
    fn load(vb: VarBuilder) -> Result<Self> {
        let conv_vb = vb.pp("conv");
        let weight_g = conv_vb.get((1, 1, POS_CONV_KERNEL), "weight_g")?;
        let weight_v = conv_vb.get(
            (HIDDEN_SIZE, HIDDEN_SIZE / POS_CONV_GROUPS, POS_CONV_KERNEL),
            "weight_v",
        )?;
        let bias = conv_vb.get(HIDDEN_SIZE, "bias")?;

        // weight = g * v / ||v|| computed per kernel position over (out, in) dims.
        let norm = weight_v.sqr()?.sum((0, 1))?.sqrt()?.reshape((1, 1, POS_CONV_KERNEL))?;
        let weight = weight_v.broadcast_div(&norm)?.broadcast_mul(&weight_g)?;

        let cfg = Conv1dConfig {
            padding: POS_CONV_KERNEL / 2,
            stride: 1,
            dilation: 1,
            groups: POS_CONV_GROUPS,
            cudnn_fwd_algo: None,
        };
        Ok(Self { weight, bias, cfg })
    }

    /// x: (1, T, hidden) -> (1, T, hidden)
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let x_t = x.transpose(1, 2)?.contiguous()?;
        let conv = Conv1d::new(self.weight.clone(), Some(self.bias.clone()), self.cfg);
        let out = conv.forward(&x_t)?;
        // num_conv_pos_embeddings is even, so SamePadLayer removes the last frame.
        let len = out.dim(2)?;
        let out = out.narrow(2, 0, len - 1)?;
        let out = out.gelu_erf()?;
        Ok(out.transpose(1, 2)?.contiguous()?)
    }
}

struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    out_proj: Linear,
}

impl Attention {
    fn load(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            q_proj: linear(HIDDEN_SIZE, HIDDEN_SIZE, vb.pp("q_proj"))?,
            k_proj: linear(HIDDEN_SIZE, HIDDEN_SIZE, vb.pp("k_proj"))?,
            v_proj: linear(HIDDEN_SIZE, HIDDEN_SIZE, vb.pp("v_proj"))?,
            out_proj: linear(HIDDEN_SIZE, HIDDEN_SIZE, vb.pp("out_proj"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (b, t, _) = x.dims3()?;
        let scale = (HEAD_DIM as f64).powf(-0.5);

        let q = (self.q_proj.forward(x)? * scale)?;
        let k = self.k_proj.forward(x)?;
        let v = self.v_proj.forward(x)?;

        let shape = (b, t, NUM_HEADS, HEAD_DIM);
        let q = q.reshape(shape)?.transpose(1, 2)?.contiguous()?;
        let k = k.reshape(shape)?.transpose(1, 2)?.contiguous()?;
        let v = v.reshape(shape)?.transpose(1, 2)?.contiguous()?;

        let attn = q.matmul(&k.transpose(2, 3)?)?;
        let attn = softmax_last_dim(&attn)?;
        let out = attn.matmul(&v)?;

        let out = out.transpose(1, 2)?.reshape((b, t, HIDDEN_SIZE))?;
        Ok(self.out_proj.forward(&out)?)
    }
}

struct FeedForward {
    intermediate_dense: Linear,
    output_dense: Linear,
}

impl FeedForward {
    fn load(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            intermediate_dense: linear(HIDDEN_SIZE, INTERMEDIATE_SIZE, vb.pp("intermediate_dense"))?,
            output_dense: linear(INTERMEDIATE_SIZE, HIDDEN_SIZE, vb.pp("output_dense"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let x = self.intermediate_dense.forward(x)?;
        let x = x.gelu_erf()?;
        Ok(self.output_dense.forward(&x)?)
    }
}

struct EncoderLayer {
    attention: Attention,
    layer_norm: LayerNorm,
    feed_forward: FeedForward,
    final_layer_norm: LayerNorm,
}

impl EncoderLayer {
    fn load(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            attention: Attention::load(vb.pp("attention"))?,
            layer_norm: layer_norm(HIDDEN_SIZE, EPS, vb.pp("layer_norm"))?,
            feed_forward: FeedForward::load(vb.pp("feed_forward"))?,
            final_layer_norm: layer_norm(HIDDEN_SIZE, EPS, vb.pp("final_layer_norm"))?,
        })
    }

    /// Post-norm layer (do_stable_layer_norm = false).
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let residual = x;
        let attn_out = self.attention.forward(x)?;
        let x = (residual + attn_out)?;
        let x = self.layer_norm.forward(&x)?;
        let ff_out = self.feed_forward.forward(&x)?;
        let x = (&x + ff_out)?;
        Ok(self.final_layer_norm.forward(&x)?)
    }
}

struct Encoder {
    pos_conv_embed: PosConvEmbed,
    layer_norm: LayerNorm,
    layers: Vec<EncoderLayer>,
}

impl Encoder {
    fn load(vb: VarBuilder) -> Result<Self> {
        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for i in 0..NUM_LAYERS {
            layers.push(EncoderLayer::load(vb.pp(format!("layers.{i}")))?);
        }
        Ok(Self {
            pos_conv_embed: PosConvEmbed::load(vb.pp("pos_conv_embed"))?,
            layer_norm: layer_norm(HIDDEN_SIZE, EPS, vb.pp("layer_norm"))?,
            layers,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let pos_emb = self.pos_conv_embed.forward(x)?;
        let mut x = (x + pos_emb)?;
        x = self.layer_norm.forward(&x)?;
        for layer in &self.layers {
            x = layer.forward(&x)?;
        }
        Ok(x)
    }
}

pub struct Wav2Vec2Ctc {
    feature_extractor: FeatureExtractor,
    feature_projection: FeatureProjection,
    encoder: Encoder,
    lm_head: Linear,
    vocab: Vec<String>,
}

impl Wav2Vec2Ctc {
    pub fn load() -> Result<Self> {
        let api = hf_hub::api::sync::Api::new()?;
        let repo = api.model("facebook/wav2vec2-base-960h".to_string());

        let weights_path = repo.get("model.safetensors")?;
        let vocab_path = repo.get("vocab.json")?;

        let vocab_map: HashMap<String, usize> =
            serde_json::from_str(&std::fs::read_to_string(vocab_path)?)?;
        let vocab_size = vocab_map.len();
        let mut vocab = vec![String::new(); vocab_size];
        for (token, id) in vocab_map {
            vocab[id] = token;
        }

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &Device::Cpu)?
        };
        let vb = vb.pp("wav2vec2");

        Ok(Self {
            feature_extractor: FeatureExtractor::load(vb.pp("feature_extractor"))?,
            feature_projection: FeatureProjection::load(vb.pp("feature_projection"))?,
            encoder: Encoder::load(vb.pp("encoder"))?,
            lm_head: linear(HIDDEN_SIZE, vocab_size, vb.root().pp("lm_head"))?,
            vocab,
        })
    }

    pub fn forward(&self, samples: &[f32]) -> Result<Emissions> {
        // Wav2Vec2 expects zero-mean, unit-variance input.
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let var = samples.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / samples.len() as f32;
        let std = var.sqrt();
        let normalized: Vec<f32> = samples.iter().map(|s| (s - mean) / (std + 1e-7)).collect();

        let x = Tensor::from_vec(normalized, (1, 1, samples.len()), &Device::Cpu)?;
        let x = self.feature_extractor.forward(&x)?;
        let x = self.feature_projection.forward(&x)?;
        let x = self.encoder.forward(&x)?;
        let logits = self.lm_head.forward(&x)?;
        let log_probs = candle_nn::ops::log_softmax(&logits, D::Minus1)?;

        let log_probs = log_probs.squeeze(0)?;
        let (t, v) = log_probs.dims2()?;
        let data = log_probs.to_vec2::<f32>()?;
        debug_assert_eq!((t, v), (data.len(), data[0].len()));

        Ok(Emissions {
            log_probs: data,
            vocab: self.vocab.clone(),
        })
    }
}

// Total stride of the conv feature extractor: 5*2*2*2*2*2*2 = 320 samples/frame.
const SAMPLES_PER_FRAME: usize = 320;
const SAMPLE_RATE: usize = 16_000;
// wav2vec2 self-attention is O(T^2), so long audio must be processed in chunks.
const CHUNK_SECONDS: usize = 20;
const OVERLAP_SECONDS: usize = 1;

/// Load the wav2vec2 model and run inference on audio samples (16kHz mono).
///
/// Long audio is processed in overlapping chunks (attention is O(T^2), so a single
/// multi-minute pass is infeasible); the overlap is trimmed from each chunk's
/// emissions before concatenation so frame timing stays uniform.
pub fn run_inference(samples: &[f32]) -> Result<Emissions> {
    let model = Wav2Vec2Ctc::load()?;

    let chunk_samples = CHUNK_SECONDS * SAMPLE_RATE;
    let overlap_samples = OVERLAP_SECONDS * SAMPLE_RATE;
    let overlap_frames = overlap_samples / SAMPLES_PER_FRAME;

    if samples.len() <= chunk_samples {
        return model.forward(samples);
    }

    let stride_samples = chunk_samples - overlap_samples;
    let mut log_probs = Vec::new();
    let mut vocab = Vec::new();

    let mut start = 0;
    while start < samples.len() {
        let end = (start + chunk_samples).min(samples.len());
        let chunk = &samples[start..end];
        let is_first = start == 0;
        let is_last = end == samples.len();

        let emissions = model.forward(chunk)?;
        vocab = emissions.vocab;

        let n = emissions.log_probs.len();
        let half_overlap = overlap_frames / 2;
        let trim_start = if is_first { 0 } else { half_overlap };
        let trim_end = if is_last { n } else { n.saturating_sub(half_overlap) };
        log_probs.extend(emissions.log_probs[trim_start..trim_end].iter().cloned());

        if is_last {
            break;
        }
        start += stride_samples;
    }

    Ok(Emissions { log_probs, vocab })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::load_audio;
    use std::path::Path;

    #[test]
    #[ignore = "slow in debug builds (~90s); run with `cargo test --release -- --ignored`"]
    fn runs_on_short_sentence() {
        let samples = load_audio(Path::new("samples/short-sentence.mp3"), 16_000).unwrap();
        let emissions = run_inference(&samples).unwrap();
        println!("frames={} vocab={}", emissions.log_probs.len(), emissions.vocab.len());

        // Greedy CTC decode for a sanity check.
        let mut prev = usize::MAX;
        let mut out = String::new();
        for frame in &emissions.log_probs {
            let (idx, _) = frame.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
            if idx != prev {
                let tok = &emissions.vocab[idx];
                if tok != "<pad>" {
                    out.push_str(tok);
                }
            }
            prev = idx;
        }
        println!("decoded: {out}");
        assert!(out.to_lowercase().contains("contain"));
    }
}
