#!/bin/sh
set -eu

BINARY="${1:-}"
PUBLIC_HOST="${2:-}"
DDNS_BINARY="${3:-}"
VOICE_BINARY="${4:-}"
TTS_BINARY="${5:-}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ENV_FILE="/home/android/.config/camera-hub.env"
DDNS_ENV_FILE="/home/android/.config/camera-hub-ddns.env"
STARTER="/usr/local/bin/camera-hub-start"
DDNS_STARTER="/usr/local/bin/camera-hub-ddns-start"
VOICE_STARTER="/usr/local/bin/camera-hub-voice-start"
TTS_STARTER="/usr/local/bin/camera-hub-tts-start"
VOICE_AUDIO="/usr/local/bin/camera-hub-mi6-audio"
VOICE_LIB_DIR="/usr/local/lib/camera-hub-voice"
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

install -m 0755 "$BINARY" /usr/local/bin/camera-hub
setcap cap_net_bind_service=+ep /usr/local/bin/camera-hub
if [ -n "$DDNS_BINARY" ]; then
    [ -x "$DDNS_BINARY" ] || {
        echo "camera-hub-ddns binary not found: $DDNS_BINARY" >&2
        exit 1
    }
    install -m 0755 "$DDNS_BINARY" /usr/local/bin/camera-hub-ddns
fi
if [ -n "$VOICE_BINARY" ]; then
    [ -x "$VOICE_BINARY" ] || {
        echo "camera-hub-voice binary not found: $VOICE_BINARY" >&2
        exit 1
    }
    install -m 0755 "$VOICE_BINARY" /usr/local/bin/camera-hub-voice
    install -m 0755 "$SCRIPT_DIR/mi6-audio.sh" "$VOICE_AUDIO"
fi
if [ -n "$TTS_BINARY" ]; then
    [ -x "$TTS_BINARY" ] || {
        echo "camera-hub-tts binary not found: $TTS_BINARY" >&2
        exit 1
    }
    install -m 0755 "$TTS_BINARY" /usr/local/bin/camera-hub-tts
fi
RUNTIME_BINARY="$VOICE_BINARY"
[ -n "$RUNTIME_BINARY" ] || RUNTIME_BINARY="$TTS_BINARY"
if [ -n "$RUNTIME_BINARY" ]; then
    install -d "$VOICE_LIB_DIR"
    for library in "$(dirname "$RUNTIME_BINARY")"/libonnxruntime.so* \
        "$(dirname "$RUNTIME_BINARY")"/libsherpa-onnx-c-api.so*; do
        [ -f "$library" ] || continue
        install -m 0755 "$library" "$VOICE_LIB_DIR/$(basename "$library")"
    done
fi
install -d -o android -g android /home/android/camera-data
install -d -o android -g android /home/android/camera-data/voice
install -d -m 0700 -o android -g android /home/android/camera-data/voice/tts
install -d -o android -g android /home/android/camera-voice/models
install -d -o android -g android /home/android/.config
install -d -m 0700 -o android -g android /home/android/.ssh
install -d -o android -g android "$ACME_WEBROOT/.well-known/acme-challenge"
install -m 0755 "$SCRIPT_DIR/acme-ip.sh" "$ACME_SCRIPT"
install -m 0755 "$SCRIPT_DIR/acme-edge.sh" "$EDGE_ACME_SCRIPT"

if [ ! -f "$DDNS_ENV_FILE" ]; then
    install -m 0600 -o android -g android \
        "$SCRIPT_DIR/camera-hub-ddns.env.example" "$DDNS_ENV_FILE"
else
    chown android:android "$DDNS_ENV_FILE"
    chmod 0600 "$DDNS_ENV_FILE"
fi

if [ ! -f "$ENV_FILE" ]; then
    {
        echo "CAMERA_HUB_WEB_USERNAME='admin'"
        echo "CAMERA_HUB_WEB_PASSWORD='12345'"
        echo "CAMERA_HUB_BIND='[::]:80'"
        echo "CAMERA_HUB_TLS_BIND='[::]:443'"
        echo "CAMERA_HUB_TLS_CERT='$TLS_CERT'"
        echo "CAMERA_HUB_TLS_KEY='$TLS_KEY'"
        echo "CAMERA_HUB_MOQ_ENABLED='true'"
        echo "CAMERA_HUB_MOQ_BIND='[::]:443'"
        echo "CAMERA_HUB_ACME_WEBROOT='$ACME_WEBROOT'"
        echo "CAMERA_HUB_PUBLIC_INTERFACE='wlan0'"
        echo "CAMERA_HUB_PUBLIC_DOMAIN='mi6.gwghome.site'"
        echo "CAMERA_HUB_EDGE_ACME_ENABLED='false'"
        echo "CAMERA_HUB_EDGE_DEVICE_ID='v831cam'"
        echo "CAMERA_HUB_EDGE_DOMAIN='v831.gwghome.site'"
        echo "CAMERA_HUB_EDGE_SSH_USER='root'"
        echo "CAMERA_HUB_EDGE_SSH_KEY='$EDGE_ACME_KEY'"
        echo "CAMERA_HUB_EDGE_RUNTIME_DIR='/root/maix_dist'"
        echo "CAMERA_HUB_DATA_DIR='/home/android/camera-data'"
        echo "CAMERA_HUB_SETTINGS_FILE='/home/android/.config/camera-hub.json'"
        echo "CAMERA_HUB_QQ_CONFIG_FILE='/home/android/.config/camera-hub-qq.json'"
        echo "CAMERA_HUB_VOICE_CONFIG_FILE='/home/android/.config/camera-hub-voice.json'"
        echo "CAMERA_HUB_VOICE_STATUS_FILE='/home/android/.config/camera-hub-voice-status.json'"
        echo "CAMERA_HUB_VOICE_EVENTS_FILE='/home/android/camera-data/voice/events.jsonl'"
        echo "CAMERA_HUB_VOICE_COMMAND_FILE='/home/android/.config/camera-hub-voice-command.json'"
        echo "CAMERA_HUB_VOICE_MODEL_DIR='/home/android/camera-voice/models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01'"
        echo "CAMERA_HUB_TTS_BIND='127.0.0.1:39081'"
        echo "CAMERA_HUB_TTS_URL='http://127.0.0.1:39081'"
        echo "CAMERA_HUB_TTS_TOKEN='$(openssl rand -hex 32)'"
        echo "CAMERA_HUB_TTS_MODEL_DIR='/home/android/camera-voice/models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia'"
        echo "CAMERA_HUB_TTS_VOCODER='/home/android/camera-voice/models/vocos_24khz.onnx'"
        echo "CAMERA_HUB_TTS_DATA_DIR='/home/android/camera-data/voice/tts'"
        echo "CAMERA_HUB_TTS_THREADS='2'"
        echo "CAMERA_HUB_TTS_MAX_DATA_BYTES='536870912'"
        echo "CAMERA_HUB_SEGMENT_SECONDS='600'"
        echo "CAMERA_HUB_MAX_BYTES='8589934592'"
        echo "CAMERA_HUB_RETAIN_DAYS='7'"
        echo "CAMERA_HUB_AI_ENABLED='true'"
        echo "CAMERA_HUB_AI_RUNTIME='/home/android/camera-ai/runtime/lib/libonnxruntime.so'"
        echo "CAMERA_HUB_AI_MODEL='/home/android/camera-ai/models/yolox_nano.onnx'"
        echo "CAMERA_HUB_AI_INTERVAL_MS='1000'"
        echo "CAMERA_HUB_AI_THRESHOLD='0.30'"
        echo "CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO='0.02'"
        echo "CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS='10'"
        echo "CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT='500'"
        echo "CAMERA_HUB_AI_SNAPSHOT_QUALITY='95'"
    } > "$ENV_FILE"
    chown android:android "$ENV_FILE"
    chmod 0600 "$ENV_FILE"
fi

grep -q '^CAMERA_HUB_WEB_USERNAME=' "$ENV_FILE" ||
    echo "CAMERA_HUB_WEB_USERNAME='admin'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_WEB_PASSWORD=' "$ENV_FILE" ||
    echo "CAMERA_HUB_WEB_PASSWORD='12345'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_ENABLED=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_ENABLED='true'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_RUNTIME=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_RUNTIME='/home/android/camera-ai/runtime/lib/libonnxruntime.so'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_MODEL=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_MODEL='/home/android/camera-ai/models/yolox_nano.onnx'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_INTERVAL_MS=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_INTERVAL_MS='1000'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_THRESHOLD=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_THRESHOLD='0.30'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO='0.02'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS='10'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT='500'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_AI_SNAPSHOT_QUALITY=' "$ENV_FILE" ||
    echo "CAMERA_HUB_AI_SNAPSHOT_QUALITY='95'" >> "$ENV_FILE"
sed -i \
    "/^CAMERA_HUB_SPEECH_TRANSCRIBE=/d;
     /^CAMERA_HUB_SPEECH_SUMMARIZE=/d;
     /^CAMERA_HUB_AI_SNAPSHOT_RETAIN_DAYS=/d;
     /^CAMERA_HUB_VOICE_STUDIO_ENABLED=/d;
     /^CAMERA_HUB_VOICE_STUDIO_TOKEN=/d" \
    "$ENV_FILE"
grep -q '^CAMERA_HUB_TLS_BIND=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TLS_BIND='[::]:443'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TLS_CERT=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TLS_CERT='$TLS_CERT'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TLS_KEY=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TLS_KEY='$TLS_KEY'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_SETTINGS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_SETTINGS_FILE='/home/android/.config/camera-hub.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_QQ_CONFIG_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_QQ_CONFIG_FILE='/home/android/.config/camera-hub-qq.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_CONFIG_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_CONFIG_FILE='/home/android/.config/camera-hub-voice.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_STATUS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_STATUS_FILE='/home/android/.config/camera-hub-voice-status.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_EVENTS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_EVENTS_FILE='/home/android/camera-data/voice/events.jsonl'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_COMMAND_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_COMMAND_FILE='/home/android/.config/camera-hub-voice-command.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_MODEL_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_MODEL_DIR='/home/android/camera-voice/models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_BIND=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_BIND='127.0.0.1:39081'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_URL=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_URL='http://127.0.0.1:39081'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_TOKEN=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_TOKEN='$(openssl rand -hex 32)'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_MODEL_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_MODEL_DIR='/home/android/camera-voice/models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_VOCODER=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_VOCODER='/home/android/camera-voice/models/vocos_24khz.onnx'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_DATA_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_DATA_DIR='/home/android/camera-data/voice/tts'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_THREADS=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_THREADS='2'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_TTS_MAX_DATA_BYTES=' "$ENV_FILE" ||
    echo "CAMERA_HUB_TTS_MAX_DATA_BYTES='536870912'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_MOQ_ENABLED=' "$ENV_FILE" ||
    echo "CAMERA_HUB_MOQ_ENABLED='true'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_MOQ_BIND=' "$ENV_FILE" ||
    echo "CAMERA_HUB_MOQ_BIND='[::]:443'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_ACME_WEBROOT=' "$ENV_FILE" ||
    echo "CAMERA_HUB_ACME_WEBROOT='$ACME_WEBROOT'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_PUBLIC_INTERFACE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_PUBLIC_INTERFACE='wlan0'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_PUBLIC_DOMAIN=' "$ENV_FILE" ||
    echo "CAMERA_HUB_PUBLIC_DOMAIN='mi6.gwghome.site'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_ACME_ENABLED=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_ACME_ENABLED='false'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_DEVICE_ID=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_DEVICE_ID='v831cam'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_DOMAIN=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_DOMAIN='v831.gwghome.site'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_SSH_USER=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_SSH_USER='root'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_SSH_KEY=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_SSH_KEY='$EDGE_ACME_KEY'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_EDGE_RUNTIME_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_EDGE_RUNTIME_DIR='/root/maix_dist'" >> "$ENV_FILE"
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
set -eu
set -a
. /home/android/.config/camera-hub.env
set +a
exec /usr/local/bin/camera-hub
EOF
chmod 0755 "$STARTER"

cat > "$DDNS_STARTER" <<'EOF'
#!/bin/sh
set -eu
set -a
. /home/android/.config/camera-hub-ddns.env
set +a
exec /usr/local/bin/camera-hub-ddns
EOF
chmod 0755 "$DDNS_STARTER"

if [ -n "$TTS_BINARY" ]; then
    cat > "$TTS_STARTER" <<'EOF'
#!/bin/sh
set -u
while :; do
    status=0
    su -s /bin/sh android -c '
        set -a
        . /home/android/.config/camera-hub.env
        set +a
        LD_LIBRARY_PATH=/usr/local/lib/camera-hub-voice \
            exec /usr/local/bin/camera-hub-tts
    ' || status=$?
    echo "camera-hub-tts exited with status ${status}; restarting in 2 seconds" >&2
    sleep 2
done
EOF
    chmod 0755 "$TTS_STARTER"
fi

if [ -n "$VOICE_BINARY" ]; then
    if ! command -v espeak-ng >/dev/null 2>&1; then
        DEBIAN_FRONTEND=noninteractive apt-get update
        DEBIAN_FRONTEND=noninteractive apt-get install -y espeak-ng
    fi
    cat > "$VOICE_STARTER" <<'EOF'
#!/bin/sh
set -u
/usr/local/bin/camera-hub-mi6-audio setup || exit $?
set -a
. /home/android/.config/camera-hub.env
set +a
if [ -x /usr/local/bin/camera-hub-tts-start ]; then
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
        curl -fsS "${CAMERA_HUB_TTS_URL%/}/health" >/dev/null 2>&1 && break
        sleep 1
    done
fi
while :; do
    status=0
    su -s /bin/sh android -c '
        set -a
        . /home/android/.config/camera-hub.env
        set +a
        LD_LIBRARY_PATH=/usr/local/lib/camera-hub-voice \
            exec /usr/local/bin/camera-hub-voice
    ' || status=$?
    echo "camera-hub-voice exited with status ${status}; restarting in 2 seconds" >&2
    sleep 2
done
EOF
    chmod 0755 "$VOICE_STARTER"
fi

TMP="$(mktemp)"
awk '
    /^# BEGIN CAMERA HUB$/ { skip=1; next }
    /^# END CAMERA HUB$/ { skip=0; next }
    !skip { print }
' "$RC_LOCAL" > "$TMP"

awk '
    /^exit 0$/ {
        print "# BEGIN CAMERA HUB"
        print "if ! pgrep -x \"camera-hub\" > /dev/null; then"
        print "    su -s /bin/sh android -c '\''nohup /usr/local/bin/camera-hub-start > /home/android/camera-hub.log 2>&1 &'\''"
        print "fi"
        print "if [ -x /usr/local/bin/camera-hub-tts-start ] && ! pgrep -f \042[c]amera-hub-tts-start\042 > /dev/null; then"
        print "    nohup /usr/local/bin/camera-hub-tts-start > /home/android/camera-hub-tts.log 2>&1 &"
        print "fi"
        print "if [ -x /usr/local/bin/camera-hub-voice-start ] && ! pgrep -f \042[c]amera-hub-voice-start\042 > /dev/null; then"
        print "    nohup /usr/local/bin/camera-hub-voice-start > /home/android/camera-hub-voice.log 2>&1 &"
        print "fi"
        print "if ! pgrep -f \042[c]amera-hub-acme-loop\042 > /dev/null; then"
        print "    nohup sh -c \047sleep 30; while :; do /usr/local/bin/camera-hub-acme >> /home/android/camera-hub-acme.log 2>&1 || true; sleep 43200; done\047 camera-hub-acme-loop > /dev/null 2>&1 &"
        print "fi"
        print "if ! pgrep -f \042[c]amera-hub-acme-edge-loop\042 > /dev/null; then"
        print "    su -s /bin/sh android -c '\''nohup sh -c \"sleep 60; while :; do /usr/local/bin/camera-hub-acme-edge >> /home/android/camera-hub-acme-edge.log 2>&1 || true; sleep 43200; done\" camera-hub-acme-edge-loop > /dev/null 2>&1 &'\''"
        print "fi"
        print "if grep -q \"^CAMERA_HUB_DDNS_ENABLED='\''true'\''\" /home/android/.config/camera-hub-ddns.env && ! pgrep -x \"camera-hub-ddns\" > /dev/null; then"
        print "    su -s /bin/sh android -c '\''nohup /usr/local/bin/camera-hub-ddns-start > /home/android/camera-hub-ddns.log 2>&1 &'\''"
        print "fi"
        print "# END CAMERA HUB"
    }
    { print }
' "$TMP" > "$RC_LOCAL"
rm -f "$TMP"
chmod 0755 "$RC_LOCAL"

pkill -x camera-hub 2>/dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do
    pgrep -x camera-hub > /dev/null 2>&1 || break
    sleep 1
done
pkill -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -f '[c]amera-hub-opus' 2>/dev/null || true
sleep 1
pkill -9 -f '[c]amera-hub-mux' 2>/dev/null || true
pkill -9 -f '[c]amera-hub-opus' 2>/dev/null || true
su -s /bin/sh android -c \
    'nohup /usr/local/bin/camera-hub-start > /home/android/camera-hub.log 2>&1 &'
if [ -x "$TTS_STARTER" ]; then
    pkill -f '[c]amera-hub-tts-start' 2>/dev/null || true
    pkill -f '^/usr/local/bin/camera-hub-tts( |$)' 2>/dev/null || true
    nohup "$TTS_STARTER" > /home/android/camera-hub-tts.log 2>&1 &
fi
if [ -x "$VOICE_STARTER" ]; then
    pkill -f '[c]amera-hub-voice-start' 2>/dev/null || true
    pkill -f '^/usr/local/bin/camera-hub-voice( |$)' 2>/dev/null || true
    nohup "$VOICE_STARTER" > /home/android/camera-hub-voice.log 2>&1 &
fi
sleep 2
if [ -x "$TTS_STARTER" ]; then
    set -a
    . "$ENV_FILE"
    set +a
    tts_ready=0
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 \
        21 22 23 24 25 26 27 28 29 30; do
        if curl -fsS "${CAMERA_HUB_TTS_URL%/}/health" >/dev/null 2>&1; then
            tts_ready=1
            break
        fi
        sleep 1
    done
    if [ "$tts_ready" -ne 1 ]; then
        echo "camera-hub-tts failed to become ready" >&2
        tail -n 80 /home/android/camera-hub-tts.log >&2 || true
        exit 1
    fi
fi
if [ -x "$VOICE_STARTER" ]; then
    voice_ready=0
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 \
        21 22 23 24 25 26 27 28 29 30; do
        if pgrep -f '^/usr/local/bin/camera-hub-voice( |$)' >/dev/null 2>&1 &&
            grep -Eq '"available"[[:space:]]*:[[:space:]]*true' \
                /home/android/.config/camera-hub-voice-status.json 2>/dev/null; then
            voice_ready=1
            break
        fi
        sleep 1
    done
    if [ "$voice_ready" -ne 1 ]; then
        echo "camera-hub-voice failed to become ready" >&2
        tail -n 80 /home/android/camera-hub-voice.log >&2 || true
        exit 1
    fi
fi
curl -g -fsS 'http://[::1]/health'
echo
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
if grep -q "^CAMERA_HUB_DDNS_ENABLED='true'" "$DDNS_ENV_FILE" &&
    [ -x /usr/local/bin/camera-hub-ddns ] &&
    ! pgrep -x camera-hub-ddns > /dev/null; then
    su -s /bin/sh android -c \
        'nohup /usr/local/bin/camera-hub-ddns-start > /home/android/camera-hub-ddns.log 2>&1 &'
fi
