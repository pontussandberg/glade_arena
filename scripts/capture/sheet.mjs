// Lay out images as a labelled contact sheet and screenshot it in a fresh headless-Chrome tab.
// Usage: node sheet.mjs out.png COLS SCALE img1 img2 ...   (image paths as Windows paths)
import { writeFileSync, unlinkSync } from "node:fs";
import { basename } from "node:path";

const [, , out, colsArg, scaleArg, ...images] = process.argv;
const cols = Number(colsArg);
const scale = Number(scaleArg);
const cells = images.map((p, i) => `<figure><img src="file:///${p.replace(/\\/g, "/")}" style="width:${Math.round(640 * scale)}px"><figcaption>${i + 1} ${basename(p)}</figcaption></figure>`).join("");
const html = `<!doctype html><meta charset="utf-8"><style>body{margin:0;background:#222;color:#eee;font:12px monospace}main{display:grid;grid-template-columns:repeat(${cols},max-content);gap:4px}figure{margin:0}figcaption{padding:1px 2px}</style><main>${cells}</main>`;
const page = out.replace(/\.png$/, ".html");
writeFileSync(page, html);

const list = await (await fetch("http://127.0.0.1:9222/json/version")).json();
const ws = new WebSocket(list.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let seq = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
const send = (method, params = {}, sessionId) =>
  new Promise((r, j) => {
    const id = ++seq;
    pending.set(id, (m) => (m.error ? j(new Error(`${method}: ${JSON.stringify(m.error)}`)) : r(m)));
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });
const sleep = (s) => new Promise((r) => setTimeout(r, s * 1000));

const url = "file:///" + page.replace(/\\/g, "/");
const { result: { targetId } } = await send("Target.createTarget", { url, width: 1600, height: 900, newWindow: true });
const { result: { sessionId } } = await send("Target.attachToTarget", { targetId, flatten: true });
await send("Page.enable", {}, sessionId);
await sleep(1.5);
const { result: { cssContentSize } } = await send("Page.getLayoutMetrics", {}, sessionId);
const width = Math.ceil(cssContentSize.width), height = Math.ceil(cssContentSize.height);
await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false }, sessionId);
await sleep(0.5);
const shot = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true, clip: { x: 0, y: 0, width, height, scale: 1 } }, sessionId);
writeFileSync(out, Buffer.from(shot.result.data, "base64"));
await send("Target.closeTarget", { targetId });
unlinkSync(page);
console.log("saved", out, `${width}x${height}`);
ws.close();
