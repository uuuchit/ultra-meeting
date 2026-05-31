//! Microphone capture: cpal + rtrb ring buffer + chunked WAV writer.
//!
//! Production-hardened: format conversion, overrun detection, disk-full handling,
//! device disconnect propagation, and capture metrics.

use crate::error::RecordingError;
use crate::storage::SessionStorage;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
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
    worker_error: Arc<Mutex<Option<RecordingError>>>,
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

        let (config, sample_format) = select_input_config(&device)?;
        let input_channels = usize::from(config.channels.max(1));

        let (mut producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let sample_count = Arc::new(AtomicU64::new(0));
        let overrun_count = Arc::new(AtomicU64::new(0));
        let worker_error = Arc::new(Mutex::new(None));

        let stop_worker = stop.clone();
        let sample_count_worker = sample_count.clone();
        let overrun_worker = overrun_count.clone();
        let worker_error_worker = worker_error.clone();

        let worker = thread::spawn(move || {
            run_wav_writer(
                consumer,
                storage,
                stop_worker,
                sample_count_worker,
                overrun_worker,
                worker_error_worker,
            );
        });

        let err_fn = {
            let stop = stop.clone();
            move |e: cpal::StreamError| {
                error!(
                    "Mic stream error (device disconnect or backend error): {}",
                    e
                );
                stop.store(true, Ordering::SeqCst);
            }
        };

        let stream = match sample_format {
            SampleFormat::F32 => {
                let sample_count_cb = sample_count.clone();
                let overrun_cb = overrun_count.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            push_samples_f32(
                                &mut producer,
                                data,
                                input_channels,
                                &sample_count_cb,
                                &overrun_cb,
                            );
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?
            }
            SampleFormat::I16 => {
                let sample_count_cb = sample_count.clone();
                let overrun_cb = overrun_count.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            push_samples_i16(
                                &mut producer,
                                data,
                                input_channels,
                                &sample_count_cb,
                                &overrun_cb,
                            );
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?
            }
            SampleFormat::U16 => {
                let sample_count_cb = sample_count.clone();
                let overrun_cb = overrun_count.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            push_samples_u16(
                                &mut producer,
                                data,
                                input_channels,
                                &sample_count_cb,
                                &overrun_cb,
                            );
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?
            }
            other => {
                return Err(RecordingError::DeviceNotFound(format!(
                    "unsupported input sample format: {other:?}"
                )))
            }
        };

        stream
            .play()
            .map_err(|e| RecordingError::DeviceDisconnected(e.to_string()))?;
        info!(
            sample_rate = config.sample_rate,
            channels = config.channels,
            format = ?sample_format,
            "Mic capture started"
        );

        Ok(Self {
            stop,
            sample_count,
            overrun_count,
            worker_error,
            _stream: Some(stream),
            _worker: Some(worker),
        })
    }

    pub fn stop(&mut self) -> Result<(), RecordingError> {
        self.stop.store(true, Ordering::SeqCst);
        self._stream = None;
        if let Some(h) = self._worker.take() {
            if h.join().is_err() {
                return Err(RecordingError::Other("mic writer thread panicked".into()));
            }
        }
        self.take_worker_error()
    }

    fn take_worker_error(&self) -> Result<(), RecordingError> {
        let Some(mut guard) = self.worker_error.lock().ok() else {
            return Err(RecordingError::Other(
                "mic writer error lock poisoned".into(),
            ));
        };
        if let Some(err) = guard.take() {
            return Err(err);
        }
        Ok(())
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count.load(Ordering::Relaxed)
    }

    pub fn overrun_count(&self) -> u64 {
        self.overrun_count.load(Ordering::Relaxed)
    }
}

fn select_input_config(
    device: &cpal::Device,
) -> Result<(StreamConfig, SampleFormat), RecordingError> {
    for range in device
        .supported_input_configs()
        .map_err(|e| RecordingError::DeviceNotFound(e.to_string()))?
    {
        match range.sample_format() {
            SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16 => {
                if let Some(cfg) = range.try_with_sample_rate(SAMPLE_RATE) {
                    let format = cfg.sample_format();
                    return Ok((cfg.into(), format));
                }
            }
            _ => {}
        }
    }

    Err(RecordingError::DeviceNotFound(
        "no 48 kHz input configuration with f32/i16/u16 samples".into(),
    ))
}

fn push_samples_f32(
    producer: &mut Producer<f32>,
    data: &[f32],
    channels: usize,
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
) {
    push_mono_frames(producer, data, channels, sample_count, overrun_count, |s| s);
}

fn push_samples_i16(
    producer: &mut Producer<f32>,
    data: &[i16],
    channels: usize,
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
) {
    push_mono_frames(producer, data, channels, sample_count, overrun_count, |s| {
        s as f32 / 32768.0
    });
}

fn push_samples_u16(
    producer: &mut Producer<f32>,
    data: &[u16],
    channels: usize,
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
) {
    push_mono_frames(producer, data, channels, sample_count, overrun_count, |s| {
        (s as f32 / 65535.0) * 2.0 - 1.0
    });
}

fn push_mono_frames<T: Copy>(
    producer: &mut Producer<f32>,
    data: &[T],
    channels: usize,
    sample_count: &AtomicU64,
    overrun_count: &AtomicU64,
    convert: impl Fn(T) -> f32,
) {
    let channels = channels.max(1);
    let mut written = 0_u64;

    for frame in data.chunks_exact(channels) {
        let mut mono = 0.0;
        for &sample in frame {
            mono += convert(sample);
        }
        mono /= channels as f32;

        if let Err(PushError::Full(_)) = producer.push(mono) {
            let n = overrun_count.fetch_add(1, Ordering::Relaxed) + 1;
            if n == 1 {
                warn!("Ring buffer overrun (backpressure): disk writer cannot keep up");
            }
        }
        written += 1;
    }

    sample_count.fetch_add(written, Ordering::Relaxed);
}

fn run_wav_writer(
    mut consumer: rtrb::Consumer<f32>,
    storage: Arc<Mutex<SessionStorage>>,
    stop: Arc<AtomicBool>,
    _sample_count: Arc<AtomicU64>,
    overrun_count: Arc<AtomicU64>,
    worker_error: Arc<Mutex<Option<RecordingError>>>,
) {
    let spec = WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut chunk_index: u32 = 0;
    let mut buffer = Vec::with_capacity(SAMPLES_PER_CHUNK);

    loop {
        match consumer.pop() {
            Ok(s) => buffer.push(s),
            Err(_) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
                continue;
            }
        }

        if buffer.len() >= SAMPLES_PER_CHUNK {
            let total_start = Instant::now();

            let (path, metadata_elapsed) = {
                let Ok(mut guard) = storage.lock() else {
                    *worker_error.lock().unwrap() = Some(RecordingError::Other(
                        "session storage lock poisoned".into(),
                    ));
                    return;
                };
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
            let write_result = write_wav_chunk(&path, &buffer, &spec);
            let wav_elapsed = wav_start.elapsed();
            let total_elapsed = total_start.elapsed();

            if let Err(e) = write_result {
                if e.to_string().contains("No space left")
                    || matches!(e, RecordingError::Io(ref io) if io.kind() == ErrorKind::StorageFull)
                {
                    error!("Disk full during mic chunk write");
                }
                *worker_error.lock().unwrap() = Some(e);
                return;
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
            buffer.clear();
        }
    }

    let overruns = overrun_count.load(Ordering::Relaxed);
    if overruns > 0 {
        warn!(
            "Mic capture had {} sample overruns (ring buffer full)",
            overruns
        );
    }

    if !buffer.is_empty() {
        let Ok(mut guard) = storage.lock() else {
            *worker_error.lock().unwrap() = Some(RecordingError::Other(
                "session storage lock poisoned".into(),
            ));
            return;
        };
        let path = guard.mic_chunk_path(chunk_index);
        guard.metadata.audio.mic_chunks = chunk_index + 1;
        drop(guard);
        if let Err(err) = write_wav_chunk(&path, &buffer, &spec) {
            *worker_error.lock().unwrap() = Some(err);
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_storage(name: &str) -> (Arc<Mutex<SessionStorage>>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ultra-meeting-capture-test-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        let storage = SessionStorage::create(root.clone(), name).unwrap();
        (Arc::new(Mutex::new(storage)), root)
    }

    #[test]
    fn downmixes_multichannel_input_before_buffering() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(8);
        let sample_count = AtomicU64::new(0);
        let overrun_count = AtomicU64::new(0);

        push_samples_f32(
            &mut producer,
            &[1.0, -1.0, 0.25, 0.75],
            2,
            &sample_count,
            &overrun_count,
        );

        assert_eq!(sample_count.load(Ordering::Relaxed), 2);
        assert_eq!(overrun_count.load(Ordering::Relaxed), 0);
        assert_eq!(consumer.pop().unwrap(), 0.0);
        assert_eq!(consumer.pop().unwrap(), 0.5);
        assert!(consumer.pop().is_err());
    }

    #[test]
    fn converts_i16_and_u16_input_to_mono_float_frames() {
        let (mut i16_producer, mut i16_consumer) = RingBuffer::<f32>::new(8);
        let i16_count = AtomicU64::new(0);
        let i16_overruns = AtomicU64::new(0);

        push_samples_i16(
            &mut i16_producer,
            &[32767, -32768, 0, 16384],
            2,
            &i16_count,
            &i16_overruns,
        );

        assert_eq!(i16_count.load(Ordering::Relaxed), 2);
        assert!((i16_consumer.pop().unwrap() + 0.000015259).abs() < 0.0001);
        assert!((i16_consumer.pop().unwrap() - 0.25).abs() < 0.0001);

        let (mut u16_producer, mut u16_consumer) = RingBuffer::<f32>::new(8);
        let u16_count = AtomicU64::new(0);
        let u16_overruns = AtomicU64::new(0);

        push_samples_u16(
            &mut u16_producer,
            &[0, u16::MAX, 32768, 32768],
            2,
            &u16_count,
            &u16_overruns,
        );

        assert_eq!(u16_count.load(Ordering::Relaxed), 2);
        assert!(u16_consumer.pop().unwrap().abs() < 0.0001);
        assert!(u16_consumer.pop().unwrap().abs() < 0.0001);
    }

    #[test]
    fn ignores_incomplete_multichannel_frames() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(8);
        let sample_count = AtomicU64::new(0);
        let overrun_count = AtomicU64::new(0);

        push_samples_f32(
            &mut producer,
            &[0.2, 0.4, 0.6, 99.0],
            3,
            &sample_count,
            &overrun_count,
        );

        assert_eq!(sample_count.load(Ordering::Relaxed), 1);
        assert_eq!(consumer.pop().unwrap(), 0.4);
        assert!(consumer.pop().is_err());
    }

    #[test]
    fn records_overruns_without_panicking_when_ring_is_full() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(2);
        let sample_count = AtomicU64::new(0);
        let overrun_count = AtomicU64::new(0);

        push_samples_f32(
            &mut producer,
            &[0.1, 0.2, 0.3, 0.4, 0.5],
            1,
            &sample_count,
            &overrun_count,
        );

        assert_eq!(sample_count.load(Ordering::Relaxed), 5);
        assert!(overrun_count.load(Ordering::Relaxed) > 0);
        assert!(consumer.pop().is_ok());
        assert!(consumer.pop().is_ok());
    }

    #[test]
    fn mic_writer_flushes_final_partial_chunk_on_stop() {
        let (mut producer, consumer) = RingBuffer::<f32>::new(SAMPLES_PER_CHUNK);
        let (storage, root) = temp_storage("partial");
        let stop = Arc::new(AtomicBool::new(false));
        let sample_count = Arc::new(AtomicU64::new(0));
        let overrun_count = Arc::new(AtomicU64::new(0));
        let worker_error = Arc::new(Mutex::new(None));

        for _ in 0..1234 {
            producer.push(0.2).unwrap();
        }
        stop.store(true, Ordering::Relaxed);

        run_wav_writer(
            consumer,
            storage.clone(),
            stop,
            sample_count,
            overrun_count,
            worker_error.clone(),
        );

        assert!(worker_error.lock().unwrap().is_none());
        let guard = storage.lock().unwrap();
        assert_eq!(guard.metadata.audio.mic_chunks, 1);
        let reader = hound::WavReader::open(guard.mic_chunk_path(0)).unwrap();
        assert_eq!(reader.duration(), 1234);
        drop(guard);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn wav_writer_clamps_out_of_range_samples() {
        let root = std::env::temp_dir().join(format!(
            "ultra-meeting-capture-clamp-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("clamp.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        write_wav_chunk(&path, &[-2.0, -1.0, 0.0, 1.0, 2.0], &spec).unwrap();
        let mut reader = hound::WavReader::open(&path).unwrap();
        let samples = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(samples, vec![-32768, -32767, 0, 32767, 32767]);
        let _ = std::fs::remove_dir_all(root);
    }
}
