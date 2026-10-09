---
name: test
description: Run the full test suite (slow). Only when the user asks for it.
disable-model-invocation: true
argument-hint: "[test filter, e.g. -p arena-server --test abilities]"
---

Run the arena test suite.

1. If the dev server is running, stop it first (`taskkill //IM arena-server.exe //F` from Git
   Bash): tests rebuild `arena-server.exe`, which Windows can't overwrite while it runs.
2. Run `cargo test $ARGUMENTS` from the repo root (the whole suite if no arguments; it takes a
   few minutes with the build). Run it in the background and filter the output to
   `test result|FAILED|panicked|^error`.
3. Report: how many passed and failed per test binary, and for each failure its message and
   where it panicked. Don't fix anything unless the user asks.
4. If the dev server was running before, start it again with `./scripts/dev.sh` (in the
   background).
5. Delete stray build folders as `CLAUDE.md` says (keep only `target/debug`, `target/tmp`,
   `target/wasm32-unknown-unknown`, `target/wasm-release`).
