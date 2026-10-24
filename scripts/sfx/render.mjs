// Cut and process a recording in headless Chrome's OfflineAudioContext, out to a 16-bit mono
// WAV ready for client/web/sfx: node render.mjs out.wav URL key=value...
//   start=S end=S       the slice (seconds in the source)
//   rev=1               reverse the slice
//   rate=R              playback rate (pitch and speed; 0.5 = an octave down, twice as long)
//   lp=HZ hp=HZ         lowpass / highpass biquads (Q 0.7)
//   fin=MS fout=MS      linear fades at the ends (default 5 / 20)
//   peak=P              normalize to this peak (default 0.89, -1 dBFS)
//   out=RATE            output sample rate (default 22050)
import { writeFileSync } from "node:fs";
const [, , outPath, url, ...kv] = process.argv;
const opt = Object.fromEntries(kv.map((s) => s.split("=")).map(([k, v]) => [k, Number(v)]));
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = list.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let seq = 0; const pending = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
const send = (method, params = {}) => new Promise((r) => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
await send("Page.enable");
await send("Page.navigate", { url: "http://127.0.0.1:8099/" });
await new Promise((r) => setTimeout(r, 400));
const r = await send("Runtime.evaluate", { awaitPromise: true, returnByValue: true, expression: `(async () => {
  const o = ${JSON.stringify(opt)};
  const outRate = o.out ?? 22050;
  const buf = await (await fetch(${JSON.stringify(url)})).arrayBuffer();
  const probe = new OfflineAudioContext(1, 1, 44100);
  const audio = await probe.decodeAudioData(buf);
  const src = audio.sampleRate, ch = audio.numberOfChannels;
  const a = Math.round((o.start ?? 0) * src), b = Math.min(audio.length, Math.round((o.end ?? audio.duration) * src));
  let mono = new Float32Array(b - a);
  for (let c = 0; c < ch; c++) { const d = audio.getChannelData(c); for (let i = a; i < b; i++) mono[i - a] += d[i] / ch; }
  if (o.rev) mono.reverse();
  // Resample by hand (linear interpolation): the slice at the output rate, sped up by rate.
  const rate = o.rate ?? 1;
  const step = rate * src / outRate, n = Math.max(1, Math.floor((mono.length - 1) / step));
  const slow = new Float32Array(n);
  for (let i = 0; i < n; i++) { const x = i * step, j = Math.floor(x), f = x - j; slow[i] = mono[j] * (1 - f) + (mono[j + 1] ?? 0) * f; }
  const off = new OfflineAudioContext(1, n, outRate);
  const sb = off.createBuffer(1, n, outRate); sb.copyToChannel(slow, 0);
  const s = off.createBufferSource(); s.buffer = sb;
  let node = s;
  for (const [type, key] of [["lowpass", "lp"], ["highpass", "hp"]]) if (o[key]) { const f = off.createBiquadFilter(); f.type = type; f.frequency.value = o[key]; f.Q.value = 0.7; node.connect(f); node = f; }
  node.connect(off.destination); s.start();
  const d = (await off.startRendering()).getChannelData(0);
  const fin = Math.round((o.fin ?? 5) / 1000 * outRate), fout = Math.round((o.fout ?? 20) / 1000 * outRate);
  for (let i = 0; i < fin && i < d.length; i++) d[i] *= i / fin;
  for (let i = 0; i < fout && i < d.length; i++) d[d.length - 1 - i] *= i / fout;
  let peak = 0; for (const x of d) peak = Math.max(peak, Math.abs(x));
  const g = (o.peak ?? 0.89) / (peak || 1);
  const pcm = new Int16Array(d.length); for (let i = 0; i < d.length; i++) pcm[i] = Math.max(-32768, Math.min(32767, Math.round(d[i] * g * 32767)));
  let bin = ""; const bytes = new Uint8Array(pcm.buffer); for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  return { b64: btoa(bin), rate: outRate, secs: d.length / outRate, peakBefore: peak };
})()` });
if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
const { b64, rate, secs, peakBefore } = r.result.result.value;
const pcm = Buffer.from(b64, "base64");
const h = Buffer.alloc(44);
h.write("RIFF", 0); h.writeUInt32LE(36 + pcm.length, 4); h.write("WAVE", 8); h.write("fmt ", 12);
h.writeUInt32LE(16, 16); h.writeUInt16LE(1, 20); h.writeUInt16LE(1, 22); h.writeUInt32LE(rate, 24);
h.writeUInt32LE(rate * 2, 28); h.writeUInt16LE(2, 32); h.writeUInt16LE(16, 34); h.write("data", 36); h.writeUInt32LE(pcm.length, 40);
writeFileSync(outPath, Buffer.concat([h, pcm]));
console.log(`${outPath}: ${secs.toFixed(2)}s @${rate} (${(pcm.length / 1024).toFixed(0)} KB, source peak ${peakBefore.toFixed(2)})`);
ws.close();
