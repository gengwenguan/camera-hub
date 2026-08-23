#!/usr/bin/env bash
set -euo pipefail

CACHE_DIR="${CAMERA_HUB_AI_CACHE:-/tmp/camera-hub-ai-cache}"
ORT_VERSION="1.23.2"
ORT_ARCHIVE="${CACHE_DIR}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz"
MODEL="${CACHE_DIR}/yolox_nano.onnx"

mkdir -p "${CACHE_DIR}"

if [[ ! -s "${ORT_ARCHIVE}" ]]; then
    curl -L --fail --retry 2 \
        -o "${ORT_ARCHIVE}.tmp" \
        "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz"
    mv "${ORT_ARCHIVE}.tmp" "${ORT_ARCHIVE}"
fi

if [[ ! -s "${MODEL}" ]]; then
    curl -L --fail --retry 2 \
        -o "${MODEL}.tmp" \
        "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx"
    mv "${MODEL}.tmp" "${MODEL}"
fi

printf '%s\n' "${ORT_ARCHIVE}" "${MODEL}"
