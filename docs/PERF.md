# Client performance: how to measure, what we know

Notes from the first round of perf work (October 2026): the tools, how to use them so the numbers
mean something, what costs what, and what's still open. Written as a starting point for a perf
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
- It notes every frame's time and, once a second, a line of counts: entities, meshes and meshes
  drawn, fx parts, lights, UI nodes, mesh/material/image assets.
- At the end it writes a summary line (frames, avg, median, p99, p999, worst) and the per-second
  lines, then quits.
- `ARENA_PERF_OFF=lights,shadows,fog,ui,fx` switches parts of the scene off, for A/B tests:
  - `lights` removes the torches' point lights;
  - `shadows` turns off the sun's shadows;
  - `fog` removes distance fog;
  - `ui` hides every UI root;
  - `fx` despawns effect parts.

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
| The Wilds camp, steady | `ARENA_ROOM="Waves 1"` with no bot: a run's shop phase, nothing fighting |
| A busy fight | `ARENA_ROOM="Waves 6" ARENA_BOT=1` (or `"Waves 8"`): the bot fights a mid-run wave |

Room names that don't start with "Waves" quick-join a PvP room on Keep instead. `ARENA_SHOTS=<dir>`
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
  most costly first. Skip the first few seconds (startup and pipeline compilation).

## What we found (this machine: fast desktop GPU, native, dev profile)

### Baseline numbers

| Scene | Frame time | Notes |
|---|---|---|
| Home screen | 9.4 ms → **5.1 ms** | after turning off the stage point lights' shadows |
| Wilds camp, steady | ~5.3 ms | |
| Waves fight (bot) | ~5.9 ms avg, ~6.4 ms under load | 2,000–2,500 entities, ~500 meshes, ~900 UI nodes, 34 lights |

### The game's own systems are cheap

- Every fx system together takes about **0.03 ms/frame**.
- Cloth `drape` takes about 0.3 ms/frame, the costliest of our own systems.
- Mesh and material asset counts stay flat through a fight, so there's no asset leak.

When a scene is slow, look at the engine's per-entity work before our gameplay code.

### The engine dominates

The top items in a trace:

- per-entity render work: extract, prepare, queue, and the mesh/material bind work, which scale
  with the number of drawn meshes;
- UI layout, about **0.7 ms** (around 900 nodes in a fight);
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
  `shadows_enabled: false` unless there's a measured reason.

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
- **UI node count** in fights (~900): find out which panels make up most of it.
- **Entity count** in fights (2,000–2,500): see which kinds dominate (fx parts, rig bones,
  health bars) and whether they can be merged or pooled.
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
