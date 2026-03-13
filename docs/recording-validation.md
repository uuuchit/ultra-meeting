# Live Recording Performance Validation

Repeatable procedure to validate meeting recording stability and remote speaker continuity.

## Prerequisites

- Built app: `rust-core/target/release/libultra_meeting_core.dylib` and Xcode scheme
- Mic and screen capture permissions granted
- Browser meeting environment (e.g. Google Meet, Zoom in browser, or similar)
- External media source: YouTube tab, shared screen with video, etc.

## Scenario

1. **Setup**
   - Join a browser meeting with one other participant who will speak (remote speaker)
   - Open a second tab with video/audio (e.g. YouTube) as external media
   - Ensure both remote participant audio and system audio will play

2. **Recording**
   - Start UltraMeeting recording
   - Select system audio + screen capture source that includes the meeting
   - Run for **10–15 minutes**
   - Have the remote participant speak periodically; play external media occasionally

## Success Criteria

| Criterion | How to Check |
|-----------|--------------|
| **No laptop shutdown** | Laptop stays on for the full duration |
| **Stable CPU** | Activity Monitor: UltraMeeting CPU remains stable (no runaway spikes) |
| **No ring-buffer overruns** | Console.app: search for `recording_metrics` or `overrun` — count should be 0 |
| **Continuous remote signal** | Post-recording: inspect `remote_*.wav` chunks for non-silent content during participant speech |

## Metrics Collection

During recording, metrics are logged to NSLog every 12 seconds. To capture:

1. Open **Console.app** (macOS)
2. Select your Mac and filter by process `UltraMeeting` or search for `recording_metrics`
3. Or run in Terminal for a live tail:

```bash
log stream --predicate 'processImagePath contains "UltraMeeting"' 2>/dev/null | grep -E "recording_metrics|overrun|chunk"
```

## Post-Recording Checks

After stopping recording:

1. **Session folder** – Check `~/Library/Application Support/UltraMeeting/` (or configured storage) for the session
2. **Remote chunks** – `remote_0.wav`, `remote_1.wav`, … should exist and contain audio during speech
3. **Metadata** – `metadata.yaml` should have correct `mic_chunks` and `remote_chunks`
4. **Console logs** – Final metrics on stop should show:
   - `remote_samples_ingested` > 0
   - `remote_overruns: 0`, `mic_overruns: 0`

## Quick Run Script (optional)

```bash
#!/bin/bash
# recording-validation.sh – Launch app and stream metrics for validation

cd "$(dirname "$0")/.."
log stream --predicate 'processImagePath contains "UltraMeeting"' 2>/dev/null | grep -E "recording_metrics|overrun|chunk|Remote|Mic" &
LOG_PID=$!

open -a UltraMeeting
echo "UltraMeeting launched. Start a 10–15 minute meeting recording."
echo "Metrics will stream above. Press Enter when done."
read -r
kill $LOG_PID 2>/dev/null
echo "Validation session complete."
```
