# Fault Injection Scenarios

Run these scenarios before feature expansion to validate error recovery.

## Disk Full

1. Fill disk to < 100MB free (or use sparse file)
2. Start recording
3. **Expected**: Recording stops gracefully, user notified, no crash
4. **Check**: Metadata marked incomplete, no corrupted WAV chunks

## Permission Revoked

1. Start recording with mic + screen capture granted
2. Revoke microphone in System Settings
3. **Expected**: Error detected within 10s, state → Error, user notified
4. **Check**: Partial recording saved, transcript marked incomplete

## Capture Source Loss

1. Start recording with external mic
2. Unplug mic during recording
3. **Expected**: Stream error callback fires, stop flag set, worker exits cleanly
4. **Check**: Existing chunks valid, metadata updated

## App Restart Mid-Session

1. Start recording
2. Force quit app (Cmd+Q or kill)
3. Restart app
4. **Expected**: On launch, detect interrupted recording in state.json
5. **Check**: Offer resume or discard; partial artifacts in session folder

## System Sleep

1. Start recording
2. Put Mac to sleep (lid close or menu)
3. Wake
4. **Expected**: Document observed behavior; target: pause on sleep, resume on wake
5. **Check**: No crash, no infinite loop

## Performance Budgets

| Metric | Target | Measurement |
|--------|--------|-------------|
| CPU (recording) | < 5% | Activity Monitor |
| Memory (recording) | < 150 MB | Instruments / memory report |
| Queue backpressure | No overflow | Log when ring buffer > 80% |
| Disk IO | Bounded | Chunk flush every 5s only |
