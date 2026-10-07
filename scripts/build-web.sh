#!/usr/bin/env bash
# Build the browser client into client/web/pkg. Requires: rustup target wasm32-unknown-unknown,
# and wasm-bindgen-cli at the same version as the wasm-bindgen crate in Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${1:-wasm-release}"
cargo build -p arena-client --target wasm32-unknown-unknown --profile "$profile"
wasm-bindgen --target web --no-typescript --out-dir client/web/pkg \
  "target/wasm32-unknown-unknown/$profile/arena-client.wasm"
ls -la client/web/pkg
