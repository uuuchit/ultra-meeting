//! Phase 0 Spike: Audio stability over 60+ minutes, device disconnect handling.
//!
//! Pass criteria: no crash, no unbounded memory growth, disconnect produces
//! detectable error.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

const TARGET_DURATION_MIN: u64 = 60;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 1;
const CHUNK_DURATION_SEC: u64 = 5;
const SAMPLES_PER_CHUNK: usize = (SAMPLE_RATE as u64 * CHUNK_DURATION_SEC) as usize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("no default input device");
    let config = device
        .default_input_config()
        .expect("no default input config");

    let device_name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".to_string());
    info!("Device: {}", device_name);
    info!("Config: {:?}", config);

    let output_dir = std::env::temp_dir().join("ultra-meeting-spike-audio");
    std::fs::create_dir_all(&output_dir)?;
    let wav_path = output_dir.join("spike_capture.wav");

    let spec = WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = Arc::new(std::sync::Mutex::new(
        WavWriter::create(&wav_path, spec).expect("create wav writer"),
    ));

    let sample_count = Arc::new(AtomicU64::new(0));
    let error_occurred = Arc::new(AtomicBool::new(false));
    let start = Instant::now();

    let config = config.into();
    let err_fn = {
        let error_occurred = error_occurred.clone();
        move |e: cpal::StreamError| {
            error!("Stream error: {}", e);
            error_occurred.store(true, Ordering::SeqCst);
        }
    };

    let writer_clone = writer.clone();
    let sample_count_clone = sample_count.clone();
    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut w = writer_clone.lock().unwrap();
            for &s in data {
                let clamped = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
                let _ = w.write_sample(clamped);
            }
            sample_count_clone.fetch_add(data.len() as u64, Ordering::Relaxed);
        },
        err_fn,
        None,
    )?;

    stream.play()?;
    info!("Recording started. Target: {} minutes", TARGET_DURATION_MIN);

    let target_duration = Duration::from_secs(TARGET_DURATION_MIN * 60);
    while start.elapsed() < target_duration {
        std::thread::sleep(Duration::from_secs(10));
        let elapsed = start.elapsed();
        let samples = sample_count.load(Ordering::Relaxed);
        let expected = SAMPLE_RATE as u64 * elapsed.as_secs() * CHANNELS as u64;
        let dropout = expected.saturating_sub(samples);
        if dropout > SAMPLE_RATE as u64 {
            warn!(
                "Possible dropout: expected ~{} samples, got {} (diff {})",
                expected, samples, dropout
            );
        }
        if error_occurred.load(Ordering::SeqCst) {
            error!("Stream error detected; stopping");
            break;
        }
        info!(
            "Elapsed: {:?} | Samples: {} | Status: {}",
            elapsed,
            samples,
            if error_occurred.load(Ordering::SeqCst) {
                "ERROR"
            } else {
                "OK"
            }
        );
    }

    drop(stream);
    let writer_final = match Arc::try_unwrap(writer) {
        Ok(mutex) => mutex.into_inner().expect("Mutex unwrap failed"),
        Err(_) => panic!("Arc unwrap failed - still has references"),
    };
    writer_final.finalize()?;

    let total = sample_count.load(Ordering::Relaxed);
    info!(
        "Spike complete. Total samples: {}, duration: {:?}, output: {:?}",
        total,
        start.elapsed(),
        wav_path
    );

    Ok(())
}
