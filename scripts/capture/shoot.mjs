// Drive headless Chrome over CDP: load the game, optionally click/type, screenshot.
// Usage: node shoot.mjs out.png [steps...]
//   steps: wait:SECONDS | click:X,Y | type:TEXT | key:KEYNAME | size:W,H (the viewport, in CSS px)
//          down:KEY / up:KEY (hold a key) | mdown:X,Y / mup:X,Y (hold the left button)
//          throttle:N (the tab's CPU N times slower, as on a loaded machine; 1 lifts it)
// CDP_PORT=9223 drives a Chrome started on that port instead (a second player).
// Chrome must already be running with --remote-debugging-port=9222.
import { writeFileSync } from "node:fs";

const [, , out, ...steps] = process.argv;
const port = process.env.CDP_PORT ?? 9222; // another Chrome, for a second player
const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = list.find((t) => t.type === "page");
if (!page) throw new Error("no page target");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });

let seq = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
const send = (method, params = {}) =>
  new Promise((r) => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const sleep = (s) => new Promise((r) => setTimeout(r, s * 1000));
let clip = null;

await send("Page.enable");
await send("Runtime.enable");
const logs = [];
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.method === "Runtime.consoleAPICalled") {
    const text = (m.params.args ?? []).map((a) => a.value ?? "").join(" ");
    if (text) logs.push(text);
  }
});
for (const step of steps) {
  const [cmd, rest] = [step.slice(0, step.indexOf(":")), step.slice(step.indexOf(":") + 1)];
  if (cmd === "wait") await sleep(Number(rest));
  else if (cmd === "throttle") await send("Emulation.setCPUThrottlingRate", { rate: Number(rest) }); // N times slower, like a loaded machine
  else if (cmd === "size") {
    const [width, height] = rest.split(",").map(Number);
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  }
  else if (cmd === "nav") await send("Page.navigate", { url: rest });
  else if (cmd === "move") {
    const [x, y] = rest.split(",").map(Number);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  } else if (cmd === "press") {
    const code = /^[a-z]$/.test(rest) ? "Key" + rest.toUpperCase() : rest;
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: rest, code, text: rest });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: rest, code });
  }
  else if (cmd === "down" || cmd === "up") {
    const code = /^[a-z]$/.test(rest) ? "Key" + rest.toUpperCase() : rest;
    await send("Input.dispatchKeyEvent", { type: cmd === "down" ? "keyDown" : "keyUp", key: rest, code, text: cmd === "down" ? rest : undefined });
  } else if (cmd === "mdown" || cmd === "mup") {
    const [x, y] = rest.split(",").map(Number);
    await send("Input.dispatchMouseEvent", { type: cmd === "mdown" ? "mousePressed" : "mouseReleased", x, y, button: "left", buttons: cmd === "mdown" ? 1 : 0, clickCount: 1 });
  } else if (cmd === "click") {
    const [x, y] = rest.split(",").map(Number);
    for (const type of ["mousePressed", "mouseReleased"])
      await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
  } else if (cmd === "preload") {
    // JS that runs in every new document before the page's own scripts (e.g. to tap audio).
    await send("Page.addScriptToEvaluateOnNewDocument", { source: rest });
  } else if (cmd === "eval") {
    const r = await send("Runtime.evaluate", { expression: rest, returnByValue: true });
    console.log("eval:", JSON.stringify(r.result?.result?.value ?? r.result));
  } else if (cmd === "scroll") {
    const [x, y, dy] = rest.split(",").map(Number);
    await send("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY: dy });
  } else if (cmd === "rclick") {
    const [x, y] = rest.split(",").map(Number);
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "right", buttons: 2, clickCount: 1 });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "right", buttons: 0, clickCount: 1 });
  } else if (cmd === "clip") {
    // Later `shot`s capture only this x,y,w,h of the page (as jpeg: quicker, for bursts).
    const [x, y, width, height] = rest.split(",").map(Number);
    clip = { x, y, width, height, scale: 1 };
  } else if (cmd === "shot") {
    // An extra screenshot mid-sequence, to PATH.
    const s = await send("Page.captureScreenshot", clip ? { format: "jpeg", quality: 75, clip } : { format: "png" });
    writeFileSync(rest, Buffer.from(s.result.data, "base64"));
  } else if (cmd === "type") await send("Input.insertText", { text: rest });
  else if (cmd === "key") {
    const codes = { Enter: 13, Escape: 27, Tab: 9 };
    for (const type of ["rawKeyDown", "keyUp"])
      await send("Input.dispatchKeyEvent", { type, key: rest, code: rest, windowsVirtualKeyCode: codes[rest] ?? 0, text: rest === "Enter" ? "\r" : undefined });
  }
}
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync(out, Buffer.from(shot.result.data, "base64"));
console.log("saved", out);
const wanted = process.env.GREP_LOGS;
if (wanted) {
  const re = new RegExp(wanted);
  for (const line of logs.filter((l) => re.test(l)).slice(-120)) console.log(line);
}
ws.close();
