#!/bin/bash
# NDFR Media Server. Relays the information as the user using the pipe created by the daemon.
# Expects the ndfr-media-helper to be present.

PIPE_PATH="/tmp/ndfr-media.pipe"
MEDIA_HELPER="/usr/bin/ndfr-media-helper"

if [ ! -x "$MEDIA_HELPER" ]; then
    echo "Error: Media helper not found or not executable at $MEDIA_HELPER" >&2
    exit 1
fi

if [ ! -p "$PIPE_PATH" ]; then
    echo "Waiting for daemon pipe at $PIPE_PATH..." >&2
    sleep 3
    if [ ! -p "$PIPE_PATH" ]; then
        echo "Error: ndfr daemon not running or pipe not found." >&2
        exit 1
    fi
fi

echo "Media agent started. Streaming updates to daemon." >&2

if [ -n "$SUDO_USER" ]; then
    HELPER_PATH=$(realpath "$MEDIA_HELPER")
    sudo -u "$SUDO_USER" "$HELPER_PATH" listen > "$PIPE_PATH"
else
    "$MEDIA_HELPER" listen > "$PIPE_PATH"
fi
