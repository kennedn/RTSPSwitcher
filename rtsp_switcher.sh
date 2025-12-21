#!/usr/bin/env bash
set -euo pipefail

# gpio-ir device from dtoverlay (configured in /boot/firmware/config.txt)
DEV="/dev/input/by-path/platform-ir-receiver@11-event"

SOCK="/tmp/mpv-frigate.sock"

URL_1="rtsp://frigate.kennedn.com:8554/front_garden_sub"
URL_2="rtsp://frigate.kennedn.com:8554/back_garden_sub"
URL_3="rtsp://frigate.kennedn.com:8554/livingroom_sub"

mpv_ipc() {
  printf '%s\n' "$1" | socat - "UNIX-CONNECT:$SOCK" >/dev/null
}

# Send loadfile command to MPV with IPC
switch_to() {
  local url="$1"
  mpv_ipc "$(printf '{"command":["loadfile","%s","replace"]}' "$url")"
}

start_mpv() {
    mpv --input-ipc-server=/tmp/mpv-frigate.sock \
        --no-cache \
        --profile=low-latency \
        --rtsp-transport=tcp \
        "${URL_1}" &
    MPV_PID=$!
}

trap 'kill "$MPV_PID" 2>/dev/null' EXIT

# Start background MPV process
start_mpv

# Ensure mpv socket exists before reading events
until [[ -S "$SOCK" ]]; do sleep 0.05; done

# Read key press events (value 1) and switch streams
# Event: time 1765981628.956459, type 1 (EV_KEY), code 515 (KEY_NUMERIC_3), value 1
evtest --grab "$DEV" 2>/dev/null | while IFS= read -r line; do
  case "$line" in
    *"EV_KEY"*"(KEY_NUMERIC_1)"*"value 1"*) switch_to "$URL_1" ;;
    *"EV_KEY"*"(KEY_NUMERIC_2)"*"value 1"*) switch_to "$URL_2" ;;
    *"EV_KEY"*"(KEY_NUMERIC_3)"*"value 1"*) switch_to "$URL_3" ;;
  esac
done
