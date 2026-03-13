#!/bin/bash
# Live recording performance validation – launch app and stream metrics for a repeatable 10–15 min test.
# See docs/recording-validation.md for full procedure.

set -e
cd "$(dirname "$0")/.."

echo "Recording validation helper"
echo "1. This script will stream UltraMeeting metrics to this terminal"
echo "2. Launch UltraMeeting and start a 10–15 minute meeting recording"
echo "3. Use Console.app or this stream to verify: no overruns, stable CPU"
echo ""

# Build if needed
if [[ ! -f rust-core/target/release/libultra_meeting_core.dylib ]]; then
    echo "Building Rust core..."
    (cd rust-core && CARGO_TARGET_DIR=target cargo build --release --features transcription)
fi

# Build Xcode project
echo "Building UltraMeeting..."
(cd UltraMeeting && CARGO_TARGET_DIR=../rust-core/target xcodebuild -project UltraMeeting.xcodeproj -scheme UltraMeeting -configuration Debug build 2>/dev/null | tail -5)

LOG_PID=""
cleanup() {
    [[ -n "$LOG_PID" ]] && kill "$LOG_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo ""
echo "Starting metrics stream (filter: UltraMeeting, recording_metrics, overrun, chunk)..."
echo "Press Ctrl+C when the meeting is done."
echo ""

log stream --predicate 'processImagePath contains "UltraMeeting"' 2>/dev/null | grep -E "recording_metrics|overrun|chunk|Remote|Mic" &
LOG_PID=$!

# Give user a moment then open app (try build dir first, then Applications)
sleep 2
APP=$(find UltraMeeting/build -name "UltraMeeting.app" -type d 2>/dev/null | head -1)
if [[ -n "$APP" ]]; then
    open "$APP"
elif open -a UltraMeeting 2>/dev/null; then
    :
else
    echo "Launch UltraMeeting manually (from Xcode or build output)."
fi

wait "$LOG_PID" 2>/dev/null || true
