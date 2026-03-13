//! ScreenCaptureKit bridge: capture app audio and feed PCM to Rust.
//! Requires Screen Recording permission. Audio is 48kHz mono Float32.

import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

@available(macOS 12.3, *)
final class ScreenCaptureBridge: NSObject, SCStreamDelegate, SCStreamOutput, ScreenCaptureProtocol {
    private var stream: SCStream?
    private let audioQueue = DispatchQueue(label: "com.ultrameeting.audio")
    private var isCapturing = false

    /// Capture app audio from the main display, excluding our own app.
    /// - Parameters:
    ///   - includingApps: Apps to include (empty = all). Pass nil to capture all app audio.
    func startCapture(includingApps: [SCRunningApplication]? = nil) async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )

        guard let display = content.displays.first else {
            throw CaptureError.noDisplay
        }

        let excludedApps = content.applications.filter {
            $0.bundleIdentifier == Bundle.main.bundleIdentifier
        }

        let filter: SCContentFilter
        if let apps = includingApps, !apps.isEmpty {
            filter = SCContentFilter(display: display, including: apps, exceptingWindows: [])
        } else {
            filter = SCContentFilter(
                display: display,
                excludingApplications: excludedApps,
                exceptingWindows: []
            )
        }

        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true
        config.captureMicrophone = false

        stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream?.addStreamOutput(self, type: .audio, sampleHandlerQueue: audioQueue)

        try await stream?.startCapture()
        isCapturing = true
    }

    func stopCapture() async {
        guard let stream else { return }
        try? await stream.stopCapture()
        try? stream.removeStreamOutput(self, type: .audio)
        self.stream = nil
        isCapturing = false
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        isCapturing = false
        NSLog("ScreenCaptureKit stream stopped: %@", error.localizedDescription)
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard isCapturing, type == .audio, CMSampleBufferDataIsReady(sampleBuffer) else { return }
        autoreleasepool {
            sendAudioToRust(sampleBuffer)
        }
    }

    private func sendAudioToRust(_ sampleBuffer: CMSampleBuffer) {
        guard let dataBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }
        guard let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer) else { return }
        guard let basicDescPtr = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc) else { return }
        let basicDesc = basicDescPtr.pointee

        let channelCount = Int(basicDesc.mChannelsPerFrame)
        let sampleRate = basicDesc.mSampleRate
        let frames = CMSampleBufferGetNumSamples(sampleBuffer)

        guard channelCount > 0, frames > 0 else { return }

        var length = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        let status = CMBlockBufferGetDataPointer(dataBuffer, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer)
        guard status == kCMBlockBufferNoErr, let ptr = dataPointer, length > 0 else { return }

        let byteCount = length

        // Fast path: already 48kHz mono Float32 - zero-copy ingest, no Array allocation
        let needsConversion = (basicDesc.mFormatFlags & kAudioFormatFlagIsFloat) == 0
        let needsResample = abs(sampleRate - 48000) >= 1
        let needsMonoDownmix = channelCount > 1

        if !needsConversion && !needsResample && !needsMonoDownmix {
            let f32 = UnsafeRawPointer(ptr).assumingMemoryBound(to: Float32.self)
            let count = byteCount / MemoryLayout<Float32>.size
            if count > 0 {
                _ = RustBridge.ingestRemoteAudioUnsafe(pointer: f32, count: count)
            }
            return
        }

        // Slow path: convert and/or resample
        var floats: [Float]
        if (basicDesc.mFormatFlags & kAudioFormatFlagIsFloat) != 0 {
            let f32 = UnsafeRawPointer(ptr).assumingMemoryBound(to: Float32.self)
            let count = byteCount / MemoryLayout<Float32>.size
            floats = Array(UnsafeBufferPointer(start: f32, count: count))
        } else if (basicDesc.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0 {
            let i16 = UnsafeRawPointer(ptr).assumingMemoryBound(to: Int16.self)
            let count = byteCount / MemoryLayout<Int16>.size
            floats = []
            floats.reserveCapacity(count)
            for idx in 0..<count {
                floats.append(Float(i16[idx]) / 32768.0)
            }
        } else {
            let u16 = UnsafeRawPointer(ptr).assumingMemoryBound(to: UInt16.self)
            let count = byteCount / MemoryLayout<UInt16>.size
            floats = []
            floats.reserveCapacity(count)
            for idx in 0..<count {
                floats.append((Float(u16[idx]) / 65535.0) * 2.0 - 1.0)
            }
        }

        if needsResample || needsMonoDownmix {
            floats = resampleTo48kMono(floats, fromRate: sampleRate, channels: channelCount)
        }

        if !floats.isEmpty {
            _ = RustBridge.ingestRemoteAudio(samples: floats)
        }
    }

    private func resampleTo48kMono(_ samples: [Float], fromRate: Double, channels: Int) -> [Float] {
        var mono: [Float] = samples
        if channels > 1 {
            mono = []
            mono.reserveCapacity(samples.count / channels)
            var i = 0
            while i + channels <= samples.count {
                var sum: Float = 0
                for c in 0..<channels { sum += samples[i + c] }
                mono.append(sum / Float(channels))
                i += channels
            }
        }
        if abs(fromRate - 48000) < 1 { return mono }
        let ratio = fromRate / 48000.0
        let targetCount = Int(Double(mono.count) / ratio)
        var resampled: [Float] = []
        resampled.reserveCapacity(targetCount)
        for t in 0..<targetCount {
            let srcIdx = Double(t) * ratio
            let idx = Int(srcIdx)
            let frac = Float(srcIdx - Double(idx))
            if idx + 1 < mono.count {
                resampled.append(mono[idx] * (1 - frac) + mono[idx + 1] * frac)
            } else {
                resampled.append(mono[min(idx, mono.count - 1)])
            }
        }
        return resampled
    }
}

enum CaptureError: LocalizedError {
    case noDisplay
    case noAudio

    var errorDescription: String? {
        switch self {
        case .noDisplay: return "No display available for capture"
        case .noAudio: return "No audio configuration"
        }
    }
}
