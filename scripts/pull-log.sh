#!/usr/bin/env bash
# Pull the live court event log off the Fly volume for local surgery.
# Usage: ./scripts/pull-log.sh [dest]   (default: ./events.live.jsonl)
# Requires: flyctl logged in, app mgetf-judge.
set -euo pipefail
APP="${FLY_APP:-mgetf-judge}"
DEST="${1:-events.live.jsonl}"
flyctl ssh sftp --app "$APP" get /data/events.jsonl "$DEST"
echo "pulled $(wc -l < "$DEST") lines -> $DEST"
