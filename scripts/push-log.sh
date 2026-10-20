#!/usr/bin/env bash
# Push a locally-edited event log back to the Fly volume and restart the court.
# Usage: ./scripts/push-log.sh [src]   (default: ./events.live.jsonl)
# The app replays the log on boot; back up first (pull-log.sh already saved yours).
set -euo pipefail
APP="${FLY_APP:-mgetf-judge}"
SRC="${1:-events.live.jsonl}"
flyctl ssh sftp --app "$APP" put "$SRC" /data/events.jsonl
echo "pushed $SRC -> /data/events.jsonl"
flyctl machine restart --app "$APP" --select 2>&1 | tail -n 3 || flyctl apps restart --app "$APP" 2>&1 | tail -n 3
