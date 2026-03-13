//! Remote audio buffer: receives PCM from Swift/ScreenCaptureKit, writes chunked WAV.

use crate::error::RecordingError;
use crate::storage::SessionStorage;
use hound::{WavSpec, WavWriter};
use rtrb::{PushError, RingBuffer};
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
const RING_CAPACITY: usize = SAMPLES_PER_CHUNK * 8;

pub struct RemoteAudioWriter {
    stop: Arc<AtomicBool>,
    overrun_count: Arc<AtomicU64>,
    worker_error: Arc<Mutex<Option<RecordingError>>>,
    producer: rtrb::Producer<f32>,
    worker: Option<thread::JoinHandle<()>>,
}

impl RemoteAudioWriter {
    pub fn new(storage: Arc<Mutex<SessionStorage>>) -> Self {
        let (producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let overrun_count = Arc::new(AtomicU64::new(0));
        let worker_error = Arc::new(Mutex::new(None));

        let stop_worker = stop.clone();
        let worker_error_worker = worker_error.clone();
        let worker = thread::spawn(move || {
            run_remote_wav_writer(consumer, storage, stop_worker, worker_error_worker);
        });

        Self {
            stop,
            overrun_count,
            worker_error,
            producer,
            worker: Some(worker),
        }
    }

    pub fn push_samples(&mut self, samples: &[f32]) -> Result<(), RecordingError> {
        self.take_worker_error()?;

        for &s in samples {
            match self.producer.push(s) {
                Ok(()) => {}
                Err(PushError::Full(_)) => {
                    let n = self.overrun_count.fetch_add(1, Ordering::Relaxed) + 1;
                    if n == 1 {
                        warn!("Remote audio ring buffer overrun: dropping samples to protect the capture pipeline");
                    }
                }
            }
        }

        self.take_worker_error()
    }

    pub fn overrun_count(&self) -> u64 {
        self.overrun_count.load(Ordering::Relaxed)
    }

    pub fn flush(&mut self) -> Result<(), RecordingError> {
        self.stop.store(true, Ordering::SeqCst);

        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                return Err(RecordingError::Other("remote audio worker thread panicked".into()));
            }
        }

        let overruns = self.overrun_count.load(Ordering::Relaxed);
        if overruns > 0 {
            warn!("Remote audio capture dropped {} samples due to backpressure", overruns);
        }

        self.take_worker_error()
    }

    fn take_worker_error(&self) -> Result<(), RecordingError> {
        if let Some(err) = self
            .worker_error
            .lock()
            .map_err(|e| RecordingError::Other(e.to_string()))?
            .take()
        {
            return Err(err);
        }
        Ok(())
    }
}

fn run_remote_wav_writer(
    mut consumer: rtrb::Consumer<f32>,
    storage: Arc<Mutex<SessionStorage>>,
    stop: Arc<AtomicBool>,
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
    let mut chunk_buf = Vec::with_capacity(SAMPLES_PER_CHUNK);

    loop {
        match consumer.pop() {
            Ok(sample) => buffer.push(sample),
            Err(_) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
                continue;
            }
        }

        while buffer.len() >= SAMPLES_PER_CHUNK {
            chunk_buf.clear();
            chunk_buf.extend(buffer.drain(..SAMPLES_PER_CHUNK));
            let total_start = Instant::now();
            if let Err(err) = write_remote_chunk(&storage, chunk_index, &chunk_buf, &spec, false) {
                error!("Remote audio chunk write failed: {}", err);
                *worker_error.lock().unwrap() = Some(err);
                return;
            }
            let total_elapsed = total_start.elapsed();
            if chunk_index % 12 == 0 {
                info!(
                    chunk_index = chunk_index,
                    total_ms = total_elapsed.as_millis(),
                    "Remote chunk flush metrics"
                );
            } else {
                debug!(
                    chunk_index = chunk_index,
                    total_ms = total_elapsed.as_millis(),
                    "Remote chunk flush"
                );
            }
            chunk_index += 1;
        }
    }

    if !buffer.is_empty() {
        if let Err(err) = write_remote_chunk(&storage, chunk_index, &buffer, &spec, true) {
            error!("Remote audio final flush failed: {}", err);
            *worker_error.lock().unwrap() = Some(err);
        }
    }
}

fn write_remote_chunk(
    storage: &Arc<Mutex<SessionStorage>>,
    chunk_index: u32,
    samples: &[f32],
    spec: &WavSpec,
    force_metadata_write: bool,
) -> Result<(), RecordingError> {
    let path = {
        let guard = storage.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
        guard.remote_chunk_path(chunk_index)
    };

    let wav_start = Instant::now();
    write_wav_chunk(&path, samples, spec)?;
    let wav_elapsed = wav_start.elapsed();

    let meta_start = Instant::now();
    {
        let mut guard = storage.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
        guard.metadata.audio.remote_chunks = chunk_index + 1;
        // Write metadata at most every 60 seconds (chunks 0,12,24,...) to reduce disk churn; always write on final chunk
        if force_metadata_write || chunk_index % 12 == 0 {
            if let Ok(yaml) = serde_yaml::to_string(&guard.metadata) {
                let _ = std::fs::write(guard.root.join("metadata.yaml"), yaml);
            }
        }
    }
    let meta_elapsed = meta_start.elapsed();

    if chunk_index % 12 == 0 {
        info!(
            chunk_index = chunk_index,
            wav_ms = wav_elapsed.as_millis(),
            metadata_ms = meta_elapsed.as_millis(),
            "Remote chunk write breakdown"
        );
    }

    Ok(())
}

fn write_wav_chunk(
    path: &std::path::Path,
    samples: &[f32],
    spec: &WavSpec,
) -> Result<(), RecordingError> {
    let mut w = WavWriter::create(path, *spec).map_err(map_hound_error)?;
    for &s in samples {
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
