# Spike 1: Permission Lifecycle

Minimal Swift app to validate microphone and screen capture permission behavior on macOS 15+.

## Build & Run

1. Open `PermissionSpike.xcodeproj` in Xcode
2. Run on macOS 15+ simulator or device
3. Follow prompts and document behavior

## What to Observe

- Mic: Does grant take effect immediately?
- Screen: Does grant require app restart? (Expected: yes)
- Denial: Does UI guide user to System Preferences?
