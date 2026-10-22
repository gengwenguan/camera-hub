#!/system/bin/sh
set -u

PACKAGE="site.gwghome.camerahub"
ROOTFS="/data/linux"
ACME_PATH="/home/android/.config/camera-hub-acme-standard"
DOMAIN="mi6.gwghome.site"
SOURCE_CERT="$ROOTFS$ACME_PATH/certificates/$DOMAIN.crt"
SOURCE_KEY="$ROOTFS$ACME_PATH/certificates/$DOMAIN.key"
APP_CERT="files/engine/camera-hub-state/cert.pem"
APP_KEY="files/engine/camera-hub-state/key.pem"
LOG="/data/adb/camera-hub-cert-renew.log"

log() {
    printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*"
}

app_cert_sum() {
    run-as "$PACKAGE" sha256sum "$APP_CERT" 2>/dev/null | awk '{print $1}'
}

sync_certificate() {
    run-as "$PACKAGE" sh -c "cat > '$APP_CERT.new'" < "$SOURCE_CERT" &&
        run-as "$PACKAGE" sh -c "cat > '$APP_KEY.new'" < "$SOURCE_KEY" &&
        run-as "$PACKAGE" chmod 644 "$APP_CERT.new" &&
        run-as "$PACKAGE" chmod 600 "$APP_KEY.new" &&
        run-as "$PACKAGE" mv "$APP_CERT.new" "$APP_CERT" &&
        run-as "$PACKAGE" mv "$APP_KEY.new" "$APP_KEY"
}

renew_once() {
    if [ ! -x "$ROOTFS/usr/local/bin/lego" ] ||
        [ ! -s "$SOURCE_CERT" ] ||
        [ ! -s "$SOURCE_KEY" ] ||
        [ ! -d "/data/user/0/$PACKAGE" ]; then
        log "renewal prerequisites are unavailable"
        return 1
    fi

    if ! chroot "$ROOTFS" /usr/local/bin/lego \
        --accept-tos \
        --path "$ACME_PATH" \
        --domains "$DOMAIN" \
        --http \
        --http.port :80 \
        --disable-cn \
        renew --days 30; then
        log "ACME renewal failed"
        return 1
    fi

    source_sum="$(sha256sum "$SOURCE_CERT" | awk '{print $1}')"
    current_sum="$(app_cert_sum)"
    if [ "$source_sum" = "$current_sum" ]; then
        log "certificate is current"
        return 0
    fi

    if ! sync_certificate; then
        log "certificate sync failed"
        return 1
    fi
    log "certificate updated; restarting camera-hub core"

    pid="$(ps -A -o PID,ARGS |
        awk '$2 == "libcamera_hub_exec.so" && $3 == "server" {print $1; exit}')"
    [ -z "$pid" ] || kill -TERM "$pid"
}

renew_edge_once() {
    if [ ! -x "$ROOTFS/usr/local/bin/camera-hub-acme-edge" ] ||
        [ ! -x "$ROOTFS/bin/su" ]; then
        log "edge renewal prerequisites are unavailable"
        return 1
    fi

    if ! chroot "$ROOTFS" /bin/su -s /bin/sh android -c \
        'PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; export PATH; exec /usr/local/bin/camera-hub-acme-edge'; then
        log "edge ACME renewal failed"
        return 1
    fi
    log "edge certificate check completed"
}

renew_all_once() {
    renewal_status=0
    renew_once || renewal_status=1
    renew_edge_once || renewal_status=1
    return "$renewal_status"
}

if [ "${1:-}" = "--once" ]; then
    renew_all_once
    exit $?
fi

(
    sleep 180
    while :; do
        renew_all_once
        sleep 43200
    done
) >> "$LOG" 2>&1 &
