# Ultra Meeting Architecture

## Overview

Local-first meeting recorder: two-track audio capture (mic + remote) with post-meeting transcription.

## Components

| Component | Responsibility |
|-----------|----------------|
| Menu Bar App (Swift/SwiftUI) | UI, permissions, settings, bridge to Rust |
| Rust Core | State machine, audio orchestration, storage, transcription queue |
| cpal | Microphone capture |
| ScreenCaptureKit (Swift) | App/system audio capture |
| whisper-rs | Local transcription |

## Invariants

1. Two tracks always: mic + remote (or empty remote if capture fails)
2. Chunked WAV (5s) for crash safety
3. Metadata journal updated incrementally
4. State persisted to `~/.ultra-meeting/state.json` for crash recovery

## Data Flow

```
User Start → Preparing → Recording (mic + remote) → Stopping → Processing → Idle
                ↓                    ↓                  ↓            ↓
            Error ──────────────────────────────────────────────────→ Idle
```

## Storage Layout

```
~/Documents/UltraMeeting/recordings/
  YYYY-MM-DD_HH-MM-SS_meeting-name/
    metadata.yaml
    mic_000.wav
    remote_000.wav
    transcript.md (post-processing)
```
