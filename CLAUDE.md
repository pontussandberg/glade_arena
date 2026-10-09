# Working in this repo

## After a change: rebuild and run, don't test

- To check a change, rebuild and run the game locally with `./scripts/dev.sh dev` (run it in the
  background: it builds the wasm client, then the server, and serves http://localhost:8080). It
  replaces any arena-server already running. Tell the user when the server is up, and to
  hard-reload the page (Ctrl+Shift+R).
- Use the `dev` profile locally: it compiles much faster than the default `wasm-release` (thin
  LTO, one codegen unit), at the cost of a bigger, slower wasm. `wasm-release` is for deploys.
- Before building, check for stale `cargo`/`rustc` processes holding the build lock
  (`tasklist | grep -iE "cargo|rustc"`). Kill them only if no build of yours is running.
- Type-checking (`cargo check -p <crate>`) and a single targeted test are fine when they help.
- **Never run the full test suite (`cargo test`) on your own.** It is slow and heavy. Ask the user
  first, or wait for them to run `/test`.

## Keep the disk clean

The disk has filled up before (builds failing with os error 112). Only these build folders are
needed: `target/debug`, `target/wasm32-unknown-unknown`, `target/wasm-release` (and `target/tmp`,
which cargo makes itself).

- Don't build into extra target dirs (`CARGO_TARGET_DIR=target/<something>`, `target-test/`).
- After rebuilding and starting the dev server, delete any other build folders: everything in
  `target/` except those (and `CACHEDIR.TAG`), and `target-test/` at the root.
- If the disk is still nearly full, ask before running `cargo clean` (the next build is a full
  rebuild).

## Tests and the running server

`cargo test` rebuilds `arena-server.exe`, which Windows can't overwrite while the dev server is
running ("Access is denied"). Stop the dev server first, and restart it afterwards.
