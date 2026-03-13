//! C FFI for Swift bridge.
//!
//! Exposes session coordinator: init, create_session, start/stop recording,
//! ingest_remote_audio, state, progress, error, transcription_progress.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uint};
use std::panic;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;

use once_cell::sync::Lazy;

use crate::session::SessionCoordinator;
use crate::state::RecordingState;

static COORD: Lazy<Mutex<Option<Arc<SessionCoordinator>>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the Rust core. Call once at app launch.
/// Returns null on success, or error message (caller must free with ultra_meeting_free_string).
#[no_mangle]
pub extern "C" fn ultra_meeting_init() -> *mut c_char {
    match SessionCoordinator::new() {
        Ok(c) => {
            if let Err(e) = c.init() {
                return CString::new(e.to_string()).unwrap().into_raw();
            }
            *COORD.lock().unwrap() = Some(Arc::new(c));
            ptr::null_mut()
        }
        Err(e) => CString::new(e.to_string()).unwrap().into_raw(),
    }
}

/// Free a string returned by the FFI. Safe to call with null.
#[no_mangle]
pub extern "C" fn ultra_meeting_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)); }
    }
}

/// Create a new session. Must be in Idle. Transitions to Preparing.
/// meeting_name: null-terminated C string.
/// storage_root: null-terminated path, or null for default.
/// Returns null on success.
#[no_mangle]
pub extern "C" fn ultra_meeting_create_session(
    meeting_name: *const c_char,
    storage_root: *const c_char,
) -> *mut c_char {
    let name = match unsafe { meeting_name.as_ref() } {
        Some(p) => match unsafe { CStr::from_ptr(p) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return CString::new("invalid meeting_name").unwrap().into_raw(),
        },
        None => return CString::new("invalid meeting_name").unwrap().into_raw(),
    };
    let root = unsafe { storage_root.as_ref() }
        .map(|p| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
        .map(std::path::PathBuf::from);
    let guard = COORD.lock().unwrap();
    match guard.as_ref() {
        Some(c) => match c.create_session(&name, root) {
            Ok(()) => ptr::null_mut(),
            Err(e) => CString::new(e.to_string()).unwrap().into_raw(),
        },
        None => CString::new("not initialized").unwrap().into_raw(),
    }
}

/// Start recording. Must be in Preparing. Transitions to Recording.
/// mic_device: null for default, or device name.
/// Returns null on success.
#[no_mangle]
pub extern "C" fn ultra_meeting_start_recording(mic_device: *const c_char) -> *mut c_char {
    let device = unsafe { mic_device.as_ref() }
        .map(|p| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned());
    let guard = COORD.lock().unwrap();
    match guard.as_ref() {
        Some(c) => match c.start_recording(device) {
            Ok(()) => ptr::null_mut(),
            Err(e) => CString::new(e.to_string()).unwrap().into_raw(),
        },
        None => CString::new("not initialized").unwrap().into_raw(),
    }
}

/// Stop recording. Must be in Recording.
/// Returns immediately; blocking work (including transcription) runs in a background thread.
/// Poll ultra_meeting_state_name and ultra_meeting_transcription_progress for progress.
/// Returns null on success.
#[no_mangle]
pub extern "C" fn ultra_meeting_stop_recording() -> *mut c_char {
    let coord = {
        let guard = COORD.lock().unwrap();
        guard.clone()
    };
    let Some(c) = coord else {
        return CString::new("not initialized").unwrap().into_raw();
    };
    thread::spawn(move || {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            c.stop_recording()
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                c.set_error(e.to_string());
                c.finish_processing();
            }
            Err(panic_payload) => {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "Transcription panicked".into());
                c.set_error(msg);
                c.finish_processing();
            }
        }
    });
    ptr::null_mut()
}

/// Transcribe an existing session folder in the background. path: null-terminated session folder path.
/// Returns null on success, error message (caller must free) on failure.
#[no_mangle]
pub extern "C" fn ultra_meeting_transcribe_session(path: *const c_char) -> *mut c_char {
    let path_str = match (unsafe { path.as_ref() }).and_then(|p| unsafe { CStr::from_ptr(p) }.to_str().ok()) {
        Some(s) => s.to_string(),
        None => return CString::new("invalid path").unwrap().into_raw(),
    };
    let path_buf = std::path::PathBuf::from(&path_str);

    let guard = COORD.lock().unwrap();
    match guard.as_ref() {
        Some(c) => {
            #[cfg(feature = "transcription")]
            match c.transcribe_session_later(path_buf) {
                Ok(()) => ptr::null_mut(),
                Err(e) => CString::new(e.to_string()).unwrap().into_raw(),
            }
            #[cfg(not(feature = "transcription"))]
            CString::new("transcription not enabled").unwrap().into_raw()
        }
        None => CString::new("not initialized").unwrap().into_raw(),
    }
}

/// Path of last completed session (for Swift to insert into DB). Consumed on read.
/// Caller must free with ultra_meeting_free_string.
#[no_mangle]
pub extern "C" fn ultra_meeting_last_completed_recording_path() -> *mut c_char {
    let guard = COORD.lock().unwrap();
    match guard.as_ref().and_then(|c| c.take_last_completed_recording_path()) {
        Some(p) => match CString::new(p.to_string_lossy().into_owned()) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

/// Path of session currently being transcribed (transcribe-later). None when idle.
/// Caller must free with ultra_meeting_free_string.
#[no_mangle]
pub extern "C" fn ultra_meeting_transcribing_session_path() -> *mut c_char {
    let guard = COORD.lock().unwrap();
    match guard.as_ref().and_then(|c| c.transcribing_session_path()) {
        Some(p) => match CString::new(p.to_string_lossy().into_owned()) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

/// Request that processing (transcription) stop early. Safe to call from any thread.
#[no_mangle]
pub extern "C" fn ultra_meeting_skip_transcription() {
    let guard = COORD.lock().unwrap();
    if let Some(c) = guard.as_ref() {
        c.request_cancel_processing();
    }
}

/// Ingest remote audio PCM samples (f32, mono, 48kHz).
/// samples: pointer to f32 array.
/// len: number of samples.
/// Returns null on success.
#[no_mangle]
pub extern "C" fn ultra_meeting_ingest_remote_audio(samples: *const f32, len: c_uint) -> *mut c_char {
    if samples.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(samples, len as usize) };
    let guard = COORD.lock().unwrap();
    match guard.as_ref() {
        Some(c) => match c.ingest_remote_audio(slice) {
            Ok(()) => ptr::null_mut(),
            Err(e) => CString::new(e.to_string()).unwrap().into_raw(),
        },
        None => ptr::null_mut(),
    }
}

/// Current state name: "idle", "preparing", "recording", "stopping", "processing", "error", "failed".
/// Returns static C string - do not free.
#[no_mangle]
pub extern "C" fn ultra_meeting_state_name() -> *const c_char {
    static IDLE: &[u8] = b"idle\0";
    static PREPARING: &[u8] = b"preparing\0";
    static RECORDING: &[u8] = b"recording\0";
    static STOPPING: &[u8] = b"stopping\0";
    static PROCESSING: &[u8] = b"processing\0";
    static ERROR: &[u8] = b"error\0";
    static FAILED: &[u8] = b"failed\0";

    let guard = COORD.lock().unwrap();
    let name = guard.as_ref().map(|c| match c.state() {
        RecordingState::Idle => IDLE.as_ptr() as *const c_char,
        RecordingState::Preparing => PREPARING.as_ptr() as *const c_char,
        RecordingState::Recording => RECORDING.as_ptr() as *const c_char,
        RecordingState::Stopping => STOPPING.as_ptr() as *const c_char,
        RecordingState::Processing => PROCESSING.as_ptr() as *const c_char,
        RecordingState::Error(_) => ERROR.as_ptr() as *const c_char,
        RecordingState::Failed(_) => FAILED.as_ptr() as *const c_char,
    });
    name.unwrap_or(IDLE.as_ptr() as *const c_char)
}

/// Recording duration in seconds when Recording. Returns 0 if not recording.
#[no_mangle]
pub extern "C" fn ultra_meeting_recording_duration_secs() -> u64 {
    let guard = COORD.lock().unwrap();
    guard
        .as_ref()
        .and_then(|c| c.recording_duration_secs())
        .unwrap_or(0)
}

/// Last error message if any. Caller must free with ultra_meeting_free_string.
#[no_mangle]
pub extern "C" fn ultra_meeting_last_error() -> *mut c_char {
    let guard = COORD.lock().unwrap();
    match guard.as_ref().and_then(|c| c.last_error()) {
        Some(s) => CString::new(s).unwrap().into_raw(),
        None => ptr::null_mut(),
    }
}

/// Transcription progress 0-100 when in Processing. Returns 0 otherwise.
#[no_mangle]
pub extern "C" fn ultra_meeting_transcription_progress() -> c_uint {
    let guard = COORD.lock().unwrap();
    guard
        .as_ref()
        .map(|c| c.transcription_progress())
        .unwrap_or(0)
}

/// Recording metrics as JSON when recording (for instrumentation). Caller must free with ultra_meeting_free_string.
/// Returns null when not recording.
#[no_mangle]
pub extern "C" fn ultra_meeting_recording_metrics_json() -> *mut c_char {
    let guard = COORD.lock().unwrap();
    let Some(c) = guard.as_ref() else {
        return ptr::null_mut();
    };
    let Some((mic_overruns, remote_overruns, remote_samples, duration_secs)) =
        c.recording_metrics_snapshot()
    else {
        return ptr::null_mut();
    };
    let remote_rate = if duration_secs > 0 {
        remote_samples / duration_secs
    } else {
        0
    };
    let json = format!(
        r#"{{"mic_overruns":{},"remote_overruns":{},"remote_samples":{},"duration_secs":{},"remote_samples_per_sec":{}}}"#,
        mic_overruns, remote_overruns, remote_samples, duration_secs, remote_rate
    );
    match CString::new(json) {
        Ok(s) => s.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}
