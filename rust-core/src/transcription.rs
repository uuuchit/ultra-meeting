//! Post-meeting transcription: chunk merge, VAD, batched whisper, markdown output.
//! Requires `transcription` feature and cmake to build.

use crate::error::RecordingError;
use crate::storage::{SessionMetadata, TranscriptionInfo};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

const BATCH_DURATION_SEC: usize = 60;
const SAMPLE_RATE_16K: u32 = 16000;
const SAMPLES_PER_BATCH_16K: usize = SAMPLE_RATE_16K as usize * BATCH_DURATION_SEC;

#[derive(Debug)]
pub struct TranscriptSegment {
    pub start_sec: f64,
    pub end_sec: f64,
    pub speaker: String,
    pub text: String,
}

pub struct TranscriptionPipeline {
    model_path: PathBuf,
}

impl TranscriptionPipeline {
    pub fn new() -> Result<Self, RecordingError> {
        let model_dir = dirs::home_dir()
            .ok_or_else(|| RecordingError::Other("no home dir".into()))?
            .join(".ultra-meeting/models");
        std::fs::create_dir_all(&model_dir)?;
        let model_path = model_dir.join("ggml-base.en.bin");
        Ok(Self { model_path })
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Merge chunked WAV files from session root into a single f32 buffer.
    fn merge_chunks(root: &Path, prefix: &str, chunk_count: u32) -> Result<(Vec<f32>, u32), RecordingError> {
        let mut all_samples: Vec<f32> = Vec::new();
        let mut sample_rate: Option<u32> = None;

        for i in 0..chunk_count {
            let path = root.join(format!("{}_{:03}.wav", prefix, i));
            if !path.exists() {
                warn!("{} chunk {} missing, skipping", prefix, i);
                continue;
            }
            let (samples, rate, _) = load_wav(&path)?;
            sample_rate = Some(rate);
            all_samples.extend(samples);
        }

        let rate = sample_rate.unwrap_or(48000);
        Ok((all_samples, rate))
    }

    /// Run session transcription with progress callbacks and checkpointing.
    /// root: session folder (recordings/{id}/) containing mic_*.wav, remote_*.wav.
    /// transcript_path: output path for transcript.md (e.g. transcripts/{id}/transcript.md).
    /// check_cancel: if provided and returns true, stop early (e.g. user clicked Skip).
    pub fn transcribe_session_with_progress(
        &self,
        root: &Path,
        transcript_path: &Path,
        metadata: &SessionMetadata,
        mic_chunks: u32,
        remote_chunks: u32,
        progress_cb: impl Fn(u32, u32),
        check_cancel: impl Fn() -> bool,
    ) -> Result<(), RecordingError> {
        if !self.model_path.exists() {
            return Err(RecordingError::TranscriptionFailed(
                "Whisper model not found. Run model download first.".into(),
            ));
        }

        let (mic_audio_f32, mic_sample_rate) = Self::merge_chunks(root, "mic", mic_chunks)?;
        let (remote_audio_f32, remote_sample_rate) = Self::merge_chunks(root, "remote", remote_chunks)?;
        if mic_audio_f32.is_empty() && remote_audio_f32.is_empty() {
            info!("No audio to transcribe");
            return Ok(());
        }

        let mic_audio_16k = if mic_audio_f32.is_empty() {
            Vec::new()
        } else if mic_sample_rate != SAMPLE_RATE_16K {
            resample_to_16k(&mic_audio_f32, mic_sample_rate)?
        } else {
            mic_audio_f32
        };

        let remote_audio_16k = if remote_audio_f32.is_empty() {
            Vec::new()
        } else if remote_sample_rate != SAMPLE_RATE_16K {
            resample_to_16k(&remote_audio_f32, remote_sample_rate)?
        } else {
            remote_audio_f32
        };

        let mic_batches = if mic_audio_16k.is_empty() {
            0
        } else {
            (mic_audio_16k.len() + SAMPLES_PER_BATCH_16K - 1) / SAMPLES_PER_BATCH_16K
        };
        let remote_batches = if remote_audio_16k.is_empty() {
            0
        } else {
            (remote_audio_16k.len() + SAMPLES_PER_BATCH_16K - 1) / SAMPLES_PER_BATCH_16K
        };
        let total_batches = (mic_batches + remote_batches).max(1);
        let mut all_segments: Vec<TranscriptSegment> = Vec::new();
        if let Some(parent) = transcript_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        progress_cb(0, total_batches as u32);
        if check_cancel() {
            info!("Transcription cancelled by user");
            return Ok(());
        }

        let ctx = whisper_rs::WhisperContext::new_with_params(
            self.model_path.to_str().ok_or_else(|| RecordingError::Other("invalid model path".into()))?,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

        let mut completed_batches = 0u32;
        for (audio_16k, speaker) in [
            (&mic_audio_16k, "You (mic)"),
            (&remote_audio_16k, "Remote"),
        ] {
            if audio_16k.is_empty() {
                continue;
            }

            let batches = audio_16k.len().div_ceil(SAMPLES_PER_BATCH_16K);
            for batch_i in 0..batches {
                if check_cancel() {
                    info!("Transcription cancelled by user");
                    return Ok(());
                }
                let start = batch_i * SAMPLES_PER_BATCH_16K;
                let end = (start + SAMPLES_PER_BATCH_16K).min(audio_16k.len());
                let batch: Vec<f32> = audio_16k[start..end].to_vec();
                completed_batches += 1;

                if batch.iter().all(|&s| s.abs() < 1e-6) {
                    progress_cb(completed_batches, total_batches as u32);
                    continue;
                }

                let mut state = ctx
                    .create_state()
                    .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

                let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
                params.set_translate(false);
                params.set_print_realtime(false);
                params.set_print_progress(false);

                state
                    .full(params, &batch)
                    .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

                let batch_offset_sec = (start as f64) / SAMPLE_RATE_16K as f64;
                let n = state.full_n_segments();
                for i in 0..n {
                    if let Some(seg) = state.get_segment(i) {
                        let text = seg.to_str_lossy().unwrap_or_default().trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        let start_sec = seg.start_timestamp() as f64 / 100.0 + batch_offset_sec;
                        let end_sec = seg.end_timestamp() as f64 / 100.0 + batch_offset_sec;
                        all_segments.push(TranscriptSegment {
                            start_sec,
                            end_sec,
                            speaker: speaker.into(),
                            text,
                        });
                    }
                }

                all_segments.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec));
                let md = Self::render_markdown(metadata, &all_segments);
                if let Err(e) = std::fs::write(transcript_path, &md) {
                    warn!("Checkpoint write failed: {}", e);
                }

                progress_cb(completed_batches, total_batches as u32);
            }
        }

        all_segments.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec));
        let md = Self::render_markdown(metadata, &all_segments);
        std::fs::write(transcript_path, &md)?;

        Ok(())
    }

    fn render_markdown(meta: &SessionMetadata, segments: &[TranscriptSegment]) -> String {
        let mut md = String::new();
        md.push_str("---\n");
        md.push_str("version: 1\n");
        md.push_str(&format!("meeting: {}\n", meta.meeting.name));
        md.push_str(&format!(
            "date: {}\n",
            meta.meeting.start_time.format("%Y-%m-%d")
        ));
        md.push_str(&format!(
            "duration: {:?}\n",
            meta.meeting.duration_seconds.unwrap_or(0)
        ));
        md.push_str("participants:\n");
        md.push_str("  - You (mic)\n");
        md.push_str("  - Remote\n");
        md.push_str("---\n\n");
        md.push_str(&format!("# {}\n\n", meta.meeting.name));
        md.push_str("**Date**: ");
        md.push_str(&meta.meeting.start_time.format("%B %d, %Y").to_string());
        md.push_str("\n\n");
        md.push_str("## Transcript\n\n");

        for seg in segments {
            let ts = format_timestamp(seg.start_sec);
            md.push_str(&format!("**[{}] {}**: {}\n\n", ts, seg.speaker, seg.text));
        }

        md
    }

    /// Legacy single-file transcription (for tests).
    pub fn transcribe_audio_file(
        &self,
        wav_path: &PathBuf,
    ) -> Result<Vec<TranscriptSegment>, RecordingError> {
        if !self.model_path.exists() {
            return Err(RecordingError::TranscriptionFailed(
                "Whisper model not found. Run model download first.".into(),
            ));
        }

        let (audio_f32, sample_rate, _duration) = load_wav(wav_path)?;
        let audio_16k = if sample_rate != SAMPLE_RATE_16K {
            resample_to_16k(&audio_f32, sample_rate)?
        } else {
            audio_f32.clone()
        };

        let ctx = whisper_rs::WhisperContext::new_with_params(
            self.model_path.to_str().ok_or_else(|| RecordingError::Other("invalid model path".into()))?,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

        let mut state = ctx
            .create_state()
            .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_translate(false);
        params.set_print_realtime(false);
        params.set_print_progress(false);

        state
            .full(params, &audio_16k)
            .map_err(|e| RecordingError::TranscriptionFailed(e.to_string()))?;

        let mut segments = Vec::new();
        let n = state.full_n_segments();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                let text = seg.to_str_lossy().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let start = seg.start_timestamp() as f64 / 100.0;
                let end = seg.end_timestamp() as f64 / 100.0;
                segments.push(TranscriptSegment {
                    start_sec: start,
                    end_sec: end,
                    speaker: "Unknown".into(),
                    text,
                });
            }
        }

        Ok(segments)
    }
}

fn format_timestamp(secs: f64) -> String {
    let h = (secs / 3600.0) as u32;
    let m = ((secs % 3600.0) / 60.0) as u32;
    let s = (secs % 60.0) as u32;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn load_wav(path: &Path) -> Result<(Vec<f32>, u32, f64), RecordingError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let duration_sec = reader.len() as f64
        / spec.sample_rate as f64
        / spec.channels as u32 as f64;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let raw: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
            let mono: Vec<i16> = if spec.channels > 1 {
                raw.chunks(spec.channels as usize).map(|c| c[0]).collect()
            } else {
                raw
            };
            mono.iter().map(|&s| s as f32 / 32768.0).collect()
        }
        hound::SampleFormat::Float => {
            let raw: Vec<f32> = reader.samples::<f32>().collect::<Result<_, _>>()?;
            if spec.channels > 1 {
                raw.chunks(spec.channels as usize).map(|c| c[0]).collect()
            } else {
                raw
            }
        }
    };

    Ok((samples, spec.sample_rate, duration_sec))
}

fn resample_to_16k(audio: &[f32], from_rate: u32) -> Result<Vec<f32>, RecordingError> {
    if from_rate == SAMPLE_RATE_16K {
        return Ok(audio.to_vec());
    }
    let ratio = SAMPLE_RATE_16K as f32 / from_rate as f32;
    let new_len = (audio.len() as f32 * ratio) as usize;
    let mut out = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_idx = i as f32 / ratio;
        let lo = src_idx.floor() as usize;
        let hi = (lo + 1).min(audio.len().saturating_sub(1));
        let t = src_idx - lo as f32;
        let v = audio[lo] * (1.0 - t) + audio.get(hi).copied().unwrap_or(0.0) * t;
        out.push(v);
    }
    Ok(out)
}

pub fn default_transcription_info(model_path: &Path) -> TranscriptionInfo {
    TranscriptionInfo {
        model_path: model_path.to_string_lossy().into_owned(),
        model_strategy: "whisper-ggml-base-en".into(),
    }
}
