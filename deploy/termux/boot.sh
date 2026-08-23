#!/data/data/com.termux/files/usr/bin/sh
set -eu

PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
HOME="${HOME:-/data/data/com.termux/files/home}"
export PREFIX HOME
export PATH="${PREFIX}/bin:${PATH:-/system/bin}"

"${PREFIX}/bin/termux-wake-lock" 2>/dev/null || true
if ! "${PREFIX}/bin/pgrep" -f '^.*/camera-hub$' >/dev/null 2>&1; then
    nohup "${PREFIX}/bin/camera-hub-start" \
        >> "${HOME}/camera-hub.log" 2>&1 &
fi
