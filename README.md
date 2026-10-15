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
| Jav | Marksman | Slow, heavy javelins that hit harder the farther they fly (`far_scale`) | **Deadeye**: at once, your next auto-attack will critically strike, with 50% more attack speed; press again to call it off (15s cooldown, from the throw; off the global cooldown) |
| Revenant | Duelist | A heavy overhead blow of its dark glaive, the blade coming down flat on the ground from 1.6 m to 3.08 m ahead, tearing it open for 3s (inside the tear the revenant moves 30% faster, and enemies standing in it burn for 4 every 0.5s, the revenant healing a quarter of it); each hit empowers the glaive for the next swing if it follows within 10s (time to kite): a lane 50% further and wider, from 2 m to 4.62 m, 0.75 m wide, 50% more attack speed and a 50% crit chance (it never crits otherwise; an empowered hit renews it), and it knocks enemies up for 0.3s (shorter than its own quickened attack, so it can't chain alone), lost on a miss (`follow_up`, the **Follow Through** passive) | **Rift Step**: dash toward the cursor, 1 m to 6 m, damaging each enemy you pass through |
| Frost Mage | Controller | Frostbolts (mid range) that shatter where they hit, the burst of ice hitting everyone round the one struck for most of the damage; every hit slows (40% for 2s) | **Frost Nova**: slam the staff down and freeze everyone within 3.5 m in place for 1.5s (no walking, no dashing) |

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

## Waves (PvE)

Besides fighting each other (PvP: free-for-all or teams), a lobby can fight **Waves**: up to six
players together, against endless waves of monsters, on a map of their own, **Hollow Wilds**.
It's far bigger than the PvP maps, with a walled camp in the middle and places all around it: a
graveyard and ruined chapel, a hedge maze, a lake with an island, a quarry with a burned
watchtower, a rotting marsh with a witch's hut, a circle of standing stones, a chasm with a stone
bridge and a rope one, and a dark grove. Waygates jump from the camp's outskirts to the far
corners, and monsters come out of lairs out there (`shared/src/map/wilds.rs`).

A run opens with time at the **merchant** in the camp (walk up to them to trade): a run starts
with some gold, and every kill pays its bounty to the whole party (no kill stealing), every
cleared wave pays more, and **chests** turn up out in the wilds during a wave (gold for everyone,
if someone goes and gets them). Press F (or the banner's button) once you're ready; the next wave
comes once everyone is, or when the countdown runs out. A boss leads every fifth wave (The
Butcher, The Pale Lich, The Grave Lord, The Ember Archmage, The Plague Mother, then round again,
tougher each time), and from wave 6 a few
**elites** come along, bigger, tougher, worth more, and with a twist: Swift, Vampiric, Volatile
(blows up where it dies), Frostbound or Splitting. Everyone has **one life**: a fallen player is
**down** where they fell (marked by a ring of spirit light) until an ally stands over them for 3
seconds to pick them up, the wave is cleared, or a boss falls; if everyone is down, the run is
over (alone, the first fall ends it). Fury stones (pale yellow crossed blades) make you attack faster for a while; the blue winged boots make you walk faster.

Every wave is the same every run: which monsters come, in what order, which are elites, and its
omen are all the wave's own (only where they come out of, and where chests turn up, changes).
From wave 4 the second and fourth wave after each boss (4, 7, 9, 12, 14, 17, 19, ...) come with
an **omen**, in turn, shown while you shop for it: Gilded (double bounties), a Horde (half as many
again), a Frenzy (every monster hasted), the Restless Dead (every third to die rises again), a
Blood Moon (elites everywhere), Volatile (everything bursts as it dies), and round again.

Players' auto-attacks
knock monsters (zombies too) back a little, bosses aside, and the Jav's javelins pierce straight
through monsters (never players).

**Items** (`shared/src/items.rs`) never give stats: each does something, and each has its own
pixel icon (`client/src/item_icons.rs`). Six at most; buying one
you have raises it a level (up to 3), selling gives back half. A few are one class's own: the shop
lists them first under "<class> ONLY", and their tiles are rimmed in gold. Items that go off on an
ability go off when the ability deals damage (once a use; Deadeye's primed javelin counts). Only
an auto-attack's own hits set off the on-hit items (a Hydra Spear's barrage counts: each javelin is
thrown); what items deal (a ricochet's bounce, an arc, a burst), and a frostbolt's shatter round
the one it struck, doesn't.

Every map has a merchant. In practice everything is free, to try items out on the target dummies,
or on three slow zombies placed side by side ("3 slow zombies" in the practice tools; they walk at
half speed and come for you).
In a PvP match everyone starts with nothing and earns gold only by their own kills (25 a zombie,
60 a player), so items there are few and far between.

What items and monsters do is drawn as little scenes, not marker circles (`client/src/fx/`):
frost growing over the ground and thawing, scorched earth with glowing cracks and flames licking
out, thorns bursting up, a void vortex, lightning, coins thrown, a ring of runes where something
is about to land. Each effect is made of parts (`recipes.rs`) that live and fade on their own, in
procedural shapes (`shapes.rs`).

| Item | What it does |
|---|---|
| Storm Coil | Auto-attack hits arc lightning through more enemies |
| Ember Heart | Kills burst into flame (bursts kill, and burst) |
| Rime Sigil | Every few auto-attack hits freeze |
| Bloodfang | Auto-attack hits heal you |
| Bramble Mantle | Being hit lashes everyone around you |
| Echo Stone | Your ability's damage echoes where it hit, as a burst |
| Powder Keg | Lobs kegs at the nearest enemy now and then |
| Wisp Lantern | Wisps circle you, burning what they touch |
| Hoarfrost Boots | You leave hoarfrost behind that slows |
| Doom Bell | Marks enemies; the bell tolls back a share of what you dealt them |
| Midas Coin | Hits finish monsters low on health, for gold (not bosses) |
| Thunder Crown | Every few hits call down lightning |
| Void Orb | Your ability's damage opens a vortex where it hit that drags enemies in |
| Mirror Shard | Your ability's damage leaves a decoy monsters go for; it shatters |
| Berserker Totem | Kills stack a frenzy (walk and attack faster) |
| Ricochet Charm | Shots that hit bounce on (ranged classes) |
| Hydra Spear | Each auto-attack throws a barrage of 2-4 javelins (Jav only) |
| Gravecaller Rift | Follow Through's empowered strike, and the rift it tears, 15/30/50% longer and wider by level (Revenant only) |
| Heart of Winter | Frost Nova's damage leaves a blizzard (Frost Mage only) |

They combine: a Hydra Spear's barrage pierces a whole line and sets off Storm Coil arcs, Ember Heart's bursts kill into
more bursts and feed a Berserker Totem, a Void Orb gathers a pack for Thunder Crown and the kegs.

**Monsters** are classes too (`shared/assets/classes/`, with a `mob` section: how it gets about,
its tricks, its bounty and when it first comes), wearing the players' and the zombie's figures
sized and tinted (`look`): zombies, packs of gnawers, pouncing ghouls, spear-throwing grave
hunters that keep their distance, bloaters that burst, grave ogres that slam the ground, frost
wraiths, bonecallers raising gnawers, rot priests mending the others, plague bombardiers lobbing
bombs where you stand (keep moving), and death knights charging through you. Later on: void
acolytes (13) that throw fans of void bolts and blink away from anyone who closes in, packs of
ember imps (16) that throw themselves at you and burst into flame, and storm callers (19) that
call lightning down where you stand (the air crackles there first). The Ember Archmage (20)
throws fireballs that burst, rains meteors round you (their burning shadows show where), throws
rings of fire, and blinks away from anyone who closes in, the spot it left blowing up a moment
later; the Plague Mother (25) lumbers after you, slams out a wave of rot that slows, lobs volleys
of plague bombs and births bloaters. The server runs them (`server/src/brain.rs`), the waves (`server/src/waves.rs`, where
all the tuning is) and the items (`server/src/items.rs`).

**Measuring performance** (native): `ARENA_PERF=perf.txt` turns vsync off, notes every frame's
time and counts what's in the world each second (entities, meshes drawn, effect parts, lights, UI
nodes, assets), then writes a summary (average, median, 99th percentile, worst frame) and quits
after `ARENA_PERF_SECS` (60 by default). `ARENA_PERF_OFF=lights,shadows,fog,ui,fx` switches parts
of the scene off, to see what each costs. Building the client with `--features profile` also
writes a Chrome trace (`trace-*.json`) of every system's time, every frame (it gets big: a few GB a
minute). On a fast machine the frame rate hides costs that show on a slow one or in the browser,
so compare frame times, not frame rates. `scripts/trace_sum.py` sums a trace per system; what
we've measured so far, and how to run a perf loop, is in [docs/PERF.md](docs/PERF.md).

To try a later wave natively: `ARENA_ROOM="Waves 10" cargo run -p arena-client -- 11 revenant`
starts a run at wave 10 with the gold you'd have by then (`ARENA_BOT=1` to let the bot play it,
`ARENA_SHOTS=<dir>` to save a screenshot every few seconds). `cargo test -p arena-server --test
integration long_run -- --ignored --nocapture` plays a run with a bot that shops, printing how
each wave goes (`ARENA_CLASS` picks its class).

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
#   abilities.rs  Deadeye priming the next attack; rift step cutting
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
