# Arena

Browser PvP arena brawler: top-down 3D presentation on 2D gameplay rules, with one authoritative
server and League-style netcode: clients send commands, nothing is predicted, and other players
are interpolated.

The fight happens on one of three maps on a 1 m tile grid, picked by the room's leader (or
switched in practice): River Crossing, a forest clearing with a river through the middle crossed
by a stone bridge and two fords; Sunken Keep, a moated keep with marshes that slow you; and Riven
Gorge, a chasm that shots fly over but no one walks, with rope bridges and waygates that jump you
to the far side. Ruined walls, boulders and trees give cover. Movement is
point-and-click like LoL or OSRS: **right click** walks to a tile (pathfinding around obstacles),
**left click** auto-attacks toward the cursor (a skillshot), **Q** uses your class's ability
toward the cursor, **S** stops, **A** rings how far your shots fly (until the next key or
click). Your ability's cooldown shows on an icon at the bottom of your screen, and a minimap in
the corner shows everyone.

Before joining you pick a **class** in the lobby, where it stands on a stage to look at (drag
to turn it) next to its numbers and abilities; then enter the arena. Each class has its own HP, speed, auto-attack (melee or
projectile) and abilities (up to four, on Q, W, E and R; each class has one on Q so far); all of
it lives in its own file in `shared/assets/classes/`, next to `common.ron` for what every class
shares (field reference in the README there):

| Class | Role | Auto-attack | Q |
|---|---|---|---|
| Jav | Marksman | Slow, heavy javelins that hit harder the farther they fly (`far_scale`) | **Spirit Spear**: a fast spectral spear, thrown instantly (no windup, no root) |
| Revenant | Duelist | A heavy overhead blow of its dark glaive, the blade coming down flat on the ground from 1.6 m to 3.08 m ahead, tearing it open for 3s (inside the tear the revenant moves 30% faster); each hit empowers the glaive for the next swing if it follows within 3 s: a lane 50% further and wider, from 2 m to 4.62 m, 0.75 m wide (an empowered hit renews it), lost on a miss (`follow_up`, the **Follow Through** passive) | **Rift Step**: dash toward the cursor, 1 m to 6 m, damaging each enemy you pass through |
| Frost Mage | Controller | Frostbolts: little damage and shorter range (mid range), but each hit slows (40% for 2s) | **Frost Nova**: slam the staff down and freeze everyone within 3.5 m in place for 1.5s (no walking, no dashing) |

Hits can carry **crowd control** (`chill` in the class files): a slow takes a share off walking
and dashing speed (a dash still goes its full distance, just slower; a haste speeds it up the
same way), a root freezes a fighter in place (it can't walk or dash, and a dash in
progress stops) but it can still attack. Slowed fighters take on a cold blue cast and frost turns
under their feet; frozen ones stand in ice.

Auto-attacks work as in League. Each click is one attack (holding the button doesn't attack
again), and the next may start once its attack timer (`attack_ticks`) has run out; once the attack
has gone off, the cast bar under your health bar fills until then. A click before then is dropped,
not queued. The attack goes
off part way through that time, after a windup (`windup`, a part of the attack per class): the
spear leaving the hand, the blade reaching its target. You stand still with your aim locked
during the windup, and an order to move (a right click, S, walking keys) cancels it: no attack,
and you may attack again at once. After it has gone off, the rest is backswing: move whenever you
like, cutting the animation short, while the timer runs on. Nothing on screen shows when the
attack has gone off but your fighter itself, so learning each class's release point is the skill
check (attack, then step, then attack again). Attack speed (only the Revenant's follow-up for now)
shortens the timer and the windup alike, and plays the animation faster. Everyone sees the windup in
the fighter's body, so attacks can be read and dodged. Starting an attack cancels your walk: afterwards you stand
still until you order a move again; so does a dash. A dash may be used mid-windup: it moves you,
and the attack still goes off when the windup's over, from wherever the dash took you (stab, then
dash into place for it to land).
Abilities have their own cooldown (shown in the HUD). Like everything else they happen on the
server: your spear flies and your dash moves once the server has your Q, about a round trip
after you press it.
Killed players sit out 3 seconds, then respawn at full health.

Stack: **Bevy 0.19 + Lightyear 0.30** (pinned together), Rust everywhere. The browser client is
the same Rust code compiled to WASM, talking WebTransport to the server.

## Layout

```
shared/   protocol (replicated components, PlayerInput, messages), sim (pure gameplay rules),
          map (the tile map, A* pathfinding, line-of-movement checks), classes (+ assets/classes/),
          abilities (one module per kind: throw, dash, nova),
          config
server/   headless authoritative server (commands, spawning, hit decisions, respawns);
          tests/integration/ has the end-to-end tests (one test binary: each links all of Bevy)
client/   ClientNetPlugin (networking + commands, headless-capable), bot (sparring AI);
          render (fighters, shots, novas, frost, HUD, input), swish (a blade's wind), camera,
          action_bar (your passive and ability icons, cooldown, hover tooltips), stat_frame (a
          class's stats: in the lobby and bottom left in the arena), minimap,
          arena (the 3D scene and all meshes), rig (animated fighters: facing, walk cycle,
          windup and throw), lobby (character select), feedback (health and cast bars, hit flash)
client/web/  index.html for the browser build (pkg/ and digest.txt are generated)
docs/     art-direction.md: the look (palette, light, shape rules, props, fighters)
scripts/  build-web.sh, serve.mjs
```

### How a tick works

- After connecting, a client sends `ChooseClass` once; the server spawns its player then.
- Clients send only **commands** (`PlayerCommand`: walk to the point they right-clicked, stop,
  walk with the keys, attack held toward an aim, cast Q), League style, never positions. They're
  sent when something changes (a click goes out right away; the aim while attacking at most once
  a tick) on a reliable channel, and the server applies each one the moment it arrives, to the
  player's intent (`PlayerInput`), which the sim runs on every tick. The server drops a
  destination by itself when you arrive, attack or dash, and a fighter carries on with its last
  order if its client goes quiet.
- The server runs `shared::sim` on those intents (pathfinding toward the clicked tile, attack
  windups and cooldowns, Q abilities and dashes, projectile flight) at 32 Hz, decides hits and
  damage, and replicates state every tick.
- Pathfinding is A* over the shared tile map, smoothed into straight lines where the way is
  clear. The map uses no platform math functions (sin, atan2), so native and WASM agree to the
  bit (the client picks the clicked point, `walk_target`, that the server walks to).
- Walls, rocks, trees and the forest edge block movement and shots; water blocks movement only.
- **Nothing is predicted.** Your own fighter is shown as the server last said it was (not
  interpolated, for the least delay), so a click shows about a round trip later: ~75-100 ms on a
  60 ms link, not noticeable below ~40 ms ping. The destination marker shows at once.
- Other players are **interpolated** between server updates, a little in the past. Their
  `AttackState`, `Cooldowns` and `Dashing` are shown on that same timeline, so a cast bar and a dash line
  up with where you see them; except that a throw shows the moment its spear appears
  (`SeenThrows`), which can be a little ahead of the thrower's body.
- **Projectiles** fly on rails: a shot's position is a pure function of the tick
  (`sim::projectile_pos`, from its origin, direction and spawn tick), so clients draw them from
  that alone, on your own fighter's clock: a shot coming at you is drawn where it is relative to
  you.
- **Every hit is judged where everyone is on the server** at that tick (no lag compensation, as
  in League): melee, dashes, novas and projectiles alike. What you dodge on the server, you
  dodged; an attacker at high ping may see a swing that looked in reach miss.
- The class files (`shared/assets/classes/`) are compiled into both sides, and their hash is
  part of the protocol id: a client built with different class numbers can't connect.

Rule of thumb: gameplay rules go in `shared/src/sim/` as pure functions. Server-only
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
#   map/sim       map symmetry, pathfinding, every spawn point reaching every other; attack
#                 timers, windups, cancels, dropped clicks, cooldowns, far_scale, Q throws, dashes (full distance, never into a wall,
#                 not mid-swing), slows, roots (stopping walks and dashes), novas
#   classes       the class file parses and every class has sane numbers
#   netcode.rs    a command showing about a round trip later, own view and interpolation
#                 matching the server, unreachable clicks, server corrections, windup and
#                 shot, server-decided hit (one bot at 60ms + 2% loss)
#   combat.rs     melee judged where the target is on the server (standing in and out of
#                 reach, running past), death and respawn
#   abilities.rs  spirit spear thrown at once (no windup) and hitting; rift step cutting
#                 through a target once and readying the blade, cutting where it ends but not
#                 who it leaves behind; frost nova freezing a revenant (no walk, no dash),
#                 then letting it go; frostbolts slowing
#   inputs.rs     4 fps client firing once per click; a frozen client's fighter finishing its
#                 walk; attacking cancels the walk, moving cancels the windup but not the
#                 backswing
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

In practice, the "Free camera" switch in the practice tools turns on a free, WoW-style camera
for looking at fighters up close. It follows behind you: WASD walks relative to it, holding
the right mouse button and dragging (or the arrow keys) turns it, the wheel (or + / -) zooms,
down to arm's length. There the right button only turns the camera: click to move is off. Tab
moves it on to the next fighter, to watch them play (and after the last, back to
you). Switching it off goes back; leaving practice does too. The keys for the current camera
are listed top left, under the ping (with the free camera also jitter).

Two tabs work: a hidden tab keeps simulating and networking without rendering (see
`client/src/hidden_tab.rs`, which works around Bevy 0.19 ignoring Lightyear's keepalive). If a
client does freeze, its fighter carries on with its last order, and the server drops it after
3s; the HUD then says DISCONNECTED and a page reload rejoins.

## Logs

The server logs to stderr. At info: start, connects and disconnects (with the reason), joins,
kills, a `status` line every minute (players, each client's ping and jitter), and warnings
when the server hitches. Debug
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

Or let GitHub Actions deploy: `bash deploy/deploy.sh ci-setup` once (needs `gh`, logged in), and
from then on every push to the `prod` branch builds the client and the server on GitHub and
installs them (`.github/workflows/deploy.yml`). The key it gets can only run that install
(`deploy/arena-deploy.sh`), and only the `prod` branch can use it.

If your provider has its own firewall (Hetzner Cloud Firewall), open the same ports there. A
deploy restarts the server, which disconnects everyone playing.

## Not done yet (see the plan)

- More classes, and more abilities (W/E/R); a balance pass (in bot duels the Revenant's
  dash-and-swing tends to beat the Jav; the Frost Mage is new and untuned)
- Network debug overlay (lag sliders, server ghost), room codes, switching class without rejoining
- Visual smoothing: your own fighter moves in 32 Hz steps (it's shown as last received);
  interpolate it between ticks
- Accounts service issuing netcode connect tokens (currently a shared zero dev key)
- WASM size: 44 MB raw / ~10 MB gzipped; add wasm-opt and brotli, and a loading bar
