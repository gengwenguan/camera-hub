#!/bin/sh
set -eu

TINY_ROOT="${CAMERA_HUB_ANDROID_ROOT:-/proc/1/root}"
TINYMIX="$TINY_ROOT/system/bin/tinymix"

[ -x "$TINYMIX" ] || {
    echo "Android tinymix not found: $TINYMIX" >&2
    exit 1
}

mix() {
    chroot "$TINY_ROOT" /system/bin/tinymix "$1" "$2" >/dev/null
}

setup_capture() {
    # Android mixer_paths_tasha.xml: audio-record + main-mic (ADC3).
    mix "MultiMedia1 Mixer SLIM_0_TX" 1
    mix "AIF1_CAP Mixer SLIM TX5" 1
    mix "SLIM_0_TX Channels" One
    mix "SLIM TX5 MUX" DEC5
    mix "ADC MUX5" AMIC
    mix "AMIC MUX5" ADC3
    mix "IIR0 INP0 MUX" DEC5
    mix "ADC3 Volume" 12
}

setup_playback() {
    # Android mixer_paths_tasha.xml: deep-buffer-playback + speaker.
    mix "QUAT_MI2S_RX Audio Mixer MultiMedia1" 1
    mix "TFA Profile" music_dual
}

reset_capture() {
    mix "MultiMedia1 Mixer SLIM_0_TX" 0
    mix "AIF1_CAP Mixer SLIM TX5" 0
    mix "SLIM TX5 MUX" ZERO
    mix "AMIC MUX5" ZERO
    mix "IIR0 INP0 MUX" ZERO
}

reset_playback() {
    mix "QUAT_MI2S_RX Audio Mixer MultiMedia1" 0
}

case "${1:-setup}" in
    setup)
        setup_capture
        setup_playback
        ;;
    setup-capture)
        setup_capture
        ;;
    setup-playback)
        setup_playback
        ;;
    reset)
        reset_capture
        reset_playback
        ;;
    *)
        echo "usage: $0 [setup|setup-capture|setup-playback|reset]" >&2
        exit 2
        ;;
esac
