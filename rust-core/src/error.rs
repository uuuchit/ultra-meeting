//! Recording error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RecordingError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device disconnected: {0}")]
    DeviceDisconnected(String),

    #[error("Disk full")]
    DiskFull,

    #[error("Buffer overrun")]
    BufferOverrun,

    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),

    #[error("{0}")]
    Other(String),
}
