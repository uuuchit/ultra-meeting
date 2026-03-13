#!/bin/bash
# Run Phase 0 spikes. All must pass to proceed to Phase 1.
set -e
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=== Phase 0 Spike Runner ==="

echo ""
echo "--- Spike 2: Audio stability ---"
echo "Run manually for 60min: cd $ROOT/02-audio-stability && cargo run --release"
echo "Quick build check:"
(cd "$ROOT/02-audio-stability" && cargo build --release 2>/dev/null) && echo "  Build OK" || echo "  Build failed"

echo ""
echo "--- Spike 3: Sync drift ---"
(cd "$ROOT/03-sync-drift" && cargo run --release 2>/dev/null) && echo "  Sync spike OK" || echo "  Sync spike failed"

echo ""
echo "--- Spike 4: Transcription throughput ---"
mkdir -p "$ROOT/04-transcription/samples"
if [ ! -f "$ROOT/04-transcription/samples/jfk.wav" ]; then
  echo "  Downloading jfk.wav..."
  curl -sL -o "$ROOT/04-transcription/samples/jfk.wav" \
    "https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav" || true
fi
(cd "$ROOT/04-transcription" && cargo run --release 2>/dev/null) && echo "  Transcription spike OK" || echo "  Transcription spike failed (ensure model downloads)"

echo ""
echo "--- Spike 1: Permissions ---"
echo "  Open spikes/01-permissions in Xcode and run manually."
echo ""
echo "See docs/spike-criteria.md for pass/fail criteria."
