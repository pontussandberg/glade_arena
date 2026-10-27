// Sum up a CPU profile from shoot.mjs's `profile:` step: where the page's main thread spent its
// time, by crate and by function (self time, so what each did itself rather than what it
// called). Usage: node profsum.mjs PROFILE.cpuprofile [FUNCTIONS]
//   FUNCTIONS: how many of the heaviest functions to list (default 30).
// `(program)` is the browser's own work between tasks, `(idle)` time with nothing to do: a frame
// rate that's the work's limit, not the display's, shows no idle at all.
import { readFileSync } from "node:fs";

const [, , file, top = "30"] = process.argv;
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
