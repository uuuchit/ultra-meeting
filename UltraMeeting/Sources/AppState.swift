import Foundation
import SwiftUI

final class AppState: ObservableObject {
    @Published var recordingState: String = "idle"
    @Published var recordingAcknowledged: Bool = false
    @Published var micGranted: Bool = false
    @Published var screenGranted: Bool = false
    @Published var errorMessage: String?
    @Published var recordingDuration: TimeInterval = 0
    @Published var transcriptionProgress: UInt32 = 0

    private var stateTimer: Timer?
    private var screenCapture: (any ScreenCaptureProtocol)?
    private var metricsLogCounter: Int = 0

    var menuBarTitle: String {
        switch recordingState {
        case "recording": return "Recording"
        case "processing": return "Processing"
        case "preparing": return "Preparing"
        case "error": return "Error"
        default: return "Ultra Meeting"
        }
    }

    var menuBarIcon: String {
        switch recordingState {
        case "recording": return "record.circle.fill"
        case "processing": return "waveform"
        case "preparing": return "arrow.clockwise"
        case "error": return "exclamationmark.triangle.fill"
        default: return "waveform.circle"
        }
    }

    func checkPermissions() {
        micGranted = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        screenGranted = CGPreflightScreenCaptureAccess()
    }

    func requestMicPermission() {
        AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
            DispatchQueue.main.async {
                self?.micGranted = granted
                self?.checkPermissions()
            }
        }
    }

    func requestScreenPermission() {
        CGRequestScreenCaptureAccess()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.checkPermissions()
        }
    }

    func startRecording() {
        guard recordingAcknowledged else {
            errorMessage = "Please acknowledge the recording disclosure first."
            return
        }
        guard micGranted && screenGranted else {
            errorMessage = "Grant microphone and screen capture permission first."
            return
        }
        errorMessage = nil
        AuditTrail.log(event: "START", meetingId: Foundation.UUID().uuidString)

        let storageRoot = AppSettings.storageRootURL?.path
        if let err = RustBridge.createSession(meetingName: AppSettings.defaultMeetingName, storageRoot: storageRoot) {
            errorMessage = err
            return
        }
        recordingState = "preparing"

        if let err = RustBridge.startRecording(micDevice: nil) {
            errorMessage = err
            recordingState = "idle"
            return
        }
        recordingState = RustBridge.stateName()
        startStateTimer()

        Task { @MainActor in
            await startScreenCapture()
        }
    }

    func stopRecording() {
        AuditTrail.log(event: "STOP", meetingId: "current")
        recordingState = "stopping"

        Task { @MainActor in
            await stopScreenCapture()
            if let err = RustBridge.stopRecording() {
                errorMessage = err
            }
            syncStateFromRust()
        }
        // Keep timer running to poll transcription progress during Processing
        startStateTimer()
    }

    private func startStateTimer() {
        stopStateTimer()
        stateTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            DispatchQueue.main.async { self?.syncStateFromRust() }
        }
        RunLoop.main.add(stateTimer!, forMode: .common)
    }

    private func stopStateTimer() {
        stateTimer?.invalidate()
        stateTimer = nil
    }

    func syncStateFromRust() {
        recordingState = RustBridge.stateName()
        recordingDuration = TimeInterval(RustBridge.recordingDurationSecs())
        transcriptionProgress = RustBridge.transcriptionProgress()
        if let err = RustBridge.lastError(), !err.isEmpty {
            errorMessage = err
        }
        // Consume last completed session (from stop_recording or transcribe-later) and insert into DB
        if let path = RustBridge.lastCompletedRecordingPath(),
           let meeting = SessionMetadataParser.parse(recordingPath: path) {
            MeetingStore.shared.insertMeeting(meeting)
            if let content = SessionMetadataParser.readTranscript(recordingPath: path),
               let tp = SessionMetadataParser.transcriptPath(fromRecordingPath: path) {
                MeetingStore.shared.insertTranscript(meetingId: meeting.id, contentMd: content, transcriptPath: tp)
                if AppSettings.qmdSearchEnabled && QMDService.shared.isQMDInstalled() {
                    DispatchQueue.global(qos: .utility).async {
                        _ = QMDService.shared.updateIndex()
                    }
                }
            }
        }
        // Detect permission revocation during recording
        if recordingState == "recording" {
            checkPermissions()
            if !micGranted {
                errorMessage = "Microphone access was revoked. Stop and re-grant permission."
            }
            // Log recording metrics every 12 seconds for performance diagnosis
            metricsLogCounter += 1
            if metricsLogCounter >= 12, let metrics = RustBridge.recordingMetricsJSON() {
                NSLog("[UltraMeeting] Recording metrics: %@", metrics)
                metricsLogCounter = 0
            }
        } else {
            metricsLogCounter = 0
        }
    }

    @MainActor
    private func startScreenCapture() async {
        guard #available(macOS 12.3, *) else { return }
        let bridge = ScreenCaptureBridge()
        screenCapture = bridge
        do {
            try await bridge.startCapture(includingApps: nil)
        } catch {
            errorMessage = "Screen capture failed: \(error.localizedDescription). Grant Screen Recording permission and restart the app."
            NSLog("ScreenCaptureKit start error: %@", error.localizedDescription)
        }
    }

    @MainActor
    private func stopScreenCapture() async {
        guard #available(macOS 12.3, *) else { return }
        if let bridge = screenCapture as? ScreenCaptureBridge {
            await bridge.stopCapture()
        }
        screenCapture = nil
    }
}

@available(macOS 12.3, *)
protocol ScreenCaptureProtocol: AnyObject {
    func stopCapture() async
}


import AVFoundation
import CoreGraphics
