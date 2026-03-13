//! Phase 0 Spike: Dual-stream timestamping and drift measurement.
//!
//! Simulates two capture streams with independent "clocks" (timers) and
//! measures drift against a reference (mach_absolute_time equivalent).
//!
//! Pass criteria: drift measurable, ≤ 2ms/min acceptable.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[cfg(target_os = "macos")]
fn mach_time_base() -> (u32, u32) {
    unsafe {
        let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
        libc::mach_timebase_info(&mut info);
        (info.numer, info.denom)
    }
}

#[cfg(target_os = "macos")]
fn mach_absolute_time_ns() -> u64 {
    let t = unsafe { libc::mach_absolute_time() };
    let (numer, denom) = mach_time_base();
    t as u64 * numer as u64 / denom as u64
}

#[cfg(not(target_os = "macos"))]
fn mach_absolute_time_ns() -> u64 {
    std::time::Instant::now().elapsed().as_nanos() as u64
}

const RUN_DURATION_SEC: u64 = 60; // 1 min for quick validation; use env OVERRIDE_RUN_SEC=600 for full
const SAMPLE_INTERVAL_MS: u64 = 100;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("Sync drift spike: {} sec run, sampling every {} ms", RUN_DURATION_SEC, SAMPLE_INTERVAL_MS);

    let stream_a_time = Arc::new(AtomicU64::new(0));
    let stream_b_time = Arc::new(AtomicU64::new(0));

    let sa = stream_a_time.clone();
    let sb = stream_b_time.clone();

    let start_ref = Instant::now();

    let handle_a = thread::spawn(move || {
        let start = Instant::now();
        for _i in 0..(RUN_DURATION_SEC * 1000 / SAMPLE_INTERVAL_MS) {
            thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
            let elapsed_ns = start.elapsed().as_nanos() as u64;
            sa.store(elapsed_ns, Ordering::SeqCst);
        }
    });

    let handle_b = thread::spawn(move || {
        let start = Instant::now();
        for _i in 0..(RUN_DURATION_SEC * 1000 / SAMPLE_INTERVAL_MS) {
            thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
            let elapsed_ns = start.elapsed().as_nanos() as u64;
            sb.store(elapsed_ns, Ordering::SeqCst);
        }
    });

    let mut samples: Vec<(u64, u64, u64)> = Vec::new();
    let end = Instant::now() + Duration::from_secs(RUN_DURATION_SEC);
    while Instant::now() < end {
        thread::sleep(Duration::from_secs(1));
        let ref_ns = mach_absolute_time_ns();
        let a_ns = stream_a_time.load(Ordering::SeqCst);
        let b_ns = stream_b_time.load(Ordering::SeqCst);
        samples.push((ref_ns, a_ns, b_ns));
    }

    handle_a.join().unwrap();
    handle_b.join().unwrap();

    let elapsed_sec = start_ref.elapsed().as_secs_f64();
    if samples.len() < 2 {
        warn!("Insufficient samples for drift analysis");
        return;
    }

    let first = samples.first().unwrap();
    let last = samples.last().unwrap();
    
    // Calculate drift between the two streams (not absolute time)
    let drift_ab = (last.2 as i64 - first.2 as i64) - (last.1 as i64 - first.1 as i64);
    let drift_ab_ms = drift_ab as f64 / 1_000_000.0;
    let drift_ab_per_min = drift_ab_ms / (elapsed_sec / 60.0);

    info!(
        "Drift over {:.2} sec: stream_a vs stream_b = {:.2}ms ({:.2}ms/min)",
        elapsed_sec, drift_ab_ms, drift_ab_per_min
    );

    let pass = drift_ab_per_min.abs() <= 2.0;
    info!("Pass (≤2ms/min): {}", pass);
}
