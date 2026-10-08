# Arena

Browser PvP arena brawler: top-down 3D presentation on 2D gameplay rules, with one authoritative
server, client-side prediction, and interpolation of other players.

The fight happens in a forest clearing on a 1 m tile grid: a river through the middle, crossed by
a stone bridge and two fords, with ruined walls, boulders and trees for cover. Movement is
point-and-click like LoL or OSRS: **right click** walks to a tile (pathfinding around obstacles),
**left click** auto-attacks toward the cursor (a skillshot).

Before joining you pick a **class**. Each has its own HP, speed and auto-attack, melee or
projectile; all of it lives in `shared/assets/classes.ron`:

| Class | Role | Auto-attack |
|---|---|---|
| Shade | Assassin | Fast dagger strikes, short reach |
| Warden | Brawler | Wide, heavy hammer swings; lots of HP |
| Javelinist | Hunter | Slow, heavy javelins that hit harder the farther they fly (`far_damage`) |
| Ranger | Sniper | Fast, thin arrows, long range |

Every attack has a short windup (`windup_ticks`): you stand still with your aim locked while a
cast bar fills under your health bar and a faint telegraph shows what it will cover (the swing's
fan, or the shot's lane), then it goes off. Everyone sees it, so attacks can be read and dodged,
and kiting takes skill. Attacking also cancels your walk: afterwards you stand still until you
right click again (a click during the windup is kept and walked to once the attack is off).
Killed players sit out 3 seconds, then respawn at full health.

Stack: **Bevy 0.19 + Lightyear 0.30** (pinned together), Rust everywhere. The browser client is
the same Rust code compiled to WASM, talking WebTransport to the server.

## Layout

```
shared/   protocol (replicated components, PlayerInput, messages), sim (pure gameplay rules),
          map (the tile map, A* pathfinding, line-of-movement checks), classes (+ assets/classes.ron),
          config
server/   headless authoritative server (spawning, hit decisions, lag compensation, respawns);
          tests/ has the end-to-end tests
client/   ClientNetPlugin (networking + prediction, headless-capable), bot (sparring AI);
          render, camera, glade (the 3D scene), join (class picker), feedback (health bars, hits)
client/web/  index.html for the browser build (pkg/ and digest.txt are generated)
docs/     art-direction.md: "The Glade" look (palette, light, shape rules, props)
scripts/  build-web.sh, serve.mjs
```

### How a tick works

- After connecting, a client sends `ChooseClass` once; the server spawns its player then.
- Clients send only **inputs** (`PlayerInput`: the tile they right-clicked, aim, fire) at 64 Hz,
  never positions.
- The server runs `shared::sim` on those inputs (pathfinding toward the clicked tile, firing with
  cooldown, projectile flight), decides hits and damage, and replicates state at 20 Hz.
- Pathfinding is A* over the shared tile map, smoothed into straight lines where the way is
  clear. It runs inside the sim, so the client predicts exactly the route the server walks.
  The map uses no platform math functions (sin, atan2), so native and WASM agree to the bit.
- Walls, rocks, trees and the forest edge block movement and shots; water blocks movement only.
- The client runs the **same** `shared::sim` functions on its own player and projectiles right
  away (prediction). When server state disagrees, Lightyear rolls back and replays.
- Projectiles are **prespawned**: the client spawns its shot immediately with a hash
  (`projectile_prespawn_hash`), and the server's copy is matched to it when it arrives.
- Other players and their projectiles are **interpolated** between server snapshots.
- `Health` is server-only: replicated, never predicted. Melee damage is decided by the server
  too; your own swing (`LastSwing`) is predicted so it shows instantly.
- **Lag compensation:** you see others slightly in the past, so the server keeps a short
  position history and judges your hits against where *you* saw the target (up to ~250 ms
  back, using the interpolation delay your inputs carry).
- `classes.ron` is compiled into both sides, and its hash is part of the protocol id: a client
  built with different class numbers can't connect.

Rule of thumb: gameplay rules go in `shared/src/sim.rs` as pure functions. Server-only
decisions (damage, death) stay in `server/`.

## Running

Prereqs: Rust (stable, MSVC on Windows), `rustup target add wasm32-unknown-unknown`, and
`cargo install wasm-bindgen-cli --version <version of wasm-bindgen in Cargo.lock>`.

```sh
# tests (~30s): sim unit tests, plus real server + headless bot clients over WebTransport:
#   map/sim     map symmetry, pathfinding, every spawn point reaching every other
#   classes     the class file parses and every class has sane numbers
#   netcode.rs  click-to-move prediction, reconciliation, interpolation, unreachable clicks,
#               prespawned shot, server-decided hit, rollback on server correction
#               (one bot at 60ms + 2% loss)
#   combat.rs   melee hits where the attacker saw the target (lag compensation), misses out
#               of reach, death and respawn
#   inputs.rs   4 fps client releasing fire; frozen client stops moving server-side
cargo test

# server (run from the repo root; writes client/web/digest.txt for the browser)
cargo run -p arena-server

# browser client
./scripts/build-web.sh          # -> client/web/pkg
node scripts/serve.mjs          # -> http://localhost:8080 (open twice to play; you are blue)

# native client (optional, handy for debugging): [client id] [class], e.g. `11 shade`;
# without a class it shows the join screen. ARENA_SERVER=ip:port picks another server.
cargo run -p arena-client

# a sparring partner: a native client played by a simple bot
ARENA_BOT=1 cargo run -p arena-client -- 99 warden
```

The server makes a new self-signed certificate (valid 14 days) on every start. The page reads
its hash from `digest.txt`, so after restarting the server, reload the page. Chrome and Firefox
support WebTransport with certificate hashes.

Controls: pick a class with a click or keys 1-4, right click to move, left click to attack
toward the cursor, hold Space to lock the camera on yourself, push the mouse to a screen edge or
use the arrow keys to pan, mouse wheel to zoom.

Two tabs work: a hidden tab keeps simulating and networking without rendering (see
`client/src/hidden_tab.rs`, which works around Bevy 0.19 ignoring Lightyear's keepalive). If a
client does freeze, the server stops its player after ~8 ticks (`neutralize_stale_inputs`) and
drops it after 3s; the HUD then says DISCONNECTED and a page reload rejoins.

## Not done yet (see the plan)

- One ability per class on Q (planned: Shade blink, Warden charge, Javelinist to be decided, Ranger roll)
- Network debug overlay (lag sliders, server ghost), room codes, switching class without rejoining
- Visual smoothing: frame interpolation between ticks and correction blending after rollbacks
- Accounts service issuing netcode connect tokens (currently a shared zero dev key)
- WASM size: 44 MB raw / ~10 MB gzipped; add wasm-opt and brotli, and a loading bar
- Lightyear 0.30 drops input messages more than 64 ticks (1s) from the server tick; fine for real
  clients, but worth knowing if you ever add large input delay
