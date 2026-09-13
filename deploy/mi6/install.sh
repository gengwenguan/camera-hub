#!/bin/sh
set -eu

BINARY="${1:-}"
PUBLIC_HOST="${2:-}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ENV_FILE="/home/android/.config/camera-hub.env"
DDNS_ENV_FILE="/home/android/.config/camera-hub-ddns.env"
DDNS_CONFIG_FILE="/home/android/.config/camera-hub-ddns.json"
DDNS_STATUS_FILE="/home/android/.config/camera-hub-ddns-status.json"
STARTER="/usr/local/bin/camera-hub-start"
VOICE_AUDIO="/usr/local/bin/camera-hub-mi6-audio"
VOICE_LIB_DIR="/usr/local/lib/camera-hub-voice"
VOICE_LD_CONFIG="/etc/ld.so.conf.d/camera-hub-voice.conf"
ACME_SCRIPT="/usr/local/bin/camera-hub-acme"
EDGE_ACME_SCRIPT="/usr/local/bin/camera-hub-acme-edge"
RC_LOCAL="/etc/rc.local"
TLS_CERT="/home/android/.config/camera-hub-cert.pem"
TLS_KEY="/home/android/.config/camera-hub-key.pem"
ACME_WEBROOT="/home/android/.config/camera-hub-acme-webroot"
EDGE_ACME_KEY="/home/android/.ssh/camera-hub-edge-acme-rsa"

[ -x "$BINARY" ] || {
    echo "camera-hub binary not found: $BINARY" >&2
    exit 1
}

install -d "$VOICE_LIB_DIR"
for library in "$(dirname "$BINARY")"/libonnxruntime.so* \
    "$(dirname "$BINARY")"/libsherpa-onnx-c-api.so*; do
    [ -f "$library" ] || continue
    install -m 0755 "$library" "$VOICE_LIB_DIR/$(basename "$library")"
done
printf '%s\n' "$VOICE_LIB_DIR" > "$VOICE_LD_CONFIG"
ldconfig
install -m 0755 "$BINARY" /usr/local/bin/camera-hub
setcap cap_net_bind_service=+ep /usr/local/bin/camera-hub
if ldd /usr/local/bin/camera-hub | grep -q 'not found'; then
    echo "camera-hub has unresolved shared libraries:" >&2
    ldd /usr/local/bin/camera-hub >&2
    exit 1
fi
install -m 0755 "$SCRIPT_DIR/audio.sh" "$VOICE_AUDIO"
if ! command -v espeak-ng >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y espeak-ng
fi
install -d -o android -g android /home/android/.config
install -d -m 0700 -o android -g android /home/android/.ssh
for log_file in \
    /home/android/camera-hub.log \
    /home/android/camera-hub-ddns.log \
    /home/android/camera-hub-ir.log \
    /home/android/camera-hub-tts.log \
    /home/android/camera-hub-voice.log; do
    touch "$log_file"
    chown android:android "$log_file"
    chmod 0600 "$log_file"
done
install -m 0755 "$SCRIPT_DIR/acme-ip.sh" "$ACME_SCRIPT"
install -m 0755 "$SCRIPT_DIR/acme-edge.sh" "$EDGE_ACME_SCRIPT"

if [ ! -f "$DDNS_CONFIG_FILE" ]; then
    if [ -f "$DDNS_ENV_FILE" ]; then
        su -s /bin/sh android -c "
            set -a
            . '$DDNS_ENV_FILE'
            set +a
            /usr/local/bin/camera-hub worker ddns \
                --config-file '$DDNS_CONFIG_FILE' --write-config
        "
    fi
fi

su -s /bin/sh android -c \
    "/usr/local/bin/camera-hub setup mi6 --home /home/android"
chown android:android "$DDNS_CONFIG_FILE"
chmod 0600 "$DDNS_CONFIG_FILE"
if [ -f "$DDNS_STATUS_FILE" ]; then
    chown android:android "$DDNS_STATUS_FILE"
    chmod 0600 "$DDNS_STATUS_FILE"
fi

chown android:android "$ENV_FILE"
chmod 0600 "$ENV_FILE"

if [ ! -s "$EDGE_ACME_KEY" ] || [ ! -s "${EDGE_ACME_KEY}.pub" ]; then
    su -s /bin/sh android -c \
        "ssh-keygen -q -t rsa -b 2048 -N '' -f '$EDGE_ACME_KEY'"
fi
chown android:android "$EDGE_ACME_KEY" "${EDGE_ACME_KEY}.pub"
chmod 0600 "$EDGE_ACME_KEY"
chmod 0644 "${EDGE_ACME_KEY}.pub"

CERT_TEXT="$(openssl x509 -in "$TLS_CERT" -noout -text 2>/dev/null |
    tr '[:lower:]' '[:upper:]' || true)"
PUBLIC_HOST_UPPER="$(printf '%s' "$PUBLIC_HOST" | tr '[:lower:]' '[:upper:]')"
if [ ! -s "$TLS_CERT" ] || [ ! -s "$TLS_KEY" ] ||
    { [ -n "$PUBLIC_HOST_UPPER" ] &&
      ! printf '%s' "$CERT_TEXT" | grep -Fq "$PUBLIC_HOST_UPPER"; }; then
    IPV6="$(ip -6 -o addr show scope global 2>/dev/null |
        awk 'NR == 1 { sub(/\/.*/, "", $4); print $4 }')"
    SAN="DNS:camera-hub"
    [ -z "$PUBLIC_HOST" ] || SAN="$SAN,IP:$PUBLIC_HOST"
    [ -z "$IPV6" ] || SAN="$SAN,IP:$IPV6"
    openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 \
        -subj "/CN=camera-hub" \
        -addext "subjectAltName=$SAN" \
        -keyout "$TLS_KEY" -out "$TLS_CERT"
    chown android:android "$TLS_CERT" "$TLS_KEY"
    chmod 0644 "$TLS_CERT"
    chmod 0600 "$TLS_KEY"
fi

cat > "$STARTER" <<'EOF'
#!/bin/sh
set -u
while :; do
    status=0
    set -a
    . /home/android/.config/camera-hub.env
    set +a
    export LD_LIBRARY_PATH="${CAMERA_HUB_VOICE_LIB_DIR:-/usr/local/lib/camera-hub-voice}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    /usr/local/bin/camera-hub server || status=$?
    echo "camera-hub exited with status ${status}; restarting in 2 seconds" >&2
    sleep 2
done
EOF
chmod 0755 "$STARTER"

TMP="$(mktemp)"
awk '
    /^# BEGIN CAMERA HUB$/ { skip=1; next }
    /^# END CAMERA HUB$/ { skip=0; next }
    !skip { print }
' "$RC_LOCAL" > "$TMP"

awk '
    /^exit 0$/ {
        print "# BEGIN CAMERA HUB"
        print "if [ -x /usr/local/bin/camera-hub-mi6-audio ]; then"
        print "    /usr/local/bin/camera-hub-mi6-audio setup > /home/android/camera-hub-audio.log 2>&1 || true"
        print "fi"
        print "if ! pgrep -f \042[c]amera-hub-start\042 > /dev/null; then"
        print "    su -s /bin/sh android -c '\''nohup /usr/local/bin/camera-hub-start > /home/android/camera-hub.log 2>&1 &'\''"
        print "fi"
        print "if ! pgrep -f \042[c]amera-hub-acme-loop\042 > /dev/null; then"
        print "    nohup sh -c \047sleep 30; while :; do /usr/local/bin/camera-hub-acme >> /home/android/camera-hub-acme.log 2>&1 || true; sleep 43200; done\047 camera-hub-acme-loop > /dev/null 2>&1 &"
        print "fi"
        print "if ! pgrep -f \042[c]amera-hub-acme-edge-loop\042 > /dev/null; then"
        print "    su -s /bin/sh android -c '\''nohup sh -c \"sleep 60; while :; do /usr/local/bin/camera-hub-acme-edge >> /home/android/camera-hub-acme-edge.log 2>&1 || true; sleep 43200; done\" camera-hub-acme-edge-loop > /dev/null 2>&1 &'\''"
        print "fi"
        print "# END CAMERA HUB"
    }
    { print }
' "$TMP" > "$RC_LOCAL"
rm -f "$TMP"
chmod 0755 "$RC_LOCAL"

pkill -f '[c]amera-hub-ddns-start' 2>/dev/null || true
pkill -f '[c]amera-hub-tts-start' 2>/dev/null || true
pkill -f '[c]amera-hub-voice-start' 2>/dev/null || true
pkill -f '[c]amera-hub-start' 2>/dev/null || true
pkill -f '^/usr/local/bin/camera-hub( |$)' 2>/dev/null || true
pkill -f '^/usr/local/bin/camera-hub-ddns( |$)' 2>/dev/null || true
pkill -f '^/usr/local/bin/camera-hub-tts( |$)' 2>/dev/null || true
pkill -f '^/usr/local/bin/camera-hub-voice( |$)' 2>/dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ! pgrep -f '^/usr/local/bin/camera-hub( |$)' >/dev/null 2>&1 &&
        ! pgrep -f '^/usr/local/bin/camera-hub-(ddns|tts|voice)( |$)' >/dev/null 2>&1; then
        break
    fi
    sleep 1
done
pkill -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -f '[c]amera-hub-opus' 2>/dev/null || true
sleep 1
pkill -9 -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -9 -f '[c]amera-hub-opus' 2>/dev/null || true
rm -f \
    /usr/local/bin/camera-hub-ddns \
    /usr/local/bin/camera-hub-ddns-start \
    /usr/local/bin/camera-hub-tts \
    /usr/local/bin/camera-hub-tts-start \
    /usr/local/bin/camera-hub-voice \
    /usr/local/bin/camera-hub-voice-start \
    /usr/local/bin/camera-hub-service-control
"$VOICE_AUDIO" setup > /home/android/camera-hub-audio.log 2>&1 || {
    echo "warning: MI6 audio routing setup failed; see /home/android/camera-hub-audio.log" >&2
}
su -s /bin/sh android -c \
    'nohup /usr/local/bin/camera-hub-start > /home/android/camera-hub.log 2>&1 &'
hub_ready=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 \
    21 22 23 24 25 26 27 28 29 30; do
    if curl -g -fsS 'http://[::1]/health' >/dev/null 2>&1; then
        hub_ready=1
        break
    fi
    sleep 1
done
if [ "$hub_ready" -ne 1 ]; then
    echo "camera-hub failed to become ready" >&2
    tail -n 120 /home/android/camera-hub.log >&2 || true
    exit 1
fi
curl -g -fsS 'http://[::1]/health'; echo
if "$ACME_SCRIPT" > /home/android/camera-hub-acme.log 2>&1; then
    curl -g -fsS 'http://[::1]/health'
    echo
else
    echo "warning: trusted IPv6 certificate issuance failed; see /home/android/camera-hub-acme.log" >&2
fi
if ! pgrep -f '[c]amera-hub-acme-loop' > /dev/null; then
    nohup sh -c \
        'sleep 43200; while :; do /usr/local/bin/camera-hub-acme >> /home/android/camera-hub-acme.log 2>&1 || true; sleep 43200; done' \
        camera-hub-acme-loop > /dev/null 2>&1 &
fi
if ! pgrep -f '[c]amera-hub-acme-edge-loop' > /dev/null; then
    su -s /bin/sh android -c \
        'nohup sh -c "sleep 60; while :; do /usr/local/bin/camera-hub-acme-edge >> /home/android/camera-hub-acme-edge.log 2>&1 || true; sleep 43200; done" camera-hub-acme-edge-loop > /dev/null 2>&1 &'
fi
