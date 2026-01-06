#!/usr/bin/env bash
set -euo pipefail

FIFO="$(mktemp -u)"
mkfifo "$FIFO"

cleanup() {
  rm -f "$FIFO"
  kill 0 2>/dev/null
}
trap cleanup EXIT

# gpio-ir device from dtoverlay (configured in /boot/firmware/config.txt)
IR_EVENTS="/dev/input/by-path/platform-ir-receiver@11-event"
NES_EVENTS="/dev/input/by-path/platform-NES_pad-event"

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
  mpv_ipc '{"command":["show-text","Loading...",1000]}'
}

start_mpv() {
    mpv --input-ipc-server=/tmp/mpv-frigate.sock \
        --no-cache \
        --profile=low-latency \
        --rtsp-transport=tcp \
        --video-unscaled=no --keepaspect=no \
        --osd-color='#00FF00' --osd-font-size=48 --osd-border-size=4 --osd-align-y=bottom \
        "${URL_1}" &
    watch_pid $!
}

start_evtest() {
  local dev="$1"
  evtest --grab "$dev" 1>"$FIFO" &
  watch_pid $!
}

watch_pid() {
    local parent_pid=$$     # PID of the main script shell
    local child_pid=$1

    (
        # Loop while the child is alive
        while kill -0 "$child_pid" 2>/dev/null; do
            sleep 1
        done

        # Child is gone -> kill the parent (this script)
        kill -TERM "$parent_pid" 2>/dev/null || true
    ) >/dev/null 2>&1 &
}

# Start multiple evtest processes that output to our FIFO
start_evtest "$IR_EVENTS"
start_evtest "$NES_EVENTS"

# Start background MPV process
start_mpv

# Ensure mpv socket exists before reading events
until [[ -S "$SOCK" ]]; do sleep 0.05; done

# Read key press events (value 1) and switch streams
# Event: time 1765981628.956459, type 1 (EV_KEY), code 515 (KEY_NUMERIC_3), value 1
while IFS= read -r line; do
  case "$line" in
    *"EV_KEY"*"(KEY_NUMERIC_1)"*"value 1"*|\
    *"EV_KEY"*"(BTN_START)"*"value 1"*)
        switch_to "$URL_1" ;;
    *"EV_KEY"*"(KEY_NUMERIC_2)"*"value 1"*|\
    *"EV_KEY"*"(BTN_EAST)"*"value 1"*)
        switch_to "$URL_2" ;;
    *"EV_KEY"*"(KEY_NUMERIC_3)"*"value 1"*|\
    *"EV_KEY"*"(BTN_SOUTH)"*"value 1"*)
        switch_to "$URL_3" ;;
  esac
done <"$FIFO"
