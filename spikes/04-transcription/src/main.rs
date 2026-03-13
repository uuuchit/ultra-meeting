//! Phase 0 Spike: Transcription throughput benchmark.
//!
//! Benchmarks whisper base.en on target hardware.
//! Pass criteria: RTF ≤ 0.6

use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let model_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ultra-meeting/models");
    std::fs::create_dir_all(&model_dir)?;

    let model_path = model_dir.join("ggml-base.en.bin");
    if !model_path.exists() {
        info!("Downloading base.en model (~142MB)...");
        let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
        let bytes = reqwest::blocking::get(url)?.bytes()?;
        std::fs::write(&model_path, &bytes)?;
        info!("Model saved to {:?}", model_path);
    }

    let audio_paths: Vec<PathBuf> = [
        "samples/jfk.wav",
        "../samples/jfk.wav",
        "jfk.wav",
        "/tmp/jfk.wav",
    ]
    .iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect();

    let audio_path = audio_paths.first().cloned().ok_or_else(|| {
        "No audio file found. Create samples/jfk.wav or run: curl -L -o jfk.wav 'https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav'"
    })?;

    info!("Model: {:?}", model_path);
    info!("Audio: {:?}", audio_path);

    let (audio_f32, sample_rate, duration_sec) = load_wav(&audio_path)?;
    let audio_f32_16k = if sample_rate != 16000 {
        resample_to_16k(&audio_f32, sample_rate)?
    } else {
        audio_f32
    };

    let load_start = Instant::now();
    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .map_err(|e| format!("Failed to load model: {}", e))?;
    let load_elapsed = load_start.elapsed();
    info!("Model load: {:?}", load_elapsed);

    let mut state = ctx.create_state().map_err(|e| format!("Create state: {}", e))?;

    let transcribe_start = Instant::now();
    let mut params = FullParams::new(SamplingStrategy::Greedy);
    params.set_translate(false);
    params.set_print_realtime(false);
    params.set_print_progress(false);

    state.full(params, &audio_f32_16k).map_err(|e| format!("Transcribe: {}", e))?;
    let transcribe_elapsed = transcribe_start.elapsed();
    let transcribe_sec = transcribe_elapsed.as_secs_f64();

    let rtf = transcribe_sec / duration_sec;
    let pass = rtf <= 0.6;

    info!(
        "Audio duration: {:.2}s | Transcribe: {:.2}s | RTF: {:.3} | Pass (≤0.6): {}",
        duration_sec, transcribe_sec, rtf, pass
    );

    let num_segments = state.full_n_segments();
    info!("Segments: {}", num_segments);
    for i in 0..num_segments.min(3) {
        let text = state.full_get_segment_text(i).unwrap_or_default();
        info!("  [{}] {}", i, text.trim());
    }

    Ok(())
}

fn load_wav(path: &std::path::Path) -> Result<(Vec<f32>, u32, f64), Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let duration_sec = samples.len() as f64 / spec.sample_rate as f64 / spec.channels as f64;
    let mono: Vec<i16> = if spec.channels > 1 {
        samples.chunks(spec.channels as usize).map(|c| c[0]).collect()
    } else {
        samples
    };
    let f32_samples: Vec<f32> = mono.iter().map(|&s| s as f32 / 32768.0).collect();
    Ok((f32_samples, spec.sample_rate, duration_sec))
}

fn resample_to_16k(audio: &[f32], from_rate: u32) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if from_rate == 16000 {
        return Ok(audio.to_vec());
    }
    let ratio = 16000.0 / from_rate as f32;
    let new_len = (audio.len() as f32 * ratio) as usize;
    let mut out = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_idx = i as f32 / ratio;
        let lo = src_idx.floor() as usize;
        let hi = (lo + 1).min(audio.len() - 1);
        let t = src_idx - lo as f32;
        let v = audio[lo] * (1.0 - t) + audio[hi] * t;
        out.push(v);
    }
    Ok(out)
}
