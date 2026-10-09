#!/usr/bin/env bash
# Build the browser client into client/web/pkg. Requires: rustup target wasm32-unknown-unknown,
# and wasm-bindgen-cli at the same version as the wasm-bindgen crate in Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${1:-wasm-release}"
# Cargo builds its built-in profiles into differently named directories.
case "$profile" in
  dev | test) dir=debug ;;
  bench) dir=release ;;
  *) dir="$profile" ;;
esac
cargo build -p arena-client --target wasm32-unknown-unknown --profile "$profile"
wasm-bindgen --target web --no-typescript --out-dir client/web/pkg \
  "target/wasm32-unknown-unknown/$dir/arena-client.wasm"
ls -la client/web/pkg
