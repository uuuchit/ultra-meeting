#!/bin/bash
# Download the default multilingual Whisper model used for post-recording transcription.

set -euo pipefail

MODEL_DIR="${HOME}/.ultra-meeting/models"
MODEL_FILE="ggml-base.bin"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_FILE}"
DEST="${MODEL_DIR}/${MODEL_FILE}"
TMP="${DEST}.download"

mkdir -p "${MODEL_DIR}"

if [[ -s "${DEST}" ]]; then
  echo "Model already exists: ${DEST}"
  exit 0
fi

echo "Downloading multilingual Whisper base model..."
echo "Target: ${DEST}"
curl -L --fail --progress-bar "${MODEL_URL}" -o "${TMP}"
mv "${TMP}" "${DEST}"
echo "Done: ${DEST}"
