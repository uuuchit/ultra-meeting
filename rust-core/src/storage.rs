//! Session storage: folder layout, metadata, chunked WAV.

use crate::error::RecordingError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub version: u32,
    pub format: String,
    pub meeting: MeetingInfo,
    pub audio: AudioInfo,
    pub sync: SyncInfo,
    pub capture: CaptureInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingInfo {
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u32,
    pub format: String,
    pub mic_chunks: u32,
    pub remote_chunks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub clock_base: String,
    pub start_timestamp: u64,
    pub drift_corrections: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureInfo {
    pub mic_device: String,
    pub remote_source: String,
    pub remote_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionInfo {
    pub model_path: String,
    pub model_strategy: String,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            format: "ultra-meeting-v1".into(),
            meeting: MeetingInfo {
                name: "Untitled".into(),
                start_time: Utc::now(),
                end_time: None,
                duration_seconds: None,
            },
            audio: AudioInfo {
                sample_rate: 48000,
                channels: 1,
                format: "16-bit PCM".into(),
                mic_chunks: 0,
                remote_chunks: 0,
            },
            sync: SyncInfo {
                clock_base: "mach_absolute_time".into(),
                start_timestamp: 0,
                drift_corrections: vec![],
            },
            capture: CaptureInfo {
                mic_device: "default".into(),
                remote_source: "".into(),
                remote_method: "ScreenCaptureKit".into(),
            },
            transcription: None,
        }
    }
}

pub struct SessionStorage {
    pub root: PathBuf,
    pub metadata: SessionMetadata,
}

/// Sanitize meeting name for use in folder path.
pub fn sanitize_meeting_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .trim_matches('-')
        .to_string()
        .chars()
        .take(80)
        .collect::<String>()
}

impl SessionStorage {
    /// Create session folder path under recordings: {recordings_root}/{timestamp}_{uuid}
    pub fn session_folder_path(recordings_root: &std::path::Path, _meeting_name: &str) -> PathBuf {
        let now = chrono::Utc::now();
        let ts = now.format("%Y-%m-%d_%H-%M-%S").to_string();
        let id = uuid::Uuid::new_v4().to_string().replace('-', "").chars().take(12).collect::<String>();
        recordings_root.join(format!("{}_{}", ts, id))
    }

    /// Path for transcript output (transcripts/{session_id}/transcript.md).
    pub fn transcript_output_path(&self) -> PathBuf {
        Self::transcript_output_path_for_session_root(&self.root)
    }

    /// Derive transcript path from session root. For recordings/{id}/ layout,
    /// returns transcripts/{id}/transcript.md. Falls back to root/transcript.md for legacy.
    pub fn transcript_output_path_for_session_root(session_root: &std::path::Path) -> PathBuf {
        let storage_base = session_root.parent().and_then(|p| p.parent());
        let session_id = session_root.file_name().map(|s| PathBuf::from(s));
        match (storage_base, session_id) {
            (Some(base), Some(id)) => base.join("transcripts").join(id).join("transcript.md"),
            _ => session_root.join("transcript.md"),
        }
    }

    pub fn create(root: PathBuf, meeting_name: &str) -> Result<Self, RecordingError> {
        std::fs::create_dir_all(&root)?;
        let mut meta = SessionMetadata::default();
        meta.meeting.name = meeting_name.to_string();
        meta.meeting.start_time = Utc::now();
        meta.sync.start_timestamp = mach_time_ns();
        let path = root.join("metadata.yaml");
        let yaml = serde_yaml::to_string(&meta)?;
        std::fs::write(path, yaml)?;
        Ok(Self {
            root: root.clone(),
            metadata: meta,
        })
    }

    pub fn update_metadata(&self, meta: &SessionMetadata) -> Result<(), RecordingError> {
        let path = self.root.join("metadata.yaml");
        let yaml = serde_yaml::to_string(meta)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    pub fn mic_chunk_path(&self, index: u32) -> PathBuf {
        self.root.join(format!("mic_{:03}.wav", index))
    }

    pub fn remote_chunk_path(&self, index: u32) -> PathBuf {
        self.root.join(format!("remote_{:03}.wav", index))
    }

    pub fn transcript_path(&self) -> PathBuf {
        self.root.join("transcript.md")
    }

    /// Load metadata from an existing session folder (for transcribe-later).
    pub fn load_metadata_from_path(root: &std::path::Path) -> Result<SessionMetadata, RecordingError> {
        let path = root.join("metadata.yaml");
        let yaml = std::fs::read_to_string(&path)?;
        serde_yaml::from_str(&yaml).map_err(|e| RecordingError::Other(e.to_string()))
    }
}

#[cfg(target_os = "macos")]
fn mach_time_ns() -> u64 {
    let t = unsafe { libc::mach_absolute_time() };
    let mut info = libc::mach_timebase_info_data_t { numer: 0, denom: 0 };
    unsafe { libc::mach_timebase_info(&mut info) };
    t as u64 * info.numer as u64 / info.denom as u64
}

#[cfg(not(target_os = "macos"))]
fn mach_time_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
}
