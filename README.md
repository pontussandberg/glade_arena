# Arena

Browser PvP arena brawler: top-down 3D presentation on 2D gameplay rules, with one authoritative
server, client-side prediction, and interpolation of other players.

The fight happens in a forest clearing on a 1 m tile grid: a river through the middle, crossed by
a stone bridge and two fords, with ruined walls, boulders and trees for cover. Movement is
point-and-click like LoL or OSRS: **right click** walks to a tile (pathfinding around obstacles),
**left click** auto-attacks toward the cursor (a skillshot), **Q** uses your class's ability
toward the cursor, **S** stops, **A** rings how far your shots fly (until the next key or
click). Your ability's cooldown shows on an icon at the bottom of your screen, and a minimap in
the corner shows everyone.

Before joining you pick a **class** in the lobby, where it stands on a stage to look at (drag
to turn it) next to its numbers and Q ability; then enter the arena. Each class has its own HP, speed, auto-attack (melee or
projectile) and Q ability; all of it lives in `shared/assets/classes.ron`:

| Class | Role | Auto-attack | Q |
|---|---|---|---|
| Javelinist | Hunter | Slow, heavy javelins that hit harder the farther they fly (`far_damage`) | **Spirit Spear**: a fast spectral spear, thrown instantly (no windup, no root) |
| Revenant | Duelist | Quick, short-windup blade swings | **Rift Step**: dash through enemies, cutting each one; a hit readies the blade at once |
| Frost Mage | Controller | Frostbolts: little damage and shorter range (mid range), but each hit slows (40% for 2 s) | **Frost Nova**: slam the staff down and freeze everyone within 3.5 m in place for 1.5 s (no walking, no dashing) |

Hits can carry **crowd control** (`chill` in `classes.ron`): a slow takes a share off walking
and dashing speed, a root freezes a fighter in place (it can't walk or dash, and a dash in
progress stops) but it can still attack. Slowed fighters take on a cold blue cast and frost turns
under their feet; frozen ones stand in ice.

Every attack has a short windup (`windup_ticks`): you stand still with your aim locked while a
cast bar fills under your health bar (and, for a swing, a faint telegraph shows the fan it will
cover), then it goes off. Everyone sees the windup, so attacks can be read and dodged, and kiting
takes skill. Attacking also cancels your walk: afterwards you stand still until you
right click again (a click during the windup is kept and walked to once the attack is off);
so does a dash. Abilities have their own cooldown (shown in the HUD) and are predicted like
everything else: your spear flies and your dash moves the moment you press Q, the server
decides the hits.
Killed players sit out 3 seconds, then respawn at full health.

Stack: **Bevy 0.19 + Lightyear 0.30** (pinned together), Rust everywhere. The browser client is
the same Rust code compiled to WASM, talking WebTransport to the server.

## Layout

```
shared/   protocol (replicated components, PlayerInput, messages), sim (pure gameplay rules),
          map (the tile map, A* pathfinding, line-of-movement checks), classes (+ assets/classes.ron),
          config
server/   headless authoritative server (spawning, hit decisions, lag compensation, respawns);
          tests/integration/ has the end-to-end tests (one test binary: each links all of Bevy)
client/   ClientNetPlugin (networking + prediction, headless-capable), bot (sparring AI);
          render (fighters, shots, swings, telegraphs, dash streaks, novas, frost, HUD, input), camera,
          action_bar (your passive and ability icons, cooldown, hover tooltips), stat_frame (a
          class's stats: in the lobby and bottom left in the arena), minimap,
          glade (the 3D scene and all meshes), rig (animated fighters: facing, walk cycle,
          windup and throw), lobby (character select), feedback (health and cast bars, hit flash)
client/web/  index.html for the browser build (pkg/ and digest.txt are generated)
docs/     art-direction.md: "The Glade" look (palette, light, shape rules, props, fighters)
scripts/  build-web.sh, serve.mjs
```

### How a tick works

- After connecting, a client sends `ChooseClass` once; the server spawns its player then.
- Clients send only **inputs** (`PlayerInput`: the tile they right-clicked, aim, fire held, Q
  pressed) at 64 Hz, never positions. A Q press is sent for exactly one tick.
- The server runs `shared::sim` on those inputs (pathfinding toward the clicked tile, attack
  windups and cooldowns, Q abilities and dashes, projectile flight), decides hits and damage, and
  replicates state at 20 Hz.
- Pathfinding is A* over the shared tile map, smoothed into straight lines where the way is
  clear. It runs inside the sim, so the client predicts exactly the route the server walks.
  The map uses no platform math functions (sin, atan2), so native and WASM agree to the bit.
- Walls, rocks, trees and the forest edge block movement and shots; water blocks movement only.
- The client runs the **same** `shared::sim` functions on its own player and projectiles right
  away (prediction). When server state disagrees, Lightyear rolls back and replays.
- Projectiles are **prespawned**: the client spawns its shot immediately with a hash
  (`projectile_prespawn_hash`, different for an auto-attack and a Q thrown in the same tick), and
  the server's copy is matched to it when it arrives.
- **Every client predicts every projectile**, not just its own. A shot flies on rails: its
  position is a pure function of the tick (`sim::projectile_pos`, from its origin, direction and
  spawn tick). So others' shots, which reach you a round trip late, are still drawn where they
  really are, on the same clock as your own player, and what you see dodge is what the server
  judges. A late spear shoots out of its thrower's hand fast and catches up within 0.2 s.
- `AttackState` (cooldown, windup) and `AbilityState` (Q cooldown, dash) are predicted for your
  own player, so your windup, throw and dash start the moment you click or press Q. For others
  they're interpolated on the same delayed timeline as their positions, so their cast bar,
  telegraph and dash line up with where you see them; except that a throw shows the moment its
  spear appears (`SeenThrows`), which can be a little ahead of the thrower's body.
- Other players are **interpolated** between server snapshots.
- `Health` is server-only: replicated, never predicted. Melee, dash and nova damage are decided
  by the server too (and a Rift Step readying the blade: your client is corrected by a rollback);
  your own swing (`LastSwing`) is predicted so it shows instantly.
- So is `Chilled` (slows and roots). It holds tick spans, so when one lands on you a round trip
  late, the rollback it causes replays your movement with it from exactly the tick it began.
- **Lag compensation:** you see others slightly in the past, so the server keeps a short
  position history and judges your swings, dashes and novas against where *you* saw the target (up to
  ~250 ms back, using the interpolation delay your inputs carry). Projectiles are judged in the
  present instead ("favor the target"): they can be dodged, and the target sees them exactly as
  judged; the shooter may sometimes see a shot pass through someone who had already stepped
  aside.
- `classes.ron` is compiled into both sides, and its hash is part of the protocol id: a client
  built with different class numbers can't connect.

Rule of thumb: gameplay rules go in `shared/src/sim.rs` as pure functions. Server-only
decisions (damage, death) stay in `server/`.

## Running

Prereqs: Rust (stable, MSVC on Windows), `rustup target add wasm32-unknown-unknown`, and
`cargo install wasm-bindgen-cli --version <version of wasm-bindgen in Cargo.lock>`.

Build settings that keep Bevy builds fast and within memory: Windows links with Rust's bundled
`rust-lld` and cargo runs at most 12 jobs (`.cargo/config.toml`); dependencies build without
debug info and our own crates with line tables only (`Cargo.toml`). `scripts/dev.sh` also
deletes our own crates' build artifacts (not dependencies') that no build has written in 3 days:
cargo never removes outdated ones itself.

```sh
# all at once: rebuild the browser client, replace any running server, serve on :8080
./scripts/dev.sh

# tests (~10s to run, plus the build): sim unit tests, plus real server + headless bot clients
# over WebTransport (server/tests/integration/):
#   map/sim       map symmetry, pathfinding, every spawn point reaching every other; windups,
#                 cooldowns, far_damage, Q throws, dashes (full distance, never into a wall,
#                 not mid-swing), slows, roots (stopping walks and dashes), novas
#   classes       the class file parses and every class has sane numbers
#   netcode.rs    click-to-move prediction, reconciliation, interpolation, unreachable clicks,
#                 predicted windup, prespawned shot, server-decided hit, rollback on server
#                 correction (one bot at 60ms + 2% loss)
#   combat.rs     melee hits where the attacker saw the target (lag compensation), misses out
#                 of reach, death and respawn
#   abilities.rs  spirit spear predicted at once (no windup) and hitting; rift step cutting
#                 through a target once and readying the blade, cutting where it ends but not
#                 who it leaves behind; frost nova freezing a revenant (no walk, no dash, its
#                 own client rolled back to the spot), then letting it go; frostbolts slowing
#   inputs.rs     4 fps client releasing fire; frozen client stops moving server-side;
#                 attacking cancels the walk, a click during the windup is kept
cargo test

# server (run from the repo root; writes client/web/digest.txt for the browser; see Logs below)
cargo run -p arena-server

# browser client
./scripts/build-web.sh          # -> client/web/pkg
node scripts/serve.mjs          # -> http://localhost:8080 (open twice to play; you are blue)

# native client (optional, handy for debugging): [client id] [class], e.g. `11 javelinist`;
# without a class it opens the lobby. ARENA_SERVER=ip:port picks another server.
cargo run -p arena-client

# a sparring partner: a native client played by a simple bot
ARENA_BOT=1 cargo run -p arena-client -- 99 revenant

# to watch two bots fight, start a second one, then in your client press V and Tab to them
ARENA_BOT=1 cargo run -p arena-client -- 98 javelinist
```

The server makes a new self-signed certificate (valid 14 days) on every start. The page reads
its hash from `digest.txt`, so after restarting the server, reload the page. Chrome and Firefox
support WebTransport with certificate hashes.

Controls: in the lobby, pick a class with a click or its number key and enter with the button or
Enter; in the arena, right click to move, S to stop, left
click to attack toward the cursor, Q for your ability toward the cursor. The camera stays locked on
you; the mouse wheel zooms.

V toggles a free, WoW-style camera that follows behind you: WASD walks relative to it, holding
the right mouse button and dragging (or the arrow keys) turns it, the wheel (or + / -) zooms,
down to arm's length. There the right button only turns the camera: click to move is off. Tab
moves it on to the next fighter, to watch them play (and after the last, back to
you). V again goes back. The keys for the current camera are listed
top left, under the ping (with the free camera also jitter and rollbacks).

Two tabs work: a hidden tab keeps simulating and networking without rendering (see
`client/src/hidden_tab.rs`, which works around Bevy 0.19 ignoring Lightyear's keepalive). If a
client does freeze, the server stops its player after ~8 ticks (`neutralize_stale_inputs`) and
drops it after 3s; the HUD then says DISCONNECTED and a page reload rejoins.

## Logs

The server logs to stderr. At info: start, connects and disconnects (with the reason), joins,
kills, a `status` line every minute (players, each client's ping, jitter and input gaps), and
warnings when the server hitches or a client's inputs stop arriving for over a second. Debug
adds every hit and respawn: `RUST_LOG=info,arena_server=debug,lightyear=warn`.
`ARENA_LOG_FORMAT=json` writes one JSON object per line, for a log collector. Panics are logged
too (`server/src/logging.rs`).

## Deploying

One Ubuntu VPS (e.g. Hetzner Cloud; 4 GB of RAM, since the server is built there) with a domain
pointing at it. Caddy serves the page over HTTPS and gets the certificate from Let's Encrypt; the
game server's WebTransport uses the same certificate (copied for it whenever Caddy renews it; the
server then restarts itself to load it), so the browser needs no `digest.txt` and connects to
`https://<domain>:5888`. The server is a systemd service; its logs go to the journal.

```sh
# .env.local (not committed):
#   DEPLOY_HOST=root@<server ip>
#   DEPLOY_DOMAIN=arena.example.com     (its A record pointing at the server)
bash deploy/deploy.sh setup   # once: Rust, Caddy, firewall (80/443 TCP, 5888 UDP), the service
bash deploy/deploy.sh app     # build the web client here, the server there (from the last
                              # commit; refuses with uncommitted changes), restart
bash deploy/deploy.sh logs    # follow the server's logs
```

If your provider has its own firewall (Hetzner Cloud Firewall), open the same ports there. A
deploy restarts the server, which disconnects everyone playing.

## Not done yet (see the plan)

- More classes, and more abilities (W/E/R); a balance pass (in bot duels the Revenant's
  dash-and-swing tends to beat the Javelinist; the Frost Mage is new and untuned)
- Network debug overlay (lag sliders, server ghost), room codes, switching class without rejoining
- Visual smoothing: frame interpolation between ticks and correction blending after rollbacks
- Accounts service issuing netcode connect tokens (currently a shared zero dev key)
- WASM size: 44 MB raw / ~10 MB gzipped; add wasm-opt and brotli, and a loading bar
- Lightyear 0.30 drops input messages more than 64 ticks (1s) from the server tick; fine for real
  clients, but worth knowing if you ever add large input delay
