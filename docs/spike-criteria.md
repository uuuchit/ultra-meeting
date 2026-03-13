# Phase 0: Spike Pass/Fail Criteria

Technical spikes validate core assumptions before main implementation. All four must pass to proceed to Phase 1.

---

## Spike 1: Permission Lifecycle

**Objective**: Validate mic + screen capture permission behavior and restart requirements.

**Scope**:
- Request microphone permission (AVCaptureDevice)
- Request screen capture permission (CGPreflightScreenCaptureAccess / CGRequestScreenCaptureAccess)
- Observe: Does screen capture require app restart after grant?
- Observe: Does mic grant take effect immediately?
- Handle denial gracefully (clear UI instructions)

**Pass Criteria**:
- [ ] Mic permission prompt appears; grant takes effect without restart
- [ ] Screen capture permission prompt appears; grant requires restart (documented)
- [ ] Preflight check accurately reflects both permission states
- [ ] Denial shows actionable System Preferences deep-link

**Fail**: Unpredictable permission state, crash on denial, or undocumented restart requirement.

---

## Spike 2: Audio Stability

**Objective**: Validate 60+ minute capture run and device disconnect handling.

**Scope**:
- Rust + cpal: capture from default input at 48kHz mono
- Run continuously for 60+ minutes
- Simulate or document: device disconnect mid-stream behavior
- Measure: buffer overruns, dropouts, memory growth

**Pass Criteria**:
- [ ] 60-minute capture completes without crash
- [ ] No unbounded memory growth (< 150MB sustained)
- [ ] Device disconnect produces detectable error (not silent failure)
- [ ] Error callback fires; stream can be torn down cleanly

**Fail**: Crash, memory leak, or silent dropout with no error signal.

---

## Spike 3: Sync / Drift

**Objective**: Validate dual-stream timestamping and drift measurement.

**Scope**:
- Two logical streams (simulated or real) with independent clocks
- Use mach_absolute_time() or equivalent as reference
- Record timestamps for both streams over 10+ minute run
- Compute drift rate (ms/minute)

**Pass Criteria**:
- [ ] Timestamps recorded for both streams with sub-millisecond resolution
- [ ] Drift rate measurable and documented (e.g., ms/min)
- [ ] Drift ≤ 2ms/min is acceptable; document actual observed rate

**Fail**: Cannot measure drift, or drift exceeds 5ms/min without mitigation path.

---

## Spike 4: Transcription Throughput

**Objective**: Benchmark whisper base.en on target Intel Mac.

**Scope**:
- whisper-rs + base.en model
- Benchmark: wall-clock time to transcribe N minutes of audio
- Compute real-time factor (RTF = transcribe_time / audio_duration)

**Pass Criteria**:
- [ ] Model loads successfully
- [ ] RTF ≤ 0.6 for base.en on target hardware (document actual)
- [ ] Memory usage during inference documented

**Fail**: RTF > 0.8, or model load fails, or OOM.

---

## Exit Gate

Proceed to Phase 1 only if all four spikes meet pass criteria. Document any variance in `docs/benchmarks.md`.
