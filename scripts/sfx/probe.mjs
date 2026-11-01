// Decode recordings in headless Chrome (ogg/flac/mp3/wav alike) and print each one's duration,
// peak, and a 50 ms RMS envelope; or export one as 16-bit mono WAV.
//   node probe.mjs info URL...            (URLs under http://127.0.0.1:8099/)
//   node probe.mjs wav URL out.wav [rate]  (resampled to rate, default 22050)
import { writeFileSync } from "node:fs";
const [, , mode, ...args] = process.argv;
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
let page = list.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let seq = 0; const pending = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
const send = (method, params = {}) => new Promise((r) => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
await send("Page.enable");
const nav = await send("Page.navigate", { url: "http://127.0.0.1:8099/" });
await new Promise((r) => setTimeout(r, 400));
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
};
const decode = `async (url, rate) => {
  const buf = await (await fetch(url)).arrayBuffer();
  const ctx = new OfflineAudioContext(1, 1, rate);
  const audio = await ctx.decodeAudioData(buf);
  const n = audio.length, ch = audio.numberOfChannels;
  const mono = new Float32Array(n);
  for (let c = 0; c < ch; c++) { const d = audio.getChannelData(c); for (let i = 0; i < n; i++) mono[i] += d[i] / ch; }
  return { mono, rate: audio.sampleRate };
}`;
if (mode === "info") {
  for (const url of args) {
    const info = await evaluate(`(${decode})(${JSON.stringify(url)}, 44100).then(({mono, rate}) => {
      let peak = 0; for (const x of mono) peak = Math.max(peak, Math.abs(x));
      const step = Math.round(rate * '${Number(process.env.STEP ?? 0.05)}'), env = [];
      for (let i = 0; i < mono.length; i += step) { let s = 0; const m = Math.min(mono.length, i + step); for (let j = i; j < m; j++) s += mono[j] * mono[j]; env.push(Math.sqrt(s / (m - i))); }
      return { dur: mono.length / rate, rate, peak, env };
    }).catch((e) => ({ error: String(e) }))`);
    if (info.error) { console.log(url, "ERROR", info.error); continue; }
    console.log(`${url.replace("http://127.0.0.1:8099/", "")}: ${info.dur.toFixed(2)}s rate=${info.rate} peak=${info.peak.toFixed(2)}`);
    console.log("  " + info.env.map((v, i) => `${(i * Number(process.env.STEP ?? 0.05)).toFixed(2)}:${v.toFixed(2)}`).join(" "));
  }
} else if (mode === "wav") {
  const [url, out, rateArg] = args;
  const rate = Number(rateArg ?? 22050);
  const b64 = await evaluate(`(${decode})(${JSON.stringify(url)}, 44100).then(async ({mono, rate: src}) => {
    const n = Math.round(mono.length * ${rate} / src);
    const off = new OfflineAudioContext(1, n, ${rate});
    const b = off.createBuffer(1, mono.length, src); b.copyToChannel(mono, 0);
    const s = off.createBufferSource(); s.buffer = b; s.connect(off.destination); s.start();
    const r = await off.startRendering(); const d = r.getChannelData(0);
    const pcm = new Int16Array(d.length); for (let i = 0; i < d.length; i++) pcm[i] = Math.max(-32768, Math.min(32767, Math.round(d[i] * 32767)));
    let bin = ""; const bytes = new Uint8Array(pcm.buffer); for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    return btoa(bin);
  })`);
  const pcm = Buffer.from(b64, "base64");
  const header = Buffer.alloc(44);
  header.write("RIFF", 0); header.writeUInt32LE(36 + pcm.length, 4); header.write("WAVE", 8); header.write("fmt ", 12);
  header.writeUInt32LE(16, 16); header.writeUInt16LE(1, 20); header.writeUInt16LE(1, 22); header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 2, 28); header.writeUInt16LE(2, 32); header.writeUInt16LE(16, 34); header.write("data", 36); header.writeUInt32LE(pcm.length, 40);
  writeFileSync(out, Buffer.concat([header, pcm]));
  console.log(`wrote ${out} (${(pcm.length / 2 / rate).toFixed(2)}s @ ${rate})`);
}
ws.close();
