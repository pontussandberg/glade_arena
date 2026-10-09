#!/usr/bin/env bash
# Rebuild and rerun everything: browser client, server, and the static file server.
# Replaces any arena-server already running, so a stale one can't hold the port or speak an old
# protocol. The server runs in the foreground; Ctrl+C stops it (and the file server, if started here).
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/build-web.sh "$@"

# Stop the old server before building: Windows won't let cargo overwrite a running exe.
if command -v taskkill >/dev/null; then
  taskkill //IM arena-server.exe //F >/dev/null 2>&1 || true
else
  pkill -x arena-server || true
fi
cargo build -p arena-server

# serve.mjs reads files fresh on every request, so one already running can stay.
if ! curl -sf -o /dev/null http://localhost:8080/index.html; then
  node scripts/serve.mjs &
  trap 'kill $! 2>/dev/null' EXIT
fi

cargo run -p arena-server
