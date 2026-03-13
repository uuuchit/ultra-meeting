//! Swift bridge to Rust core FFI.
//! Links against libultra_meeting_core.dylib.

import Foundation

enum RustBridge {
    // MARK: - C declarations (must match rust-core/src/ffi.rs)

    @_silgen_name("ultra_meeting_init")
    private static func _init() -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_free_string")
    private static func _freeString(_ s: UnsafeMutablePointer<CChar>?)

    @_silgen_name("ultra_meeting_create_session")
    private static func _createSession(_ meetingName: UnsafePointer<CChar>?, _ storageRoot: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_start_recording")
    private static func _startRecording(_ micDevice: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_stop_recording")
    private static func _stopRecording() -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_ingest_remote_audio")
    private static func _ingestRemoteAudio(_ samples: UnsafePointer<Float>?, _ len: UInt32) -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_state_name")
    private static func _stateName() -> UnsafePointer<CChar>?

    @_silgen_name("ultra_meeting_recording_duration_secs")
    private static func _recordingDurationSecs() -> UInt64

    @_silgen_name("ultra_meeting_last_error")
    private static func _lastError() -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_transcription_progress")
    private static func _transcriptionProgress() -> UInt32

    @_silgen_name("ultra_meeting_skip_transcription")
    private static func _skipTranscription()

    @_silgen_name("ultra_meeting_transcribe_session")
    private static func _transcribeSession(_ path: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_last_completed_recording_path")
    private static func _lastCompletedRecordingPath() -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_transcribing_session_path")
    private static func _transcribingSessionPath() -> UnsafeMutablePointer<CChar>?

    @_silgen_name("ultra_meeting_recording_metrics_json")
    private static func _recordingMetricsJSON() -> UnsafeMutablePointer<CChar>?

    // MARK: - Swift API

    private static func consumeError(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
        guard let ptr, ptr.pointee != 0 else { return nil }
        let str = String(cString: ptr)
        _freeString(ptr)
        return str
    }

    static func initCore() -> String? {
        let err = _init()
        return consumeError(err)
    }

    static func createSession(meetingName: String, storageRoot: String? = nil) -> String? {
        meetingName.withCString { namePtr in
            if let root = storageRoot {
                return root.withCString { rootPtr in
                    consumeError(_createSession(namePtr, rootPtr))
                }
            }
            return consumeError(_createSession(namePtr, nil))
        }
    }

    static func startRecording(micDevice: String? = nil) -> String? {
        if let device = micDevice {
            return device.withCString { ptr in
                consumeError(_startRecording(ptr))
            }
        }
        return consumeError(_startRecording(nil))
    }

    static func stopRecording() -> String? {
        consumeError(_stopRecording())
    }

    static func ingestRemoteAudio(samples: [Float]) -> String? {
        guard !samples.isEmpty else { return nil }
        return samples.withUnsafeBufferPointer { buf in
            consumeError(_ingestRemoteAudio(buf.baseAddress, UInt32(buf.count)))
        }
    }

    /// Zero-copy ingest when caller has unsafe pointer (avoids Array allocation in hot path).
    static func ingestRemoteAudioUnsafe(pointer: UnsafePointer<Float>, count: Int) -> String? {
        guard count > 0 else { return nil }
        return consumeError(_ingestRemoteAudio(pointer, UInt32(count)))
    }

    static func stateName() -> String {
        guard let ptr = _stateName() else { return "idle" }
        return String(cString: ptr)
    }

    static func recordingDurationSecs() -> UInt64 {
        _recordingDurationSecs()
    }

    static func lastError() -> String? {
        guard let ptr = _lastError() else { return nil }
        let str = String(cString: ptr)
        _freeString(ptr)
        return str
    }

    static func transcriptionProgress() -> UInt32 {
        _transcriptionProgress()
    }

    /// Request that transcription stop early (user clicked Skip).
    static func skipTranscription() {
        _skipTranscription()
    }

    /// Transcribe an existing session folder in the background. Returns error message if sync validation fails.
    static func transcribeSession(path: String) -> String? {
        path.withCString { ptr in
            consumeError(_transcribeSession(ptr))
        }
    }

    /// Path of last completed session (recordings/{id}/). Nil if none or already consumed.
    static func lastCompletedRecordingPath() -> String? {
        guard let ptr = _lastCompletedRecordingPath(), ptr.pointee != 0 else { return nil }
        let str = String(cString: ptr)
        _freeString(ptr)
        return str
    }

    /// Path of session currently being transcribed (transcribe-later). Nil when idle.
    static func transcribingSessionPath() -> String? {
        guard let ptr = _transcribingSessionPath(), ptr.pointee != 0 else { return nil }
        let str = String(cString: ptr)
        _freeString(ptr)
        return str
    }

    /// Recording metrics as JSON when recording (for instrumentation). Check Console.app when RUST_LOG=info.
    static func recordingMetricsJSON() -> String? {
        guard let ptr = _recordingMetricsJSON(), ptr.pointee != 0 else { return nil }
        let str = String(cString: ptr)
        _freeString(ptr)
        return str
    }
}
