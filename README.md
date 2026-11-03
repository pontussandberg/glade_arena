# Arena

Browser PvP arena brawler: a third-person 3D presentation on 2D gameplay rules, with one
authoritative server: clients send commands, never positions. Your own movement is predicted,
attacks and abilities show once the server confirms them (about a round trip), and other players
are interpolated.

The fight happens on one of three maps on a 1 m tile grid, picked by the room's leader (or
switched in practice): River Crossing, a forest clearing with a river through the middle crossed
by a stone bridge and two fords; Riven Gorge, a chasm that shots fly over and the bold leap (fall
short and you fall in), with rope bridges and waygates that jump you to the far side; and
Ravenhold, a fortress over a village by a river. Ruined walls, boulders and trees give cover. The
camera is third person, Smite-style, over your fighter's right shoulder, and you aim where it
looks: the mouse turns it, **WASD** walks relative to it, **holding left click** auto-attacks
onto the crosshair's line (a skillshot, up or down too), and your abilities' keys use them the
same way. Your abilities' cooldowns show on icons at the bottom of your screen, and a minimap in
the corner shows everyone.

Before joining you pick a **class** in the lobby, where it stands on a stage to look at (drag
to turn it) next to its numbers and abilities; then enter the arena. Each class has its own HP, speed, auto-attack (melee or
projectile) and abilities (up to four, on the number keys 1 to 4; each class has one so far); all of
it lives in its own file in `shared/assets/classes/`, next to `common.ron` for what every class
shares (field reference in the README there):

| Class | Role | Auto-attack | Ability |
|---|---|---|---|
| Jav | Marksman | Slow, heavy javelins that hit harder the farther they fly (`far_scale`), pierce monsters, and crit now and then | **Deadeye**: at once, your next auto-attack will critically strike, with 50% more attack speed; press again to call it off (15s cooldown, from the throw; off the global cooldown) |
| Revenant | Duelist | A heavy overhead blow of its dark glaive, the blade coming down flat on the ground from 2 m to 4.62 m ahead, 0.75 m wide, tearing it open for 3s (inside the tear the revenant moves 30% faster, and enemies standing in it burn for 4 every 0.5s, the revenant healing a third of it); each hit arms the next blow, if it follows within 10s (time to kite): it knocks enemies up for 0.3s (shorter than the attack itself, so it can't chain alone), lost on a miss, and an armed hit arms the next (`follow_up`, the **Follow Through** passive) | **Rift Step**: dash toward the cursor, 1 m to 12 m, damaging each enemy you pass through |
| Frost Mage | Controller | Frostbolts that fly until they meet something and shatter there, the burst of ice hitting everyone round the mark for most of the damage; every hit slows (40% for 2s), and it always crits a frozen enemy | **Frost Nova**: slam the staff down and freeze everyone within 8.75 m in place for 2.5s (no walking, no dashing) |

Hits can carry **crowd control** (`chill` in the class files): a slow takes a share off walking
and dashing speed (a dash still goes its full distance, just slower; a haste speeds it up the
same way), a root freezes a fighter in place (it can't walk or dash, and a dash in
progress stops) but it can still attack, and a knock-up tosses it: no walking, dashing, attacking
or casting while in the air, and a windup in progress is lost (the only thing that cancels one).
Slowed fighters take on a cold blue cast and frost turns
under their feet; frozen ones stand in ice.

Each auto-attack starts on a click, and the next may start once its attack timer
(`attack_ticks`) has run out; holding the button attacks again the moment each is ready. Once the
attack has gone off, the cast bar under your health bar fills until then. A click before then is
dropped, not queued. The attack goes
off part way through that time, after a windup (`windup`, a part of the attack per class): the
spear leaving the hand, the blade reaching its target. You keep walking through the windup (a
shot winding up slows you), and the attack follows your aim until it goes off. A blow goes off
the tick its windup is over; a shot drawn past its windup with the button still held stays drawn,
and flies the moment you let go. After it has gone
off, the rest is backswing: move whenever you like while the timer runs on (the animation plays
out over the walk; only a dash cuts it short). Nothing on screen shows when the
attack has gone off but your fighter itself, so learning each class's release point is the skill
check (attack, then step, then attack again). Attack speed (Deadeye, the frenzy of zombie kills
and the Berserker Totem's, a fury stone's, the practice shop's)
shortens the timer and the windup alike, and plays the animation faster. Everyone sees the windup in
the fighter's body, so attacks can be read and dodged. Starting an attack or a dash drops a walk
toward a point (a bot's order); your own WASD walking carries on through both. A dash may be used mid-windup: it moves you,
and the attack still goes off when the windup's over, from wherever the dash took you (stab, then
dash into place for it to land).
Abilities have their own cooldown (shown in the HUD). Like everything else they happen on the
server: your dash moves once the server has your cast, about a round trip
after you press it.
Killed players sit out 10 seconds, then respawn at full health.

Stack: **Bevy 0.19 + Lightyear 0.30** (pinned together), Rust everywhere. The browser client is
the same Rust code compiled to WASM, talking WebTransport to the server.

## Waves (PvE)

Besides fighting each other (PvP: free-for-all or teams), a lobby can fight **Waves**: up to five
players together, against endless waves of monsters, on **Ravenhold** (where solo runs go) or on
a map of the waves' own, **Hollow Wilds**.
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
**down** where they fell (marked by a pool of healing light, a cross filling in as the rescue
goes) until an ally stands over them for 3
seconds to pick them up (they come back at 40% health), the wave is cleared, or a boss falls; if
everyone is down, the run is
over (alone, the first fall ends it). Fury stones (pale yellow crossed blades) make you attack faster for a while; the blue winged boots make you walk faster; a glowing green cross heals you.

Every wave is the same every run for a party of the same size (a bigger party gets more): which
monsters come, in what order, which are elites, and its
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
or on slow zombies placed side by side ("3 slow zombies", or a horde of them with "50 slow zombies",
in rows, in the practice tools; they walk at half speed and come for you). In practice the shop also
sets your attack speed, slower or faster by a quarter at a time.
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

**Playing**: from home, **Play solo** starts a run against the waves of your own (never listed).
**Play with friends** lists the lobbies and makes new ones (PvE first; a password is optional:
only those who give it can join). In a lobby everyone readies up, and the leader starts once
they all are; a leader who leaves passes the lead on, and the lobby is gone once everyone has
left. A run's (or match's) end shows until each player leaves with its **Leave** button.

**Leaderboards**: every run against the waves that ends (lost, or left under way) and was played
from the first wave goes on the leaderboards, one for each party size (solo up to five), shown on
the home screen under the lobbies: the top five of each, with each entry's name, hero, map and
wave. Every run that ends asks each player, once they're out of the arena, for the name their
entry goes by (all runs are kept, on the board or not). There's
no database yet: the server appends each run to a CSV file (`ARENA_HIGHSCORES`, default
`highscores.csv` where it runs; on the server, `/var/lib/arena/highscores.csv`) and reads it back
when it starts (`server/src/highscores.rs`). Each line: the date (UTC), the game's version (the
workspace's, `GAME_VERSION`, now 0.0.1), the wave, the map, how many played, and the party (names
and heroes).

**Measuring performance** (native): `ARENA_PERF=perf.txt` turns vsync off, notes every frame's
time and counts what's in the world each second (entities, meshes drawn, effect parts, lights, UI
nodes, assets), then writes a summary (average, median, 99th percentile, worst frame) and quits
after `ARENA_PERF_SECS` (60 by default). `ARENA_PERF_OFF=lights,shadows,fog,ui,fx,blend` switches
parts of the scene off, to see what each costs (in the browser, the same words as `?off=` on the
URL). Building the client with `--features profile` also
writes a Chrome trace (`trace-*.json`) of every system's time, every frame (it gets big: a few GB a
minute). On a fast machine the frame rate hides costs that show on a slow one or in the browser,
so compare frame times, not frame rates. `scripts/trace_sum.py` sums a trace per system; what
we've measured so far, and how to run a perf loop, is in [docs/PERF.md](docs/PERF.md).

To try a later wave natively: `ARENA_ROOM="Waves 10" cargo run -p arena-client -- 11 revenant`
starts a run at wave 10, on Ravenhold, with the gold you'd have by then (`ARENA_BOT=1` to let the bot play it,
`ARENA_SHOTS=<dir>` to save a screenshot every few seconds). `cargo test -p arena-server --test
integration long_run -- --ignored --nocapture` plays a run with a bot that shops, printing how
each wave goes (`ARENA_CLASS` picks its class).

## Layout

```
shared/   protocol (replicated components, PlayerInput, messages), sim (pure gameplay rules),
          map (the tile map, one file per map, A* pathfinding, line-of-movement checks),
          classes (+ assets/classes/), abilities (one module per kind: throw, dash, nova, prime),
          items (what each item is), rooms (modes, maps, omens), config
server/   headless authoritative server (commands, spawning, hit decisions, respawns), the waves
          (waves.rs: all the tuning; brain.rs: the monsters), items.rs (what items do),
          highscores, rooms, certs;
          tests/integration/ has the end-to-end tests (one test binary: each links all of Bevy)
client/   ClientNetPlugin (networking + commands, headless-capable), bot (sparring AI);
          render (fighters, shots, effects, HUD, input, aim; glide is your own prediction),
          swish (a blade's wind), camera, fx (the little scenes items and monsters make),
          audio (every sound, layered at play time), cloth (moving cloth), sculpt (lofted
          meshes),
          action_bar (your passive and ability icons, cooldown, hover tooltips), stat_frame (a
          class's stats: in the lobby and bottom left in the arena), minimap,
          arena (the 3D scene and all meshes), rig (animated fighters: facing, walk cycle,
          windup and throw), lobby (character select), feedback (health and cast bars),
          waves (the shop and banners), practice (the practice tools), item_icons, browser and
          rooms (the lobby list), esc_menu, scoreboard, pickups, waygates
client/web/  index.html for the browser build (pkg/, digest.txt and the wasm size are
          generated), sfx/ (the sound samples and CREDITS.txt)
docs/     art-direction.md (the look), sound-direction.md (the sound), PERF.md (perf work)
scripts/  dev.sh, build-web.sh, serve.mjs, trace_sum.py, capture/ (headless-Chrome eyes and
          ears), sfx/ (sound cutting)
```

### How a tick works

- After connecting, a client says hello and sends `ChooseClass` whenever a class is picked; the
  server spawns its fighter once it's in a started room (in practice, a new pick respawns you on
  the spot).
- Clients send only **commands** (`PlayerCommand`: walk with the keys, attack toward an aim, the
  button held or let go, the aim as the camera turns, cast an ability, jump; a bot also walks to
  a point, or stops), never positions. They're
  sent when something changes (a click goes out right away; the aim while attacking at most once
  a tick) on a reliable channel, each stamped with its tick; the server holds one for a coming
  tick until that tick comes (at most a second ahead) and applies it to the
  player's intent (`PlayerInput`), which the sim runs on every tick. The server drops a
  destination by itself when you arrive, attack or dash, and a fighter carries on with its last
  order if its client goes quiet.
- The server runs `shared::sim` on those intents (pathfinding toward the clicked tile, attack
  windups and cooldowns, Q abilities and dashes, projectile flight) at 32 Hz, decides hits and
  damage, and replicates state every tick.
- Pathfinding is A* over the shared tile map, smoothed into straight lines where the way is
  clear. The map uses no platform math functions (sin, atan2), so native and WASM agree to the
  bit. Players steer with WASD; the paths are walked by bots (which pick the point,
  `walk_target`) and monsters.
- Walls, rocks and trees block movement, and block shots up to their height (a thrown shot
  clears what its arc tops); the forest edge blocks everything; water is waded, slowly; a chasm
  stops walkers (shots fly over it, and on Riven Gorge you can leap it).
- **Your own movement is predicted.** Your fighter is drawn where your keys and jumps will have
  taken it by the time the server has them, by the same sim the server walks it with
  (`render::glide`), your own winding shot's slow included; a dash isn't started early, but once
  the server's word arrives the prediction runs it on ahead under your steering. Where the
  server turns out to disagree, the difference is eased away.
  Everything else (an attack going off, an ability) shows about a round trip later.
- Other players are **interpolated** between server updates, a little in the past. Their
  `AttackState`, `Cooldowns` and `Dashing` are shown on that same timeline, so a cast bar and a dash line
  up with where you see them; except that a throw shows the moment its spear appears
  (`SeenThrows`), which can be a little ahead of the thrower's body.
- **Projectiles** fly on rails: a shot's position is a pure function of the tick
  (`sim::projectile_pos`, from its origin, direction and spawn tick), so clients draw them from
  that alone, on your own fighter's clock: a shot coming at you is drawn where it is relative to
  you.
- **Every hit is judged where everyone is on the server** at that tick (no lag
  compensation): melee, dashes, novas and projectiles alike. What you dodge on the server, you
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
# (the argument is the wasm profile: `dev` compiles much faster; none builds wasm-release)
./scripts/dev.sh dev

# tests (~10s to run, plus the build): sim unit tests, plus real server + headless bot clients
# over WebTransport (server/tests/integration/):
#   map/sim       map symmetry, pathfinding, every spawn point reaching every other, water
#                 waded; attack timers, windups, knock-ups, dropped clicks, cooldowns,
#                 far_scale, dashes (full distance, never into a wall, moving a mid-windup
#                 attack with them), slows, roots (stopping walks and dashes), novas
#   classes       the class file parses and every class has sane numbers
#   netcode.rs    a command showing about a round trip later, own view and interpolation
#                 matching the server, unreachable clicks, server corrections, windup and
#                 shot, server-decided hit (one bot at 60ms + 2% loss), fake latency
#   combat.rs     melee judged where the target is on the server (standing in and out of
#                 reach, running past), the rift burning and healing, death and respawn
#   abilities.rs  Deadeye priming the next attack (with the very click too) and called off;
#                 rift step cutting
#                 through a target once, cutting where it ends but not
#                 who it leaves behind; frost nova freezing a revenant (no walk, no dash),
#                 then letting it go; frostbolts slowing
#   inputs.rs     4 fps client firing once per click; a frozen client's fighter finishing its
#                 walk; key walks moving and stopping; a drawn shot slowing the walk and
#                 going off once when let go
#   pickups.rs, rooms.rs, waves.rs
#                 pickups, lobbies and whole runs end to end (long_run, ignored by default,
#                 plays one through)
cargo test

# server (run from the repo root; writes client/web/digest.txt for the browser; see Logs below;
# ARENA_PORT picks another port than 5888)
cargo run -p arena-server

# browser client
./scripts/build-web.sh          # -> client/web/pkg
node scripts/serve.mjs          # -> http://localhost:8080 (PORT picks another; open twice to
                                #    play; you are blue). On localhost the page also takes
                                #    ?server=ip:port, ?room=, ?class= and ?bot
# native client (optional, handy for debugging): [client id] [class], e.g. `11 javelinist`;
# without a class it opens the home screen. ARENA_SERVER=ip:port picks another server.
cargo run -p arena-client

# a sparring partner: a native client played by a simple bot
ARENA_BOT=1 cargo run -p arena-client -- 99 revenant

# to watch two bots fight, start a second one, then in your client press F1 to move the camera on to them
ARENA_BOT=1 cargo run -p arena-client -- 98 javelinist
```

The server makes a new self-signed certificate (valid 14 days) on every start. The page reads
its hash from `digest.txt`, so after restarting the server, reload the page. Chrome and Firefox
support WebTransport with certificate hashes.

Controls: in the lobby, pick a class with a click or its number key and enter with the button.
In the arena the camera is third person, behind you over your right shoulder: a click
captures the mouse, which then turns it (Esc opens the menu, freeing it, and closing the menu
takes it back; a right click while Tab is held frees it to point with, and a click takes it back;
until then, dragging with the right button or the arrow keys turn it). WASD walks relative to it, holding left click attacks onto the
crosshair, the number keys use your abilities, Space jumps, the wheel (or + / -) zooms, down to
arm's length, holding Tab shows the scoreboard, M opens the full map, F interacts (the merchant's
shop) and readies up between waves, and F1 moves the camera on to the next fighter,
to watch them play (and after the last, back to you). V puts it in first person, at your
fighter's eyes, and back (not finished yet). The keys are in the Controls fold-out top left,
under the ping.

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

- More classes, and more abilities (each class fills one of its four slots); a balance pass (in
  bot duels the Revenant's
  dash-and-swing tends to beat the Jav; the Frost Mage is new and untuned)
- Network debug overlay (lag sliders, server ghost; practice has a fake-ping tool, and the ping
  line shows the prediction's lead), room codes
- Accounts service issuing netcode connect tokens (currently a shared zero dev key)
- WASM size: add wasm-opt and brotli (Caddy already serves zstd and gzip)
