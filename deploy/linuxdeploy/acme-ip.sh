#!/bin/sh
set -eu

ENV_FILE="/home/android/.config/camera-hub.env"
LEGO="/usr/local/bin/lego"
LEGO_VERSION="4.32.0"
LEGO_PATH="/home/android/.config/camera-hub-acme-domain"

[ -r "$ENV_FILE" ] || {
    echo "camera-hub environment file not found: $ENV_FILE" >&2
    exit 1
}
set -a
. "$ENV_FILE"
set +a

WEBROOT="${CAMERA_HUB_ACME_WEBROOT:-/home/android/.config/camera-hub-acme-webroot}"
INTERFACE="${CAMERA_HUB_PUBLIC_INTERFACE:-wlan0}"
PUBLIC_DOMAIN="${CAMERA_HUB_PUBLIC_DOMAIN:-mi6.gwghome.site}"
EMAIL="${CAMERA_HUB_ACME_EMAIL:-}"
CERT_FILE="${CAMERA_HUB_TLS_CERT:-/home/android/.config/camera-hub-cert.pem}"
KEY_FILE="${CAMERA_HUB_TLS_KEY:-/home/android/.config/camera-hub-key.pem}"

PUBLIC_IP="$(ip -6 -o addr show dev "$INTERFACE" scope global 2>/dev/null |
    awk '!/ temporary / && !/ deprecated / { sub(/\/.*/, "", $4); print $4; exit }')"
[ -n "$PUBLIC_IP" ] || {
    echo "no stable global IPv6 address found on $INTERFACE" >&2
    exit 1
}
[ -n "$PUBLIC_DOMAIN" ] || {
    echo "camera-hub public domain is empty" >&2
    exit 1
}

if [ ! -x "$LEGO" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT INT TERM
    curl -L --fail --retry 3 \
        -o "$TMP/lego.tgz" \
        "https://github.com/go-acme/lego/releases/download/v${LEGO_VERSION}/lego_v${LEGO_VERSION}_linux_arm64.tar.gz"
    tar xzf "$TMP/lego.tgz" -C "$TMP" lego
    install -m 0755 "$TMP/lego" "$LEGO"
fi

install -d -o android -g android "$LEGO_PATH" "$WEBROOT/.well-known/acme-challenge"

set -- --accept-tos --path "$LEGO_PATH" --domains "$PUBLIC_DOMAIN" \
    --http --http.webroot "$WEBROOT" --disable-cn
[ -z "$EMAIL" ] || set -- "$@" --email "$EMAIL"

MATCH=""
for candidate in "$LEGO_PATH"/certificates/*.crt; do
    [ -f "$candidate" ] || continue
    case "$candidate" in *.issuer.crt) continue ;; esac
    if openssl x509 -in "$candidate" -noout -checkhost "$PUBLIC_DOMAIN" >/dev/null 2>&1; then
        MATCH="$candidate"
        break
    fi
done

if [ -n "$MATCH" ]; then
    "$LEGO" "$@" renew --days 3 --profile shortlived
else
    "$LEGO" "$@" run --profile shortlived
fi

MATCH=""
for candidate in "$LEGO_PATH"/certificates/*.crt; do
    [ -f "$candidate" ] || continue
    case "$candidate" in *.issuer.crt) continue ;; esac
    if openssl x509 -in "$candidate" -noout -checkhost "$PUBLIC_DOMAIN" >/dev/null 2>&1; then
        MATCH="$candidate"
        break
    fi
done
[ -n "$MATCH" ] || {
    echo "issued certificate for $PUBLIC_DOMAIN was not found" >&2
    exit 1
}
SOURCE_KEY="${MATCH%.crt}.key"
[ -s "$SOURCE_KEY" ] || {
    echo "issued private key not found: $SOURCE_KEY" >&2
    exit 1
}

OLD_SUM="$(sha256sum "$CERT_FILE" 2>/dev/null | awk '{print $1}' || true)"
install -m 0644 -o android -g android "$MATCH" "${CERT_FILE}.new"
install -m 0600 -o android -g android "$SOURCE_KEY" "${KEY_FILE}.new"
mv "${CERT_FILE}.new" "$CERT_FILE"
mv "${KEY_FILE}.new" "$KEY_FILE"
NEW_SUM="$(sha256sum "$CERT_FILE" | awk '{print $1}')"

sed -i '/^CAMERA_HUB_PUBLIC_HOST=/d' "$ENV_FILE"
printf "CAMERA_HUB_PUBLIC_HOST='%s'\n" "$PUBLIC_DOMAIN" >> "$ENV_FILE"

if [ "$OLD_SUM" != "$NEW_SUM" ]; then
    pkill -x camera-hub 2>/dev/null || true
    pkill -f '[c]amera-hub-mux' 2>/dev/null || true
    pkill -f '[c]amera-hub-opus' 2>/dev/null || true
    su -s /bin/sh android -c \
        'nohup /usr/local/bin/camera-hub-start > /home/android/camera-hub.log 2>&1 &'
    sleep 3
fi

openssl x509 -in "$CERT_FILE" -noout -subject -issuer -dates -ext subjectAltName
