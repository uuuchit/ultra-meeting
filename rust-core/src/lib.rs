//! Ultra Meeting Rust Core
//!
//! State machine, audio orchestration, storage, transcription queue.

pub mod capture;
pub mod error;
pub mod ffi;
pub mod remote_audio;
pub mod session;
pub mod state;
pub mod storage;
#[cfg(feature = "transcription")]
pub mod transcription;

pub use capture::MicCapture;
#[cfg(feature = "transcription")]
pub use transcription::{TranscriptSegment, TranscriptionPipeline};
pub use error::RecordingError;
pub use state::{RecordingState, StateMachine};
pub use storage::{SessionMetadata, SessionStorage};
