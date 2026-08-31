#!/usr/bin/env bash
set -euo pipefail

CACHE_DIR="${CAMERA_HUB_VOICE_CACHE:-/tmp/camera-hub-voice-cache}"
MODEL="sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"
ARCHIVE="${CACHE_DIR}/${MODEL}.tar.bz2"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/${MODEL}.tar.bz2"
RUNTIME="sherpa-onnx-v1.13.6-linux-aarch64-shared-cpu-lib.tar.bz2"
RUNTIME_ARCHIVE="${CACHE_DIR}/${RUNTIME}"
RUNTIME_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.6/${RUNTIME}"

mkdir -p "${CACHE_DIR}"
if [[ ! -s "${ARCHIVE}" ]]; then
    temporary="${ARCHIVE}.tmp"
    rm -f "${temporary}"
    curl -fL --retry 3 --connect-timeout 20 "${URL}" -o "${temporary}"
    mv "${temporary}" "${ARCHIVE}"
fi

tar -tjf "${ARCHIVE}" >/dev/null
if [[ ! -s "${RUNTIME_ARCHIVE}" ]]; then
    temporary="${RUNTIME_ARCHIVE}.tmp"
    rm -f "${temporary}"
    curl -fL --retry 3 --connect-timeout 20 "${RUNTIME_URL}" -o "${temporary}"
    mv "${temporary}" "${RUNTIME_ARCHIVE}"
fi
tar -tjf "${RUNTIME_ARCHIVE}" >/dev/null

printf '%s\n%s\n' "${ARCHIVE}" "${RUNTIME_ARCHIVE}"
