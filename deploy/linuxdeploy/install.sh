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
ASSET_CACHE_DIR="/home/android/camera-voice"
COMPONENTS_FILE="/home/android/.config/camera-hub-components.json"
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
install -m 0755 "$SCRIPT_DIR/mi6-audio.sh" "$VOICE_AUDIO"
if ! command -v espeak-ng >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y espeak-ng
fi
install -d -o android -g android /home/android/camera-data
install -d -o android -g android /home/android/camera-data/voice
install -d -m 0700 -o android -g android /home/android/camera-data/voice/tts
install -d -o android -g android "$ASSET_CACHE_DIR"
install -d -o android -g android /home/android/camera-voice/models
install -d -o android -g android /home/android/.config
install -d -m 0700 -o android -g android /home/android/.ssh
install -d -o android -g android "$ACME_WEBROOT/.well-known/acme-challenge"
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
    else
        install -m 0600 -o android -g android \
            "$SCRIPT_DIR/camera-hub-ddns.json.example" "$DDNS_CONFIG_FILE"
    fi
fi
chown android:android "$DDNS_CONFIG_FILE"
chmod 0600 "$DDNS_CONFIG_FILE"
if [ -f "$DDNS_STATUS_FILE" ]; then
    chown android:android "$DDNS_STATUS_FILE"
    chmod 0600 "$DDNS_STATUS_FILE"
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
        echo "CAMERA_HUB_DDNS_CONFIG_FILE='$DDNS_CONFIG_FILE'"
        echo "CAMERA_HUB_DDNS_STATUS_FILE='$DDNS_STATUS_FILE'"
        echo "CAMERA_HUB_DDNS_STATE_FILE='/home/android/.config/camera-hub-ddns.state'"
        echo "CAMERA_HUB_COMPONENT_MANAGER_ENABLED='true'"
        echo "CAMERA_HUB_COMPONENTS_FILE='$COMPONENTS_FILE'"
        echo "CAMERA_HUB_LOG_DIR='/home/android'"
        echo "CAMERA_HUB_IR_BIND='127.0.0.1:39182'"
        echo "CAMERA_HUB_IR_URL='http://127.0.0.1:39182'"
        echo "CAMERA_HUB_IR_DEVICE='/dev/peel_ir'"
        echo "CAMERA_HUB_ASSET_CACHE_DIR='$ASSET_CACHE_DIR'"
        echo "CAMERA_HUB_VOICE_CONFIG_FILE='/home/android/.config/camera-hub-voice.json'"
        echo "CAMERA_HUB_VOICE_STATUS_FILE='/home/android/.config/camera-hub-voice-status.json'"
        echo "CAMERA_HUB_VOICE_EVENTS_FILE='/home/android/camera-data/voice/events.jsonl'"
        echo "CAMERA_HUB_VOICE_COMMAND_FILE='/home/android/.config/camera-hub-voice-command.json'"
        echo "CAMERA_HUB_VOICE_LIB_DIR='$VOICE_LIB_DIR'"
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
grep -q '^CAMERA_HUB_DDNS_CONFIG_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_DDNS_CONFIG_FILE='$DDNS_CONFIG_FILE'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_DDNS_STATUS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_DDNS_STATUS_FILE='$DDNS_STATUS_FILE'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_DDNS_STATE_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_DDNS_STATE_FILE='/home/android/.config/camera-hub-ddns.state'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_COMPONENT_MANAGER_ENABLED=' "$ENV_FILE" ||
    echo "CAMERA_HUB_COMPONENT_MANAGER_ENABLED='true'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_COMPONENTS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_COMPONENTS_FILE='$COMPONENTS_FILE'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_LOG_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_LOG_DIR='/home/android'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_IR_BIND=' "$ENV_FILE" ||
    echo "CAMERA_HUB_IR_BIND='127.0.0.1:39182'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_IR_URL=' "$ENV_FILE" ||
    echo "CAMERA_HUB_IR_URL='http://127.0.0.1:39182'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_IR_DEVICE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_IR_DEVICE='/dev/peel_ir'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_ASSET_CACHE_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_ASSET_CACHE_DIR='$ASSET_CACHE_DIR'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_CONFIG_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_CONFIG_FILE='/home/android/.config/camera-hub-voice.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_STATUS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_STATUS_FILE='/home/android/.config/camera-hub-voice-status.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_EVENTS_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_EVENTS_FILE='/home/android/camera-data/voice/events.jsonl'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_COMMAND_FILE=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_COMMAND_FILE='/home/android/.config/camera-hub-voice-command.json'" >> "$ENV_FILE"
grep -q '^CAMERA_HUB_VOICE_LIB_DIR=' "$ENV_FILE" ||
    echo "CAMERA_HUB_VOICE_LIB_DIR='$VOICE_LIB_DIR'" >> "$ENV_FILE"
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
