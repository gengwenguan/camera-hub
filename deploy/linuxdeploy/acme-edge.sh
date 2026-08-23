#!/bin/sh
set -eu

ENV_FILE="/home/android/.config/camera-hub.env"
LEGO="/usr/local/bin/lego"
LEGO_VERSION="4.32.0"

[ -r "$ENV_FILE" ] || {
    echo "camera-hub environment file not found: $ENV_FILE" >&2
    exit 1
}
set -a
. "$ENV_FILE"
set +a

ENABLED="${CAMERA_HUB_EDGE_ACME_ENABLED:-false}"
[ "$ENABLED" = "true" ] || exit 0

DEVICE_ID="${CAMERA_HUB_EDGE_DEVICE_ID:-v831cam}"
EDGE_DOMAIN="${CAMERA_HUB_EDGE_DOMAIN:-v831.gwghome.site}"
SSH_USER="${CAMERA_HUB_EDGE_SSH_USER:-root}"
SSH_KEY="${CAMERA_HUB_EDGE_SSH_KEY:-/home/android/.ssh/camera-hub-edge-acme-rsa}"
REMOTE_DIR="${CAMERA_HUB_EDGE_RUNTIME_DIR:-/root/maix_dist}"
EMAIL="${CAMERA_HUB_ACME_EMAIL:-}"
LEGO_PATH="/home/android/.config/camera-hub-edge-acme-domain/${DEVICE_ID}"
WEBROOT="/home/android/.config/camera-hub-edge-acme-webroot/${DEVICE_ID}"
HOST_FILE="$LEGO_PATH/last-ipv6"

case "$DEVICE_ID" in
    *[!A-Za-z0-9_.-]*|'')
        echo "invalid edge device id: $DEVICE_ID" >&2
        exit 1
        ;;
esac
case "$REMOTE_DIR" in
    /*) ;;
    *)
        echo "edge runtime directory must be absolute: $REMOTE_DIR" >&2
        exit 1
        ;;
esac
[ -r "$SSH_KEY" ] || {
    echo "edge SSH key is missing: $SSH_KEY" >&2
    exit 1
}

PUBLIC_HOST="$(
    getent ahosts "$EDGE_DOMAIN" 2>/dev/null |
        awk '{print $1}' |
        python3 -c '
import ipaddress, sys
for line in sys.stdin:
    value = line.strip()
    try:
        address = ipaddress.ip_address(value)
    except ValueError:
        continue
    if address.version == 6 and address.is_global:
        print(address.compressed)
        break
'
)"
if [ -z "$PUBLIC_HOST" ] && [ -s "$HOST_FILE" ]; then
    CACHED_HOST="$(cat "$HOST_FILE")"
    HUB_HOST="$(ip -6 -o addr show dev "${CAMERA_HUB_PUBLIC_INTERFACE:-wlan0}" scope global 2>/dev/null |
        awk '!/ temporary / && !/ deprecated / { sub(/\/.*/, "", $4); print $4; exit }')"
    PUBLIC_HOST="$(
        python3 -c '
import ipaddress, sys
try:
    hub = ipaddress.ip_address(sys.argv[1])
    edge = ipaddress.ip_address(sys.argv[2])
except ValueError:
    raise SystemExit
if hub.version == 6 and edge.version == 6:
    value = (int(hub) & (((1 << 128) - 1) ^ ((1 << 64) - 1))) | (int(edge) & ((1 << 64) - 1))
    rebuilt = ipaddress.ip_address(value)
    if rebuilt.is_global:
        print(rebuilt.compressed)
' "$HUB_HOST" "$CACHED_HOST"
    )"
fi
[ -n "$PUBLIC_HOST" ] || {
    echo "online edge device has no stable global IPv6: $DEVICE_ID" >&2
    exit 0
}
install -d "$LEGO_PATH"
printf '%s\n' "$PUBLIC_HOST" > "${HOST_FILE}.new"
mv "${HOST_FILE}.new" "$HOST_FILE"

SSH_OPTIONS="-6 -i $SSH_KEY -o HostName=$PUBLIC_HOST -o BatchMode=yes \
    -o ConnectTimeout=10 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedKeyTypes=+ssh-rsa \
    -o KexAlgorithms=+diffie-hellman-group1-sha1,diffie-hellman-group14-sha1"

edge_ssh() {
    # shellcheck disable=SC2086
    ssh $SSH_OPTIONS "${SSH_USER}@camera-hub-edge" "$@"
}

edge_scp() {
    # shellcheck disable=SC2086
    scp $SSH_OPTIONS "$1" "${SSH_USER}@camera-hub-edge:$2"
}

edge_ssh "mkdir -p '$REMOTE_DIR/state/tls' \
    '$REMOTE_DIR/state/acme-webroot/.well-known/acme-challenge';
    chmod 700 '$REMOTE_DIR/state/tls'" >/dev/null

if [ ! -x "$LEGO" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT INT TERM
    curl -L --fail --retry 3 \
        -o "$TMP/lego.tgz" \
        "https://github.com/go-acme/lego/releases/download/v${LEGO_VERSION}/lego_v${LEGO_VERSION}_linux_arm64.tar.gz"
    tar xzf "$TMP/lego.tgz" -C "$TMP" lego
    sudo -n install -m 0755 "$TMP/lego" "$LEGO"
fi

install -d "$WEBROOT/.well-known/acme-challenge"
rm -f "$WEBROOT/.well-known/acme-challenge/"*
edge_ssh "rm -f '$REMOTE_DIR/state/acme-webroot/.well-known/acme-challenge/'*" || true

sync_challenges() {
    while :; do
        tar -C "$WEBROOT" -cf - .well-known 2>/dev/null |
            edge_ssh "tar -xf - -C '$REMOTE_DIR/state/acme-webroot'" >/dev/null 2>&1 || true
        sleep 1
    done
}

set -- --accept-tos --path "$LEGO_PATH" --domains "$EDGE_DOMAIN" \
    --http --http.webroot "$WEBROOT" --http.delay 8s --disable-cn
[ -z "$EMAIL" ] || set -- "$@" --email "$EMAIL"

MATCH=""
for candidate in "$LEGO_PATH"/certificates/*.crt; do
    [ -f "$candidate" ] || continue
    case "$candidate" in *.issuer.crt) continue ;; esac
    if openssl x509 -in "$candidate" -noout -checkhost "$EDGE_DOMAIN" >/dev/null 2>&1; then
        MATCH="$candidate"
        break
    fi
done

sync_challenges &
SYNC_PID=$!
trap 'kill "$SYNC_PID" 2>/dev/null || true; wait "$SYNC_PID" 2>/dev/null || true' EXIT INT TERM
if [ -n "$MATCH" ]; then
    "$LEGO" "$@" renew --days 3 --profile shortlived
else
    "$LEGO" "$@" run --profile shortlived
fi
kill "$SYNC_PID" 2>/dev/null || true
wait "$SYNC_PID" 2>/dev/null || true
trap - EXIT INT TERM

MATCH=""
for candidate in "$LEGO_PATH"/certificates/*.crt; do
    [ -f "$candidate" ] || continue
    case "$candidate" in *.issuer.crt) continue ;; esac
    if openssl x509 -in "$candidate" -noout -checkhost "$EDGE_DOMAIN" >/dev/null 2>&1; then
        MATCH="$candidate"
        break
    fi
done
[ -n "$MATCH" ] || {
    echo "issued certificate for edge domain was not found: $EDGE_DOMAIN" >&2
    exit 1
}
SOURCE_KEY="${MATCH%.crt}.key"
[ -s "$SOURCE_KEY" ] || {
    echo "issued edge private key not found: $SOURCE_KEY" >&2
    exit 1
}

REMOTE_TLS="$REMOTE_DIR/state/tls"
OLD_SUM="$(edge_ssh "sha256sum '$REMOTE_TLS/fullchain.pem' 2>/dev/null | awk '{print \$1}'" || true)"
edge_scp "$MATCH" "$REMOTE_TLS/fullchain.pem.new"
edge_scp "$SOURCE_KEY" "$REMOTE_TLS/private.key.new"
edge_ssh "chmod 644 '$REMOTE_TLS/fullchain.pem.new';
    chmod 600 '$REMOTE_TLS/private.key.new';
    mv '$REMOTE_TLS/fullchain.pem.new' '$REMOTE_TLS/fullchain.pem';
    mv '$REMOTE_TLS/private.key.new' '$REMOTE_TLS/private.key';
    rm -f '$REMOTE_DIR/state/acme-webroot/.well-known/acme-challenge/'*"
NEW_SUM="$(sha256sum "$MATCH" | awk '{print $1}')"

if [ "$OLD_SUM" != "$NEW_SUM" ]; then
    edge_ssh "cd '$REMOTE_DIR';
        variant=\$(cat active_variant 2>/dev/null || true);
        case \"\$variant\" in
            rust|cpp) ./switch_camera.sh \"\$variant\" ;;
            *) killall -9 camera 2>/dev/null || true ;;
        esac"
fi

openssl x509 -in "$MATCH" -noout -subject -issuer -dates -ext subjectAltName
