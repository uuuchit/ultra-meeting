//! Microphone capture: cpal + rtrb ring buffer + chunked WAV writer.
//!
//! Production-hardened: format conversion, overrun detection, disk-full handling,
//! device disconnect propagation, and capture metrics.

use crate::error::RecordingError;
use crate::storage::SessionStorage;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig, SupportedStreamConfig};
use hound::{WavSpec, WavWriter};
use rtrb::{Producer, PushError, RingBuffer};
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 1;
const CHUNK_DURATION_SEC: u64 = 5;
const SAMPLES_PER_CHUNK: usize = (SAMPLE_RATE as u64 * CHUNK_DURATION_SEC) as usize;
const RING_CAPACITY: usize = SAMPLES_PER_CHUNK * 4;

pub struct MicCapture {
    stop: Arc<AtomicBool>,
    sample_count: Arc<AtomicU64>,
    overrun_count: Arc<AtomicU64>,
    _stream: Option<cpal::Stream>,
    _worker: Option<thread::JoinHandle<()>>,
}

impl MicCapture {
    pub fn start(
        storage: Arc<Mutex<SessionStorage>>,
        device_name: Option<String>,
    ) -> Result<Self, RecordingError> {
        let host = cpal::default_host();
        let device = device_name
            .and_then(|n| {
                host.input_devices()
                    .ok()?
                    .find(|d| d.description().ok().map(|x| x.name() == n).unwrap_or(false))
            })
            .or_else(|| host.default_input_device())
            .ok_or_else(|| RecordingError::DeviceNotFound("no input device".into()))?;

        let device_desc = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        info!("Mic device: {}", device_desc);

        let (config, use_f32) = select_input_config(&device)?;

        let (mut producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let sample_count = Arc::new(AtomicU64::new(0));
        let overrun_count = Arc::new(AtomicU64::new(0));

        let stop_worker = stop.clone();
        let sample_count_worker = sample_count.clone();
        let overrun_worker = overrun_count.clone();

        let worker = thread::spawn(move || {
            run_wav_writer(consumer, storage, stop_worker, sample_count_worker, overrun_worker);
        });

        let err_fn = {
            let stop = stop.clone();
            move |e: cpal::StreamError| {
                error!("Mic stream error (device disconnect or backend error): {}", e);
                stop.store(true, Ordering::SeqCst);
            }
        };

        let stream = if use_f32 {
            let sample_count_cb = sample_count.clone();
            let overrun_cb = overrun_count.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        push_samples_f32(&mut producer, data, &sample_count_cb, &overrun_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?
        } else {
            let sample_count_cb = sample_count.clone();
            let overrun_cb = overrun_count.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        push_samples_i16(&mut producer, data, &sample_count_cb, &overrun_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?
        };

        stream
            .play()
            .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?;
        info!("Mic capture started (format: {})", if use_f32 { "f32" } else { "i16" });

        Ok(Self {
            stop,
            sample_count,
            overrun_count,
            _stream: Some(stream),
            _worker: Some(worker),
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self._stream = None;
        if let Some(h) = self._worker.take() {
            let _ = h.join();
        }
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count.load(Ordering::Relaxed)
    }

    pub fn overrun_count(&self) -> u64 {
        self.overrun_count.load(Ordering::Relaxed)
    }
}

fn select_input_config(device: &cpal::Device) -> Result<(StreamConfig, bool), RecordingError> {
    let default = device
        .default_input_config()
        .map_err(|e| RecordingError::DeviceNotFound(e.to_string()))?;

    if default.sample_format() == SampleFormat::F32 {
        return Ok((default.into(), true));
    }

    let mut preferred: Option<SupportedStreamConfig> = None;
    for range in device
        .supported_input_configs()
        .map_err(|e| RecordingError::DeviceNotFound(e.to_string()))?
    {
        if range.sample_format() == SampleFormat::F32 {
            if let Some(cfg) = range.try_with_sample_rate(SAMPLE_RATE) {
                preferred = Some(cfg);
                break;
            }
            if preferred.is_none() {
                preferred = Some(range.with_max_sample_rate());
            }
        }
    }

    if let Some(cfg) = preferred {
        return Ok((cfg.into(), true));
    }

    let use_f32 = default.sample_format() == SampleFormat::F32;
    Ok((default.into(), use_f32))
}

fn push_samples_f32(
    producer: &mut Producer<f32>,
    data: &[f32],
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
) {
    for &s in data {
        match producer.push(s) {
            Ok(()) => {}
            Err(PushError::Full(_)) => {
                let n = overrun_count.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 {
                    warn!("Ring buffer overrun (backpressure): disk writer cannot keep up");
                }
            }
        }
    }
    sample_count.fetch_add(data.len() as u64, Ordering::Relaxed);
}

fn push_samples_i16(
    producer: &mut Producer<f32>,
    data: &[i16],
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
) {
    for &s in data {
        let f = s as f32 / 32768.0;
        match producer.push(f) {
            Ok(()) => {}
            Err(PushError::Full(_)) => {
                let n = overrun_count.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 {
                    warn!("Ring buffer overrun (backpressure): disk writer cannot keep up");
                }
            }
        }
    }
    sample_count.fetch_add(data.len() as u64, Ordering::Relaxed);
}

fn run_wav_writer(
    mut consumer: rtrb::Consumer<f32>,
    storage: Arc<Mutex<SessionStorage>>,
    stop: Arc<AtomicBool>,
    _sample_count: Arc<AtomicU64>,
    overrun_count: Arc<AtomicU64>,
) {
    let spec = WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut chunk_index: u32 = 0;
    let mut buffer = Vec::with_capacity(SAMPLES_PER_CHUNK);
    let mut chunk_buf = Vec::with_capacity(SAMPLES_PER_CHUNK);

    while !stop.load(Ordering::Relaxed) {
        match consumer.pop() {
            Ok(s) => buffer.push(s),
            Err(_) => {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
        }

        while buffer.len() >= SAMPLES_PER_CHUNK {
            chunk_buf.clear();
            chunk_buf.extend(buffer.drain(..SAMPLES_PER_CHUNK));
            let total_start = Instant::now();

            let (path, metadata_elapsed) = {
                let mut guard = storage.lock().unwrap();
                let path = guard.mic_chunk_path(chunk_index);
                guard.metadata.audio.mic_chunks = chunk_index + 1;
                let meta_start = Instant::now();
                // Write metadata at most every 60 seconds (chunks 0,12,24,...) to reduce disk churn
                let meta_elapsed = if chunk_index % 12 == 0 {
                    if let Ok(yaml) = serde_yaml::to_string(&guard.metadata) {
                        let _ = std::fs::write(guard.root.join("metadata.yaml"), yaml);
                    }
                    meta_start.elapsed()
                } else {
                    meta_start.elapsed()
                };
                (path, meta_elapsed)
            };

            let wav_start = Instant::now();
            let write_result = write_wav_chunk(&path, &chunk_buf, &spec);
            let wav_elapsed = wav_start.elapsed();
            let total_elapsed = total_start.elapsed();

            if let Err(e) = write_result {
                if e.to_string().contains("No space left") || matches!(e, RecordingError::Io(ref io) if io.kind() == ErrorKind::StorageFull) {
                    error!("Disk full during mic chunk write");
                }
            }

            if chunk_index % 12 == 0 {
                info!(
                    chunk_index = chunk_index,
                    total_ms = total_elapsed.as_millis(),
                    wav_ms = wav_elapsed.as_millis(),
                    metadata_ms = metadata_elapsed.as_millis(),
                    "Mic chunk flush metrics"
                );
            } else {
                debug!(
                    chunk_index = chunk_index,
                    total_ms = total_elapsed.as_millis(),
                    wav_ms = wav_elapsed.as_millis(),
                    metadata_ms = metadata_elapsed.as_millis(),
                    "Mic chunk flush"
                );
            }
            chunk_index += 1;
        }
    }

    let overruns = overrun_count.load(Ordering::Relaxed);
    if overruns > 0 {
        warn!("Mic capture had {} sample overruns (ring buffer full)", overruns);
    }

    if !buffer.is_empty() {
        let mut guard = storage.lock().unwrap();
        let path = guard.mic_chunk_path(chunk_index);
        guard.metadata.audio.mic_chunks = chunk_index + 1;
        drop(guard);
        let _ = write_wav_chunk(&path, &buffer, &spec);
        // Final metadata write on stop for crash safety
        if let Ok(g) = storage.lock() {
            if let Ok(yaml) = serde_yaml::to_string(&g.metadata) {
                let _ = std::fs::write(g.root.join("metadata.yaml"), yaml);
            }
        }
    }
}

fn write_wav_chunk(
    path: &std::path::Path,
    samples: &[f32],
    spec: &WavSpec,
) -> Result<(), RecordingError> {
    let mut w = WavWriter::create(path, *spec).map_err(|e| map_hound_error(e))?;
    for s in samples {
        let clamped = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        w.write_sample(clamped).map_err(map_hound_error)?;
    }
    w.finalize().map_err(map_hound_error)?;
    Ok(())
}

fn map_hound_error(e: hound::Error) -> RecordingError {
    let msg = e.to_string();
    if msg.contains("No space left") || msg.contains("Disk full") {
        RecordingError::DiskFull
    } else if let hound::Error::IoError(io) = e {
        if io.kind() == ErrorKind::StorageFull {
            RecordingError::DiskFull
        } else {
            RecordingError::Io(io)
        }
    } else {
        RecordingError::Other(msg)
    }
}
