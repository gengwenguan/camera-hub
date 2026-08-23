#!/data/data/com.termux/files/usr/bin/sh
set -eu

PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
HOME="${HOME:-/data/data/com.termux/files/home}"
ENV_FILE="${CAMERA_HUB_ENV_FILE:-${HOME}/.config/camera-hub.env}"

if [ ! -r "${ENV_FILE}" ]; then
    echo "camera-hub environment file is missing: ${ENV_FILE}" >&2
    exit 1
fi

set -a
. "${ENV_FILE}"
set +a
exec "${PREFIX}/bin/camera-hub"
