#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HUB_HOST="${HUB_HOST:-mi6.gwghome.site}"
HUB_USER="${HUB_USER:-android}"
HUB_PASSWORD="${HUB_PASSWORD:-}"
REMOTE_DIR="${REMOTE_DIR:-/home/android/work/camera-hub}"
REMOTE_BUILD_DIR="${REMOTE_BUILD_DIR:-/home/android/.cache/camera-hub-build}"
WEBRTC_LOCAL_DIR="${WEBRTC_LOCAL_DIR:-${ROOT_DIR}/../github/webrtc}"
WEBRTC_REMOTE_DIR="${WEBRTC_REMOTE_DIR:-/home/android/work/webrtc}"
WEBRTC_REV="a91689c3dd237ea48a0ce5a827a69d3807420a5c"
ACTION="${1:-push}"

SSH_OPTIONS=(-6 -o ServerAliveInterval=20 -o ServerAliveCountMax=30 \
    -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null)
if [[ -n "${HUB_PASSWORD}" ]]; then
    command -v sshpass >/dev/null || {
        echo "sshpass is required when HUB_PASSWORD is set" >&2
        exit 1
    }
    export SSHPASS="${HUB_PASSWORD}"
    SSH=(sshpass -e ssh "${SSH_OPTIONS[@]}")
    RSYNC_RSH="sshpass -e ssh ${SSH_OPTIONS[*]}"
else
    SSH=(ssh "${SSH_OPTIONS[@]}")
    RSYNC_RSH="ssh ${SSH_OPTIONS[*]}"
fi
REMOTE="${HUB_USER}@${HUB_HOST}"

remote() {
    "${SSH[@]}" "${REMOTE}" "$@"
}

sync_source() {
    remote "mkdir -p '${REMOTE_DIR}'"
    rsync -az --delete \
        --exclude .git \
        --exclude target \
        --exclude .DS_Store \
        --exclude '*.log' \
        --exclude web-src/node_modules \
        -e "${RSYNC_RSH}" \
        "${ROOT_DIR}/" "${REMOTE}:${REMOTE_DIR}/"
}

sync_webrtc() {
    local revision
    revision="$(git -C "${WEBRTC_LOCAL_DIR}" rev-parse HEAD)"
    if [[ "${revision}" != "${WEBRTC_REV}" ]]; then
        echo "webrtc-rs revision mismatch: ${revision}, expected ${WEBRTC_REV}" >&2
        exit 1
    fi
    remote "mkdir -p '${WEBRTC_REMOTE_DIR}'"
    rsync -az --delete \
        --exclude .git \
        --exclude target \
        --exclude .DS_Store \
        --exclude output.h264 \
        --exclude output.ogg \
        -e "${RSYNC_RSH}" \
        "${WEBRTC_LOCAL_DIR}/" "${REMOTE}:${WEBRTC_REMOTE_DIR}/"
}

provision_ai() {
    local cache_dir="${CAMERA_HUB_AI_CACHE:-/tmp/camera-hub-ai-cache}"
    bash "${ROOT_DIR}/scripts/dev/fetch-ai-assets.sh" >/dev/null
    remote "mkdir -p /home/android/camera-ai/cache \
        /home/android/camera-ai/runtime /home/android/camera-ai/models"
    rsync -az -e "${RSYNC_RSH}" \
        "${cache_dir}/onnxruntime-linux-aarch64-1.23.2.tgz" \
        "${REMOTE}:/home/android/camera-ai/cache/onnxruntime.tgz"
    rsync -az -e "${RSYNC_RSH}" \
        "${cache_dir}/yolox_nano.onnx" \
        "${REMOTE}:/home/android/camera-ai/models/yolox_nano.onnx"
    remote "tar xzf /home/android/camera-ai/cache/onnxruntime.tgz \
        -C /home/android/camera-ai/runtime --strip-components=1"
}

build_remote() {
    remote "set -e
        . /home/android/.cargo/env
        mkdir -p '${REMOTE_BUILD_DIR}'
        rsync -a --delete --exclude .git --exclude target \
            '${REMOTE_DIR}/' '${REMOTE_BUILD_DIR}/'
        cd '${REMOTE_BUILD_DIR}'
        CARGO_TARGET_DIR='${REMOTE_DIR}/target' \
        cargo build --release --bin camera-hub --features voice-workers \
            --config 'patch.\"https://github.com/gengwenguan/webrtc\".webrtc.path=\"${WEBRTC_REMOTE_DIR}/webrtc\"'
        test -x '${REMOTE_DIR}/target/release/camera-hub'
    "
}

install_remote() {
    remote "sudo -n sh '${REMOTE_DIR}/deploy/mi6/install.sh' \
        '${REMOTE_DIR}/target/release/camera-hub'"
}

show_status() {
    remote "set -a
        . /home/android/.config/camera-hub.env
        set +a
        curl -g -fsS 'http://[::1]/health'; echo
        curl -fsS \"\${CAMERA_HUB_TTS_URL%/}/health\"; echo
        curl -fsS \"\${CAMERA_HUB_IR_URL%/}/health\"; echo
        ps -eo pid=,ppid=,etime=,args= | grep '[c]amera-hub'
    "
}

show_log() {
    local component="${2:-server}"
    local path
    case "${component}" in
        server) path="/home/android/camera-hub.log" ;;
        tts|voice|ddns|ir) path="/home/android/camera-hub-${component}.log" ;;
        *)
            echo "unknown log component: ${component}" >&2
            exit 2
            ;;
    esac
    remote "tail -n 200 -f '${path}'"
}

case "${ACTION}" in
    sync)
        sync_source
        ;;
    build)
        sync_source
        sync_webrtc
        build_remote
        ;;
    push)
        sync_source
        sync_webrtc
        build_remote
        install_remote
        ;;
    provision-ai)
        provision_ai
        ;;
    status)
        show_status
        ;;
    log)
        show_log "$@"
        ;;
    *)
        echo "usage: $0 [sync|build|push|provision-ai|status|log [server|tts|voice|ddns|ir]]" >&2
        exit 2
        ;;
esac
