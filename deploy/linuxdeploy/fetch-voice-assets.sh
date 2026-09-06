#!/usr/bin/env bash
set -euo pipefail

CACHE_DIR="${CAMERA_HUB_VOICE_CACHE:-/tmp/camera-hub-voice-cache}"
MODEL="sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"
ARCHIVE="${CACHE_DIR}/${MODEL}.tar.bz2"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/${MODEL}.tar.bz2"
ARCHIVE_SHA256="b2f7c89690dc8ce4c6ed6afeab7cd800c36ad1421fb6b6302b4a4b194cf7f35f"
RUNTIME="sherpa-onnx-v1.13.6-linux-aarch64-shared-cpu-lib.tar.bz2"
RUNTIME_ARCHIVE="${CACHE_DIR}/${RUNTIME}"
RUNTIME_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.6/${RUNTIME}"
RUNTIME_SHA256="3575bde0543da12fc626c814c14287455f70a22b72caa483c7398d5f20f4cb12"
TTS_MODEL="sherpa-onnx-zipvoice-distill-int8-zh-en-emilia"
TTS_ARCHIVE="${CACHE_DIR}/${TTS_MODEL}.tar.bz2"
TTS_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/${TTS_MODEL}.tar.bz2"
TTS_ARCHIVE_SHA256="77219c8b40f4ee8d73a7f902305ff6c1128ef9b54461c41b4ca6ed890b6c2803"
TTS_VOCODER="${CACHE_DIR}/vocos_24khz.onnx"
TTS_VOCODER_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx"
TTS_VOCODER_SHA256="bcb3b970e384161c4d634f0bb9e999ff1c471b34c9bc0b1049a5014065ed3cc0"

digest() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

fetch_verified() {
    local target="$1"
    local url="$2"
    local expected="$3"
    local temporary="${target}.tmp"
    if [[ -s "${target}" ]] && [[ "$(digest "${target}")" == "${expected}" ]]; then
        return
    fi
    rm -f "${target}"
    rm -f "${temporary}"
    curl -fL --retry 3 --connect-timeout 20 "${url}" -o "${temporary}"
    if [[ "$(digest "${temporary}")" != "${expected}" ]]; then
        rm -f "${temporary}"
        echo "SHA-256 mismatch for ${url}" >&2
        exit 1
    fi
    mv "${temporary}" "${target}"
}

mkdir -p "${CACHE_DIR}"
fetch_verified "${ARCHIVE}" "${URL}" "${ARCHIVE_SHA256}"
fetch_verified "${RUNTIME_ARCHIVE}" "${RUNTIME_URL}" "${RUNTIME_SHA256}"
fetch_verified "${TTS_ARCHIVE}" "${TTS_URL}" "${TTS_ARCHIVE_SHA256}"
fetch_verified "${TTS_VOCODER}" "${TTS_VOCODER_URL}" "${TTS_VOCODER_SHA256}"

tar -tjf "${ARCHIVE}" >/dev/null
tar -tjf "${RUNTIME_ARCHIVE}" >/dev/null
tar -tjf "${TTS_ARCHIVE}" >/dev/null

printf '%s\n%s\n%s\n%s\n' \
    "${ARCHIVE}" "${RUNTIME_ARCHIVE}" "${TTS_ARCHIVE}" "${TTS_VOCODER}"
