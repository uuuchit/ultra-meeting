# Ultra Meeting Benchmarks

Record transcription and capture performance for regression tracking.

## Transcription (Session Pipeline)

**Default build workflow**: `cargo build --release --features transcription` (requires cmake).

Pipeline: merge mic chunks → resample to 16kHz → batched transcription (60s batches) →
progress callbacks → checkpoint after each batch → deterministic markdown output →
metadata.transcription (model_path, model_strategy).

| Date | Model | Hardware | Audio (s) | Transcribe (s) | RTF | Pass |
|------|-------|----------|----------|----------------|-----|------|
| - | base.en | - | - | - | - | - |

Target: RTF ≤ 0.6

## Sync Drift (Phase 0 Spike 3)

| Date | Run (s) | Drift A (ms/min) | Drift B (ms/min) | Pass |
|------|---------|------------------|-----------------|------|
| - | - | - | - | - |

Target: ≤ 2 ms/min

## Audio Stability (Phase 0 Spike 2)

| Date | Duration | Crash | Memory | Pass |
|------|----------|-------|--------|------|
| - | - | - | - | - |

## Mic Capture (Phase 1 Production)

| Metric | Target | Notes |
|--------|--------|------|
| Ring buffer overruns | 0 (warn if >0) | Logged when buffer full |
| Format conversion | f32 native or i16→f32 | Auto-negotiated |
| Disk full | RecordingError::DiskFull | Propagated on write failure |
| Device disconnect | Stream error → stop | err_fn sets stop flag |

