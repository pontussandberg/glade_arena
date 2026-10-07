# Arena

Browser PvP arena brawler: top-down 3D presentation on 2D gameplay rules, with one authoritative
server, client-side prediction, and interpolation of other players.

Stack: **Bevy 0.19 + Lightyear 0.30** (pinned together), Rust everywhere. The browser client is
the same Rust code compiled to WASM, talking WebTransport to the server.

## Layout

```
shared/   protocol (replicated components, PlayerInput) + sim (pure gameplay rules) + config
server/   headless authoritative server; tests/netcode.rs is the end-to-end netcode test
client/   ClientNetPlugin (networking + prediction, headless-capable) and render (3D placeholder)
client/web/  index.html for the browser build (pkg/ and digest.txt are generated)
scripts/  build-web.sh, serve.mjs
```

### How a tick works

- Clients send only **inputs** (`PlayerInput`: movement, aim, fire) at 64 Hz, never positions.
- The server runs `shared::sim` on those inputs (movement, firing with cooldown, projectile
  flight), decides hits and damage, and replicates state at 20 Hz.
- The client runs the **same** `shared::sim` functions on its own player and projectiles right
  away (prediction). When server state disagrees, Lightyear rolls back and replays.
- Projectiles are **prespawned**: the client spawns its shot immediately with a hash
  (`projectile_prespawn_hash`), and the server's copy is matched to it when it arrives.
- Other players and their projectiles are **interpolated** between server snapshots.
- `Health` is server-only: replicated, never predicted.

Rule of thumb: gameplay rules go in `shared/src/sim.rs` as pure functions. Server-only
decisions (damage, death) stay in `server/`.

## Running

Prereqs: Rust (stable, MSVC on Windows), `rustup target add wasm32-unknown-unknown`, and
`cargo install wasm-bindgen-cli --version <version of wasm-bindgen in Cargo.lock>`.

```sh
# tests (~30s): sim unit tests, plus real server + headless bot clients over WebTransport:
#   netcode.rs  prediction, reconciliation, interpolation, speed cap, prespawned shot,
#               server-decided hit, rollback on server correction (one bot at 60ms + 2% loss)
#   inputs.rs   4 fps client releasing fire; frozen client stops moving server-side
cargo test

# server (run from the repo root; writes client/web/digest.txt for the browser)
cargo run -p arena-server

# browser client
./scripts/build-web.sh          # -> client/web/pkg
node scripts/serve.mjs          # -> http://localhost:8080 (open twice to play; you are blue)

# native client (optional, handy for debugging)
cargo run -p arena-client
```

The server makes a new self-signed certificate (valid 14 days) on every start. The page reads
its hash from `digest.txt`, so after restarting the server, reload the page. Chrome and Firefox
support WebTransport with certificate hashes.

Controls: WASD to move, mouse to aim, left click or space to fire.

Two tabs work: a hidden tab keeps simulating and networking without rendering (see
`client/src/hidden_tab.rs`, which works around Bevy 0.19 ignoring Lightyear's keepalive). If a
client does freeze, the server stops its player after ~8 ticks (`neutralize_stale_inputs`) and
drops it after 3s; the HUD then says DISCONNECTED and a page reload rejoins.

## Not done yet (see the plan)

- Dash ability, network debug overlay (lag sliders, server ghost), room codes
- Visual smoothing: frame interpolation between ticks and correction blending after rollbacks
- Lag compensation for hits (projectiles are currently judged against the server's present)
- Accounts service issuing netcode connect tokens (currently a shared zero dev key)
- WASM size: 44 MB raw / ~10 MB gzipped; add wasm-opt and brotli, and a loading bar
- Lightyear 0.30 drops input messages more than 64 ticks (1s) from the server tick; fine for real
  clients, but worth knowing if you ever add large input delay
