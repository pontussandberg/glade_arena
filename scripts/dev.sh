#!/usr/bin/env bash
# Rebuild and rerun everything: browser client, server, and the static file server.
# Replaces any arena-server already running, so a stale one can't hold the port or speak an old
# protocol. The server runs in the foreground; Ctrl+C stops it (and the file server, if started here).
set -euo pipefail
cd "$(dirname "$0")/.."

# Cargo never deletes outdated artifacts (each build-input change writes new copies next to the
# old), so target/ grows until the disk fills. Drop what no build has written in 3 days, but only
# our own crates' (incremental caches, our libraries, and the binaries with their large .pdb debug
# info): a dependency's still-current build isn't rewritten, so deleting it by age would force
# rebuilding Bevy. Deleting one of ours that's still current costs a quick rebuild.
for dir in target/debug target/wasm32-unknown-unknown/debug; do
  [ -d "$dir" ] || continue
  find "$dir/incremental" -mindepth 1 -maxdepth 1 -mtime +3 -exec rm -rf {} + 2>/dev/null || true
  find "$dir/deps" -maxdepth 1 -type f -mtime +3 \
    \( -name 'arena_*' -o -name 'libarena_*' -o -name '*.exe' -o -name '*.pdb' \) -delete 2>/dev/null || true
done

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
