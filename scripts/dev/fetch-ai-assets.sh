#!/usr/bin/env bash
set -euo pipefail

CACHE_DIR="${CAMERA_HUB_AI_CACHE:-/tmp/camera-hub-ai-cache}"
ORT_VERSION="1.23.2"
ORT_ARCHIVE="${CACHE_DIR}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz"
ORT_SHA256="7c63c73560ed76b1fac6cff8204ffe34fe180e70d6582b5332ec094810241e5c"
MODEL="${CACHE_DIR}/yolox_nano.onnx"
MODEL_SHA256="c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d"

mkdir -p "${CACHE_DIR}"

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
    rm -f "${target}" "${temporary}"
    curl -fL --retry 3 --connect-timeout 20 "${url}" -o "${temporary}"
    if [[ "$(digest "${temporary}")" != "${expected}" ]]; then
        rm -f "${temporary}"
        echo "SHA-256 mismatch for ${url}" >&2
        exit 1
    fi
    mv "${temporary}" "${target}"
}

fetch_verified \
    "${ORT_ARCHIVE}" \
    "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz" \
    "${ORT_SHA256}"
fetch_verified \
    "${MODEL}" \
    "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx" \
    "${MODEL_SHA256}"

printf '%s\n' "${ORT_ARCHIVE}" "${MODEL}"
