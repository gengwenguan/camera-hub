#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
HOME="${HOME:-/data/data/com.termux/files/home}"
APP_HOME="${CAMERA_HUB_TERMUX_HOME:-${HOME}/.local/share/camera-hub}"
CONFIG_DIR="${HOME}/.config"
ENV_FILE="${CONFIG_DIR}/camera-hub.env"
DATA_DIR="${APP_HOME}/data"
STATE_DIR="${APP_HOME}/state"
AI_DIR="${APP_HOME}/ai"
CERT_FILE="${STATE_DIR}/cert.pem"
KEY_FILE="${STATE_DIR}/key.pem"
BOOT_DIR="${HOME}/.termux/boot"
PUBLIC_HOST="${CAMERA_HUB_PUBLIC_HOST:-${1:-}}"

missing=()
for command in cargo clang ffmpeg openssl pgrep curl; do
    command -v "${command}" >/dev/null 2>&1 || missing+=("${command}")
done
if (( ${#missing[@]} > 0 )); then
    echo "missing Termux commands: ${missing[*]}" >&2
    echo "install them with:" >&2
    echo "  pkg install rust clang ffmpeg openssl-tool procps curl git pkg-config" >&2
    exit 1
fi

if [[ -n "${CAMERA_HUB_BINARY:-}" ]]; then
    BINARY="${CAMERA_HUB_BINARY}"
else
    cargo build --manifest-path "${ROOT_DIR}/Cargo.toml" --release
    BINARY="${ROOT_DIR}/target/release/camera-hub"
fi
[[ -x "${BINARY}" ]] || {
    echo "camera-hub binary not found: ${BINARY}" >&2
    exit 1
}

install -d \
    "${PREFIX}/bin" "${CONFIG_DIR}" "${DATA_DIR}" "${STATE_DIR}" \
    "${AI_DIR}/models" "${BOOT_DIR}"
install -m 0755 "${BINARY}" "${PREFIX}/bin/camera-hub"
install -m 0755 \
    "${ROOT_DIR}/deploy/termux/start.sh" \
    "${PREFIX}/bin/camera-hub-start"
install -m 0755 \
    "${ROOT_DIR}/deploy/termux/boot.sh" \
    "${BOOT_DIR}/20-camera-hub"

if [[ ! -s "${CERT_FILE}" || ! -s "${KEY_FILE}" ]]; then
    SAN="DNS:camera-hub"
    [[ -z "${PUBLIC_HOST}" ]] || SAN="${SAN},IP:${PUBLIC_HOST}"
    openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 \
        -subj "/CN=camera-hub" \
        -addext "subjectAltName=${SAN}" \
        -keyout "${KEY_FILE}" -out "${CERT_FILE}"
    chmod 0600 "${KEY_FILE}"
    chmod 0644 "${CERT_FILE}"
fi

if [[ ! -f "${ENV_FILE}" ]]; then
    cat > "${ENV_FILE}" <<EOF
CAMERA_HUB_WEB_USERNAME='admin'
CAMERA_HUB_WEB_PASSWORD='12345'
CAMERA_HUB_BIND='[::]:8080'
CAMERA_HUB_TLS_BIND='[::]:8443'
CAMERA_HUB_TLS_CERT='${CERT_FILE}'
CAMERA_HUB_TLS_KEY='${KEY_FILE}'
CAMERA_HUB_DATA_DIR='${DATA_DIR}'
CAMERA_HUB_SETTINGS_FILE='${STATE_DIR}/settings.json'
CAMERA_HUB_QQ_CONFIG_FILE='${STATE_DIR}/qq.json'
CAMERA_HUB_SEGMENT_SECONDS='600'
CAMERA_HUB_MAX_BYTES='4294967296'
CAMERA_HUB_RETAIN_DAYS='7'
CAMERA_HUB_AI_ENABLED='false'
CAMERA_HUB_AI_RUNTIME='${PREFIX}/lib/libonnxruntime.so'
CAMERA_HUB_AI_MODEL='${AI_DIR}/models/yolox_nano.onnx'
CAMERA_HUB_AI_INTERVAL_MS='1000'
CAMERA_HUB_AI_THRESHOLD='0.30'
CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO='0.02'
CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS='10'
CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT='500'
CAMERA_HUB_AI_SNAPSHOT_QUALITY='95'
EOF
    chmod 0600 "${ENV_FILE}"
fi

grep -q '^CAMERA_HUB_WEB_USERNAME=' "${ENV_FILE}" ||
    echo "CAMERA_HUB_WEB_USERNAME='admin'" >> "${ENV_FILE}"
grep -q '^CAMERA_HUB_WEB_PASSWORD=' "${ENV_FILE}" ||
    echo "CAMERA_HUB_WEB_PASSWORD='12345'" >> "${ENV_FILE}"
grep -q '^CAMERA_HUB_QQ_CONFIG_FILE=' "${ENV_FILE}" ||
    echo "CAMERA_HUB_QQ_CONFIG_FILE='${STATE_DIR}/qq.json'" >> "${ENV_FILE}"
sed -i '/^CAMERA_HUB_AI_SNAPSHOT_RETAIN_DAYS=/d' "${ENV_FILE}"
grep -q '^CAMERA_HUB_AI_SNAPSHOT_QUALITY=' "${ENV_FILE}" ||
    echo "CAMERA_HUB_AI_SNAPSHOT_QUALITY='95'" >> "${ENV_FILE}"

if ! ffmpeg -hide_banner -encoders 2>/dev/null | grep -q 'libopus'; then
    echo "warning: this FFmpeg build has no libopus encoder; MSE and recording work," >&2
    echo "but camera-hub WebRTC audio will be unavailable" >&2
fi

pkill -f '^.*/camera-hub$' 2>/dev/null || true
for _ in {1..10}; do
    pgrep -f '^.*/camera-hub$' >/dev/null 2>&1 || break
    sleep 1
done
pkill -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -f '[c]amera-hub-opus' 2>/dev/null || true
sleep 1
pkill -9 -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -9 -f '[c]amera-hub-opus' 2>/dev/null || true
nohup "${PREFIX}/bin/camera-hub-start" > "${HOME}/camera-hub.log" 2>&1 &
sleep 3
curl -g -fsS 'http://[::1]:8080/health'
echo
echo "camera-hub installed for Termux"
echo "HTTP:  http://[phone-ipv6]:8080/"
echo "HTTPS: https://[phone-ipv6]:8443/"
echo "Boot:  install and open Termux:Boot once; 20-camera-hub is ready"
echo "AI:    disabled until an Android/Termux ONNX Runtime is installed"
