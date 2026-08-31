#!/usr/bin/env bash
set -euo pipefail

LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HUB_HOST="${HUB_HOST:-mi6.gwghome.site}"
HUB_USER="${HUB_USER:-android}"
HUB_PASSWORD="${HUB_PASSWORD:-}"
REMOTE_DIR="${REMOTE_DIR:-/home/android/work/camera-hub}"
WEBRTC_LOCAL_DIR="${LOCAL_DIR}/../github/webrtc"
WEBRTC_REMOTE_DIR="${WEBRTC_REMOTE_DIR:-/home/android/work/webrtc}"
WEBRTC_REV="a91689c3dd237ea48a0ce5a827a69d3807420a5c"
VOICE_MODEL="sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"
VOICE_RUNTIME="sherpa-onnx-v1.13.6-linux-aarch64-shared-cpu-lib.tar.bz2"
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
    RSYNC_SSH="sshpass -e ssh -6 -o HostName=${HUB_HOST} -o ServerAliveInterval=20 -o ServerAliveCountMax=30 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
else
    SSH=(ssh "${SSH_OPTIONS[@]}")
    RSYNC_SSH="ssh -6 -o HostName=${HUB_HOST} -o ServerAliveInterval=20 -o ServerAliveCountMax=30 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
fi

remote() {
    "${SSH[@]}" "${HUB_USER}@${HUB_HOST}" "$@"
}

sync_source() {
    remote "mkdir -p '${REMOTE_DIR}'"
    rsync -az --delete \
        --exclude .git \
        --exclude target \
        --exclude .DS_Store \
        --exclude '*.log' \
        -e "${RSYNC_SSH}" \
        "${LOCAL_DIR}/" "${HUB_USER}@camera-hub-node:${REMOTE_DIR}/"
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
        --exclude output.h264 \
        --exclude output.ogg \
        -e "${RSYNC_SSH}" \
        "${WEBRTC_LOCAL_DIR}/" \
        "${HUB_USER}@camera-hub-node:${WEBRTC_REMOTE_DIR}/"
}

sync_ai_assets() {
    local cache_dir="${CAMERA_HUB_AI_CACHE:-/tmp/camera-hub-ai-cache}"
    bash "${LOCAL_DIR}/deploy/linuxdeploy/fetch-ai-assets.sh" >/dev/null
    remote "mkdir -p /home/android/camera-ai/cache \
        /home/android/camera-ai/runtime /home/android/camera-ai/models"
    rsync -az \
        -e "${RSYNC_SSH}" \
        "${cache_dir}/onnxruntime-linux-aarch64-1.23.2.tgz" \
        "${HUB_USER}@camera-hub-node:/home/android/camera-ai/cache/onnxruntime.tgz"
    rsync -az \
        -e "${RSYNC_SSH}" \
        "${cache_dir}/yolox_nano.onnx" \
        "${HUB_USER}@camera-hub-node:/home/android/camera-ai/models/yolox_nano.onnx"
    remote "tar xzf /home/android/camera-ai/cache/onnxruntime.tgz \
        -C /home/android/camera-ai/runtime --strip-components=1"
}

sync_voice_assets() {
    local cache_dir="${CAMERA_HUB_VOICE_CACHE:-/tmp/camera-hub-voice-cache}"
    local archive="${cache_dir}/${VOICE_MODEL}.tar.bz2"
    bash "${LOCAL_DIR}/deploy/linuxdeploy/fetch-voice-assets.sh" >/dev/null
    remote "mkdir -p /home/android/camera-voice/models /home/android/camera-voice/cache"
    rsync -az \
        -e "${RSYNC_SSH}" \
        "${archive}" \
        "${HUB_USER}@camera-hub-node:/home/android/camera-voice/${VOICE_MODEL}.tar.bz2"
    rsync -az \
        -e "${RSYNC_SSH}" \
        "${cache_dir}/${VOICE_RUNTIME}" \
        "${HUB_USER}@camera-hub-node:/home/android/camera-voice/cache/${VOICE_RUNTIME}"
    remote "set -e
        root='/home/android/camera-voice/models/${VOICE_MODEL}'
        if [ ! -s \"\$root/tokens.txt\" ]; then
            rm -rf \"\$root\"
            tar xjf '/home/android/camera-voice/${VOICE_MODEL}.tar.bz2' \
                -C /home/android/camera-voice/models
        fi"
}

build_remote() {
    remote "set -e
        . /home/android/.cargo/env
        cd '${REMOTE_DIR}'
        SHERPA_ONNX_ARCHIVE_DIR='/home/android/camera-voice/cache' \
        cargo build --release --bins \
            --config 'patch.\"https://github.com/gengwenguan/webrtc\".webrtc.path=\"${WEBRTC_REMOTE_DIR}/webrtc\"'
        test -x target/release/camera-hub
        test -x target/release/camera-hub-ddns
        test -x target/release/camera-hub-voice
    "
}

install_remote() {
    # HUB_HOST may be a DNS name. The installer and ACME helper discover the
    # current public IPv6 locally instead of treating the SSH endpoint as an IP SAN.
    remote "sudo -n sh '${REMOTE_DIR}/deploy/linuxdeploy/install.sh' \
        '${REMOTE_DIR}/target/release/camera-hub' '' \
        '${REMOTE_DIR}/target/release/camera-hub-ddns' \
        '${REMOTE_DIR}/target/release/camera-hub-voice'"
}

case "${ACTION}" in
    sync)
        sync_source
        ;;
    build)
        sync_source
        sync_webrtc
        sync_voice_assets
        build_remote
        ;;
    push)
        sync_source
        sync_webrtc
        sync_ai_assets
        sync_voice_assets
        build_remote
        install_remote
        ;;
    ai-assets)
        sync_ai_assets
        ;;
    voice-assets)
        sync_voice_assets
        ;;
    status)
        remote "curl -g -fsS 'http://[::1]/health'; echo
            pgrep -af camera-hub || true
            grep -E '^CAMERA_HUB_DDNS_(ENABLED|DOMAIN|INTERFACE|RECORDS)=' \
                /home/android/.config/camera-hub-ddns.env 2>/dev/null || true"
        ;;
    log)
        remote "tail -n 200 -f /home/android/camera-hub.log"
        ;;
    ddns-dry-run)
        remote "set -a
            . /home/android/.config/camera-hub-ddns.env
            set +a
            /usr/local/bin/camera-hub-ddns --dry-run"
        ;;
    ddns-once)
        remote "set -a
            . /home/android/.config/camera-hub-ddns.env
            set +a
            /usr/local/bin/camera-hub-ddns --once"
        ;;
    ddns-start)
        remote "set -e
            grep -q \"^CAMERA_HUB_DDNS_ENABLED='true'\" \
                /home/android/.config/camera-hub-ddns.env
            if ! pgrep -x camera-hub-ddns >/dev/null; then
                nohup /usr/local/bin/camera-hub-ddns-start \
                    >/home/android/camera-hub-ddns.log 2>&1 </dev/null &
            fi
            sleep 1
            pgrep -af camera-hub-ddns"
        ;;
    ddns-log)
        remote "tail -n 200 -f /home/android/camera-hub-ddns.log"
        ;;
    *)
        echo "usage: $0 [sync|build|push|ai-assets|voice-assets|status|log|ddns-dry-run|ddns-once|ddns-start|ddns-log]" >&2
        exit 2
        ;;
esac
