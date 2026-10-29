// Sum up a CPU profile from shoot.mjs's `profile:` step: where the page's main thread spent its
// time, by crate and by function (self time, so what each did itself rather than what it
// called). Usage: node profsum.mjs PROFILE.cpuprofile [FUNCTIONS] [MATCH]
//   FUNCTIONS: how many of the heaviest functions to list (default 30).
//   MATCH: also list the heaviest functions whose names match this regex by their inclusive
//   time (what they did and all they called), e.g. `^[a-z_]+::[a-z_0-9:]+$` for plain
//   functions (a system's own among them). `systems` lists Bevy's systems so, each by its path.
// `(program)` is the browser's own work between tasks, `(idle)` time with nothing to do: a frame
// rate that's the work's limit, not the display's, shows no idle at all.
import { readFileSync } from "node:fs";

const [, , file, top = "30", match] = process.argv;
const profile = JSON.parse(readFileSync(file, "utf8"));
const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
const byCrate = new Map();
const byFunction = new Map();
let total = 0;
for (let i = 0; i < profile.samples.length; i++) {
  const us = profile.timeDeltas[i] ?? 0;
  total += us;
  const frame = nodes.get(profile.samples[i]).callFrame;
  const name = frame.functionName || "(anon)";
  // Rust symbols come as `crate[hash]::path`, maybe inside `<... as ...>`; JS and browser
  // frames have no crate.
  const crate = name.match(/([a-z_0-9]+)\[/)?.[1] ?? (frame.url ? "(js)" : name);
  byCrate.set(crate, (byCrate.get(crate) ?? 0) + us);
  byFunction.set(name, (byFunction.get(name) ?? 0) + us);
}
const ranked = (map) => [...map].sort((a, b) => b[1] - a[1]);
const line = (us, what) => `${(100 * us / total).toFixed(1).padStart(5)}%  ${(us / 1000).toFixed(0).padStart(6)} ms  ${what}`;
console.log(`total ${(total / 1000).toFixed(0)} ms, ${profile.samples.length} samples`);
console.log("\nby crate");
for (const [crate, us] of ranked(byCrate).slice(0, 20)) console.log(line(us, crate));
console.log("\nby function");
for (const [name, us] of ranked(byFunction).slice(0, Number(top))) console.log(line(us, name.replace(/\[[0-9a-f]+\]/g, "").slice(0, 140)));
if (match) {
  // Each sample counts once for every distinct function on its stack.
  const parent = new Map();
  for (const n of profile.nodes) for (const child of n.children ?? []) parent.set(child, n.id);
  const re = new RegExp(match === "systems" ? "" : match);
  // A system runs as `<FunctionSystem<fn(..), .., path::to::system> as System>::run_..`.
  const system = (name) => name.match(/, ([a-z_0-9:]+(?:<[^<>]*(?:<[^<>]*>)?[^<>]*>)?)> as bevy_ecs::system::system::System>::run/)?.[1];
  const named = match === "systems" ? system : (name) => (re.test(name) ? name : undefined);
  const inclusive = new Map();
  for (let i = 0; i < profile.samples.length; i++) {
    const us = profile.timeDeltas[i] ?? 0;
    const seen = new Set();
    for (let id = profile.samples[i]; id !== undefined; id = parent.get(id)) {
      const name = named((nodes.get(id).callFrame.functionName || "(anon)").replace(/\[[0-9a-f]+\]/g, ""));
      if (name !== undefined && !seen.has(name)) {
        seen.add(name);
        inclusive.set(name, (inclusive.get(name) ?? 0) + us);
      }
    }
  }
  console.log(`\ninclusive, matching /${match}/`);
  for (const [name, us] of ranked(inclusive).slice(0, Number(top))) console.log(line(us, name.slice(0, 140)));
}
