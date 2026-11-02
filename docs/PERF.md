# Client performance: how to measure, what we know

Notes from the first round of perf work (October 2026; baselines re-measured November 2026): the
tools, how to use them so the numbers mean something, what costs what, and what's still open. Written as a starting point for a perf
loop: measure, change one thing, measure again, keep it only if the frame time drops.

## Measuring

### The probe: `ARENA_PERF`

Native client only (`client/src/perf.rs`).

```sh
cargo build -p arena-server -p arena-client          # dev profile is fine for relative numbers
./target/debug/arena-server.exe &
ARENA_PERF=perf.txt ARENA_PERF_SECS=30 ./target/debug/arena-client.exe 11 revenant
```

- It turns **vsync off**, so the frame rate is what the work allows, not the monitor's.
- It notes every frame's time and, once a second, a line of counts: main/render/extract times,
  entities, meshes and meshes drawn, batches, fx parts and corners, lights, UI nodes,
  mesh/material/image assets.
- At the end it writes a summary line (frames, avg, median, p99, p999, worst) and the per-second
  lines, plus an entity census (`<out>.census.txt`: what kinds make up the world, taken near the
  end), then quits.
- `ARENA_PERF_OFF=lights,shadows,fog,ui,fx,blend` switches parts of the scene off, for A/B tests
  (matched by substring; in the browser, the same words go on the URL as `?off=...`):
  - `lights` removes the torches' point lights;
  - `shadows` turns off the sun's shadows;
  - `fog` removes distance fog;
  - `ui` hides every UI root;
  - `fx` despawns effect parts and clears the blade trails;
  - `blend` hides everything see-through (blended materials, the lying ground marks, fx, trails).

### Rules for numbers you can trust

- **Compare frame times (ms), never fps.** fps isn't linear in cost. A 144 Hz monitor and a fast
  machine hide costs that show on a slow laptop or in the browser.
- **One change per run, same scene, same length.** Run a baseline right before the change, not
  one from earlier: background load on the machine moves the numbers by a few tenths of a ms.
- Look at both the **average/median** (steady cost) and **p99/worst** (hitches); they have
  different causes.
- Check the per-second counts too. A rising entity, mesh or asset count is a leak, and a fix
  that "speeds up" a scene by spawning less of it isn't a fix.

### Scenes to test

Test both a steady scene and a busy one:

| Scene | How |
|---|---|
| Home screen (the stage, the backdrop) | the client with no class argument |
| A camp, steady | `ARENA_ROOM="Waves 1"` with no bot: a run's shop phase, nothing fighting — but the first shop lasts only 25 s, so keep the run inside it (or expect wave 1) |
| A busy fight | `ARENA_ROOM="Waves 6" ARENA_BOT=1` (or `"Waves 8"`): the bot fights a mid-run wave |

`ARENA_ROOM` only applies when a class argument is given. "Waves..." runs start on Ravenhold (the
waves' first map); "Ravenhold..." does the same; any other name quick-joins a started FFA room on
River Crossing; "Practice <map label>" opens that map in peace. `ARENA_SHOTS=<dir>`
saves a screenshot every few seconds, so you can check afterwards what the run actually showed.

### Where the time goes: `--features profile`

```sh
cargo build -p arena-client --features profile    # turns on bevy/trace_chrome
```

- Running it writes `trace-*.json` with every system and render span, every frame.
- **It's huge (about 6 GB a minute)** and tracing roughly **doubles the frame time**, so read
  it for proportions, not absolute numbers.
- Keep the runs short (`ARENA_PERF_SECS=20`) and delete the trace when you're done (the disk
  has filled up before).
- Sum it with `scripts/trace_sum.py <trace> <from_s> <to_s> [filter]`. It streams the file,
  matching begin/end events with a regex, and prints ms per frame and calls per frame per span,
  most costly first (the filter is a plain substring). Skip the first few seconds (startup and
  pipeline compilation).

### The browser, and crowds

The wasm build is the real target, and it has its own probes:

- The page takes `?off=` (the `ARENA_PERF_OFF` words), `?room=`, `?class=`, `?bot` (or
  `?bot=stand`, natively `ARENA_BOT=stand`, to stand still in the fray), and `?pkg=pkg-xxx` to
  load another saved build for an A/B (`client/web/index.html`).
- `ARENA_HORDE=N` on the server keeps every wave at N monsters, coming out quickly and never
  running out, with an unkillable party: a steady crowd to measure against.
- `scripts/capture/shoot.mjs` drives a muted headless Chrome: `fps:SECONDS` measures frame
  times, `throttle:N` slows the tab's CPU, `profile:SECONDS,PATH` takes a CPU profile;
  `scripts/capture/profsum.mjs` sums one by crate and function (its `systems` mode gives
  inclusive time per Bevy system).
- The client's `webgpu` feature builds for WebGPU, to compare against WebGL.

## What we found (this machine: fast desktop GPU, native, dev profile)

The baselines below were re-measured 2 November 2026, after the big rounds (the Bevy feature
trim, health bars leaving the UI tree, fx parts drawn from GPU buffers, the baked rift tears).
The trace-derived proportions further down are still from October's traces; read those for
proportions and re-trace before leaning on them.

### Baseline numbers

| Scene | Frame time | Notes |
|---|---|---|
| Home screen | ~4.1 ms avg, ~5.5 ms p99 | ~870 entities, ~180 meshes, ~160 UI nodes, 22 lights |
| Ravenhold camp, steady (`"Waves 1"`, the shop) | ~4.4 ms avg, ~6.1 ms p99 | much the same counts as the home screen |
| Waves fight (bot, `"Waves 6"`) | ~4.7 ms avg, ~6.6 ms p99 | ~1,300 entities, ~500 meshes, ~180 UI nodes, 20 lights |

(October, before those rounds: the fight ran ~5.9 ms with 2,000–2,500 entities and ~900 UI
nodes, and the home screen 5.1 ms.) Every scene's first frame is 170–180 ms (startup and
pipeline compilation), and rare 13–16 ms hitches remain in the p999 — the pipeline-prewarm lead
below still stands.

### The game's own systems are cheap

- Every fx system together takes about **0.03 ms/frame**.
- Cloth `drape` takes about 0.3 ms/frame, the costliest of our own systems.
- Mesh and material asset counts stay flat through a fight, so there's no asset leak.

When a scene is slow, look at the engine's per-entity work before our gameplay code.

### The engine dominates

The top items in a trace:

- per-entity render work: extract, prepare, queue, and the mesh/material bind work, which scale
  with the number of drawn meshes;
- UI layout, about **0.7 ms** then (around 900 nodes in a fight; health bars have since left the
  UI tree, and a fight is now ~180 nodes);
- transform propagation and visibility checks, which scale with entity count;
- light clustering, `prepare_clusters` about **0.9 ms** with tracing on.

So the levers are **fewer entities and meshes** (merge static scenery, pool or batch fx parts),
**fewer UI nodes** (hide or despawn panels that aren't shown, rather than keeping them laid
out), and **fewer lights**.

### Shadow-casting point lights are very expensive

- A point light that casts shadows renders the scene **six times** (a cube map), every frame.
- The two stage lights on the home screen took it from 5.1 ms to 9.4 ms, nearly doubling it.
  Their shadows are now off (`client/src/lobby/stage.rs`).
- Rule: only the sun casts shadows. Any new `PointLight`/`SpotLight` gets
  `shadow_maps_enabled: false` unless there's a measured reason.

### Tried, no gain

- **A torch-light budget**: dropping far torches' lights (range 0) or removing them outright made
  no measurable difference at 34 lights.
- **`ClusterConfig::Single` / `FixedZ`** instead of the default clustering: no gain.

## Open leads

- **Hitches of 30–60 ms**, probably pipelines compiling on first use (a new effect, material
  or mesh layout the first time it's drawn). A prewarm (spawn each effect once, off-screen or
  invisible, during loading) would likely remove them. Confirm in a trace first: look for
  pipeline/shader spans in the slow frames.
- **The browser (wasm) is the real target.** It's single-threaded and likely 2–4× slower than
  native, so a native 6 ms frame may be 15–25 ms in the browser. Check the browser's fps
  counter and devtools performance tab on the `wasm-release` build, not just native numbers.
- **Entity count** in fights (~1,300, ~500 meshes): see which kinds dominate (fx parts, rig
  bones) and whether they can be merged or pooled. (UI nodes used to be a lead at ~900; health
  bars leaving the UI tree took a fight to ~180.)
- **Fog**: once, removing it unexpectedly showed a gain on the home screen; it didn't
  reproduce clearly. Worth an A/B with several runs each.

## A perf loop, step by step

1. Pick a scene and run a baseline with the probe. Write down avg, median, p99 and the counts.
2. If you don't know where the time goes, take a short traced run and sum it.
3. Make **one** change aimed at the top cost.
4. Rebuild and run the same scene for the same length. Keep the change only if the frame time
   drops by more than the run-to-run noise (a few tenths of a ms), and check it on a busy scene
   too.
5. Check that nothing visible broke (`ARENA_SHOTS`), and that the counts didn't just drop
   because something stopped spawning.
