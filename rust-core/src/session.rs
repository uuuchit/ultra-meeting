//! Session coordinator: owns state, storage, capture, remote audio, and transcription lifecycle.

use crate::capture::MicCapture;
use crate::error::RecordingError;
use crate::remote_audio::RemoteAudioWriter;
use crate::state::{RecordingState, StateMachine};
use crate::storage::SessionStorage;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

pub struct SessionCoordinator {
    state_machine: Mutex<StateMachine>,
    session_storage: Mutex<Option<Arc<Mutex<SessionStorage>>>>,
    mic_capture: Mutex<Option<MicCapture>>,
    remote_writer: Mutex<Option<RemoteAudioWriter>>,
    last_error: Mutex<Option<String>>,
    recording_start_secs: Mutex<Option<u64>>,
    transcription_progress: AtomicU32,
    /// Total samples ingested from remote audio (ScreenCaptureKit) for callback rate inference.
    remote_samples_ingested: AtomicU64,
    /// When set, transcription should stop early (user clicked Skip).
    cancel_processing_requested: AtomicBool,
    /// Path of session being transcribed in background (transcribe-later). None when idle.
    transcribing_session_path: Mutex<Option<PathBuf>>,
    /// Recording path of last completed session (for Swift to insert into DB). Consumed on read.
    last_completed_recording_path: Mutex<Option<PathBuf>>,
}

impl SessionCoordinator {
    pub fn new() -> Result<Self, RecordingError> {
        Ok(Self {
            state_machine: Mutex::new(StateMachine::new()?),
            session_storage: Mutex::new(None),
            mic_capture: Mutex::new(None),
            remote_writer: Mutex::new(None),
            last_error: Mutex::new(None),
            recording_start_secs: Mutex::new(None),
            transcription_progress: AtomicU32::new(0),
            remote_samples_ingested: AtomicU64::new(0),
            cancel_processing_requested: AtomicBool::new(false),
            transcribing_session_path: Mutex::new(None),
            last_completed_recording_path: Mutex::new(None),
        })
    }

    pub fn init(&self) -> Result<(), RecordingError> {
        let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
        if !matches!(sm.state(), RecordingState::Idle | RecordingState::Preparing) {
            tracing::warn!("Recovering from interrupted recording (state was {:?})", sm.state());
            sm.reset_interrupted()?;
        }
        Ok(())
    }

    pub fn create_session(&self, meeting_name: &str, storage_root: Option<PathBuf>) -> Result<(), RecordingError> {
        let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
        sm.start_preparing()?;
        drop(sm);

        let base = storage_root
            .or_else(|| dirs::document_dir().map(|d| d.join("UltraMeeting")))
            .ok_or_else(|| RecordingError::Other("no storage root".into()))?;

        let recordings_root = base.join("recordings");
        std::fs::create_dir_all(&recordings_root)?;
        let session_root = SessionStorage::session_folder_path(&recordings_root, meeting_name);
        let storage = SessionStorage::create(session_root, meeting_name)?;
        let storage_arc = Arc::new(Mutex::new(storage));

        *self.session_storage.lock().map_err(|e| RecordingError::Other(e.to_string()))? =
            Some(storage_arc.clone());
        *self.last_error.lock().map_err(|e| RecordingError::Other(e.to_string()))? = None;

        Ok(())
    }

    pub fn start_recording(&self, mic_device: Option<String>) -> Result<(), RecordingError> {
        let storage_arc = self
            .session_storage
            .lock()
            .map_err(|e| RecordingError::Other(e.to_string()))?
            .clone()
            .ok_or_else(|| RecordingError::InvalidState("no session".into()))?;

        {
            let mut guard = storage_arc.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            guard.metadata.capture.mic_device = mic_device.clone().unwrap_or_else(|| "default".into());
        }

        let mic = MicCapture::start(storage_arc.clone(), mic_device)?;

        {
            let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            sm.start_recording()?;
        }

        *self.mic_capture.lock().map_err(|e| RecordingError::Other(e.to_string()))? = Some(mic);
        *self.remote_writer.lock().map_err(|e| RecordingError::Other(e.to_string()))? =
            Some(RemoteAudioWriter::new(storage_arc.clone()));
        *self.recording_start_secs.lock().map_err(|e| RecordingError::Other(e.to_string()))? =
            Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| RecordingError::Other(e.to_string()))?
                .as_secs());

        self.remote_samples_ingested.store(0, Ordering::Relaxed);
        info!("Recording started");
        Ok(())
    }

    pub fn ingest_remote_audio(&self, samples: &[f32]) -> Result<(), RecordingError> {
        self.remote_samples_ingested
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
        let mut writer = self.remote_writer.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
        if let Some(w) = writer.as_mut() {
            w.push_samples(samples)?;
        }
        Ok(())
    }

    pub fn stop_recording(&self) -> Result<(), RecordingError> {
        {
            let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            sm.stop_recording()?;
        }

        let mic_overruns = self
            .mic_capture
            .lock()
            .map_err(|e| RecordingError::Other(e.to_string()))?
            .as_ref()
            .map(|m| m.overrun_count())
            .unwrap_or(0);
        let remote_overruns = self
            .remote_writer
            .lock()
            .map_err(|e| RecordingError::Other(e.to_string()))?
            .as_ref()
            .map(|w| w.overrun_count())
            .unwrap_or(0);
        let remote_samples = self.remote_samples_ingested.load(Ordering::Relaxed);
        let duration_secs = self.recording_duration_secs().unwrap_or(0);
        let remote_rate = if duration_secs > 0 {
            remote_samples / duration_secs
        } else {
            0
        };

        if let Some(mut mic) = self.mic_capture.lock().map_err(|e| RecordingError::Other(e.to_string()))?.take() {
            mic.stop();
        }

        if let Some(mut writer) = self.remote_writer.lock().map_err(|e| RecordingError::Other(e.to_string()))?.take() {
            writer.flush()?;
        }

        info!(
            mic_overruns = mic_overruns,
            remote_overruns = remote_overruns,
            remote_samples = remote_samples,
            duration_secs = duration_secs,
            remote_samples_per_sec = remote_rate,
            "Recording metrics summary"
        );

        if let Some(storage_arc) = self.session_storage.lock().map_err(|e| RecordingError::Other(e.to_string()))?.as_ref() {
            let mut guard = storage_arc.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            let end_time = Utc::now();
            let duration_secs = end_time
                .signed_duration_since(guard.metadata.meeting.start_time)
                .num_seconds()
                .max(0) as u64;
            guard.metadata.meeting.end_time = Some(end_time);
            guard.metadata.meeting.duration_seconds = Some(duration_secs);
            if let Ok(yaml) = serde_yaml::to_string(&guard.metadata) {
                let _ = std::fs::write(guard.root.join("metadata.yaml"), yaml);
            }
        }

        {
            let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            sm.start_processing()?;
        }

        self.cancel_processing_requested.store(false, Ordering::Relaxed);
        *self.recording_start_secs.lock().map_err(|e| RecordingError::Other(e.to_string()))? = None;

        #[cfg(feature = "transcription")]
        {
            let to_run = self
                .session_storage
                .lock()
                .map_err(|e| RecordingError::Other(e.to_string()))?
                .as_ref()
                .map(|arc| {
                    let guard = arc.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
                    let transcript_path = guard.transcript_output_path();
                    Ok::<_, RecordingError>((
                        guard.root.clone(),
                        transcript_path,
                        guard.metadata.clone(),
                        guard.metadata.audio.mic_chunks,
                    ))
                })
                .transpose()?;

            if let Some((root, transcript_path, mut meta, mic_chunks)) = to_run {
                self.transcription_progress.store(0, Ordering::Relaxed);
                if let Ok(pipeline) = crate::transcription::TranscriptionPipeline::new() {
                    let progress = &self.transcription_progress;
                    let cb = |current: u32, total: u32| {
                        let pct = if total > 0 { (current * 100) / total } else { 100 };
                        progress.store(pct, Ordering::Relaxed);
                    };
                    let remote_chunks = meta.audio.remote_chunks;
                    let cancel_flag = &self.cancel_processing_requested;
                    let check_cancel = || cancel_flag.load(Ordering::Relaxed);
                    match pipeline.transcribe_session_with_progress(
                        &root,
                        &transcript_path,
                        &meta,
                        mic_chunks,
                        remote_chunks,
                        cb,
                        check_cancel,
                    ) {
                        Ok(()) => {
                            meta.transcription =
                                Some(crate::transcription::default_transcription_info(pipeline.model_path()));
                            if let Ok(yaml) = serde_yaml::to_string(&meta) {
                                let _ = std::fs::write(root.join("metadata.yaml"), yaml);
                            }
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            self.set_error(msg.clone());
                            tracing::warn!("Transcription failed: {}", msg);
                        }
                    }
                }
                self.transcription_progress.store(100, Ordering::Relaxed);
            }
        }

        if let Some(storage_arc) = self.session_storage.lock().map_err(|e| RecordingError::Other(e.to_string()))?.as_ref() {
            let guard = storage_arc.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            *self.last_completed_recording_path.lock().map_err(|e| RecordingError::Other(e.to_string()))? =
                Some(guard.root.clone());
        }
        *self.session_storage.lock().map_err(|e| RecordingError::Other(e.to_string()))? = None;

        {
            let mut sm = self.state_machine.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            sm.finish()?;
        }

        info!("Recording stopped");
        Ok(())
    }

    pub fn state(&self) -> RecordingState {
        self.state_machine
            .lock()
            .map(|sm| sm.state().clone())
            .unwrap_or(RecordingState::Idle)
    }

    pub fn recording_duration_secs(&self) -> Option<u64> {
        let start = *self.recording_start_secs.lock().ok()?;
        start.map(|s| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(s)
        })
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok()?.clone()
    }

    pub fn set_error(&self, msg: String) {
        *self.last_error.lock().unwrap() = Some(msg.clone());
        let _ = self.state_machine.lock().unwrap().set_error(msg);
    }

    /// Request that processing (transcription) stop early. Checked between batches.
    pub fn request_cancel_processing(&self) {
        self.cancel_processing_requested.store(true, Ordering::Relaxed);
    }

    /// Force transition from Processing/Error to Idle. Call when background thread panics or to recover.
    pub fn finish_processing(&self) {
        *self.session_storage.lock().unwrap() = None;
        if let Ok(mut sm) = self.state_machine.lock() {
            let _ = sm.finish();
        }
        info!("Processing finished (recovery or completion)");
    }

    pub fn transcription_progress(&self) -> u32 {
        self.transcription_progress.load(Ordering::Relaxed)
    }

    /// Path of session currently being transcribed (transcribe-later). None when idle.
    pub fn transcribing_session_path(&self) -> Option<PathBuf> {
        self.transcribing_session_path.lock().ok().and_then(|g| g.clone())
    }

    /// Take and return the path of the last completed session (for Swift to insert into DB).
    pub fn take_last_completed_recording_path(&self) -> Option<PathBuf> {
        self.last_completed_recording_path
            .lock()
            .ok()
            .as_mut()
            .and_then(|g| g.take())
    }

    /// Transcribe an existing session folder in the background. Returns immediately.
    /// Call with Arc receiver: coord.transcribe_session_later(path)
    #[cfg(feature = "transcription")]
    pub fn transcribe_session_later(self: &Arc<Self>, path: PathBuf) -> Result<(), RecordingError> {
        if !path.exists() {
            return Err(RecordingError::Other("session folder does not exist".into()));
        }
        if !path.is_dir() {
            return Err(RecordingError::Other("path is not a directory".into()));
        }
        {
            let guard = self.transcribing_session_path.lock().map_err(|e| RecordingError::Other(e.to_string()))?;
            if guard.is_some() {
                return Err(RecordingError::Other("already transcribing another session".into()));
            }
        }

        let meta = SessionStorage::load_metadata_from_path(&path)?;
        let mic_chunks = meta.audio.mic_chunks;
        let remote_chunks = meta.audio.remote_chunks;

        let coord = self.clone();
        let path_clone = path.clone();
        std::thread::spawn(move || {
            *coord.transcribing_session_path.lock().unwrap() = Some(path_clone.clone());
            coord.transcription_progress.store(0, Ordering::Relaxed);
            coord.cancel_processing_requested.store(false, Ordering::Relaxed);

            let result = (|| -> Result<(), RecordingError> {
                let pipeline = crate::transcription::TranscriptionPipeline::new()?;
                let progress = &coord.transcription_progress;
                let cb = |current: u32, total: u32| {
                    let pct = if total > 0 { (current * 100) / total } else { 100 };
                    progress.store(pct, Ordering::Relaxed);
                };
                let cancel_flag = &coord.cancel_processing_requested;
                let check_cancel = || cancel_flag.load(Ordering::Relaxed);

                let transcript_path = SessionStorage::transcript_output_path_for_session_root(&path_clone);
                pipeline.transcribe_session_with_progress(
                    &path_clone,
                    &transcript_path,
                    &meta,
                    mic_chunks,
                    remote_chunks,
                    cb,
                    check_cancel,
                )?;

                let mut updated_meta = SessionStorage::load_metadata_from_path(&path_clone)?;
                updated_meta.transcription =
                    Some(crate::transcription::default_transcription_info(pipeline.model_path()));
                if let Ok(yaml) = serde_yaml::to_string(&updated_meta) {
                    let _ = std::fs::write(path_clone.join("metadata.yaml"), yaml);
                }
                Ok(())
            })();

            match result {
                Ok(()) => {}
                Err(e) => {
                    coord.set_error(e.to_string());
                }
            }

            *coord.last_completed_recording_path.lock().unwrap() = Some(path_clone);
            coord.transcription_progress.store(100, Ordering::Relaxed);
            *coord.transcribing_session_path.lock().unwrap() = None;
        });

        Ok(())
    }

    /// Snapshot of recording metrics when recording: (mic_overruns, remote_overruns, remote_samples, duration_secs).
    /// Returns None when not recording.
    pub fn recording_metrics_snapshot(&self) -> Option<(u64, u64, u64, u64)> {
        if !matches!(self.state(), RecordingState::Recording) {
            return None;
        }
        let duration = self.recording_duration_secs().unwrap_or(0);
        let mic_overruns = self.mic_capture.lock().ok()?.as_ref()?.overrun_count();
        let remote_overruns = self.remote_writer.lock().ok()?.as_ref()?.overrun_count();
        let remote_samples = self.remote_samples_ingested.load(Ordering::Relaxed);
        Some((mic_overruns, remote_overruns, remote_samples, duration))
    }
}
