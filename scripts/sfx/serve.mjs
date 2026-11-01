// Static file server over the sound sources, the rendered clips and the game's sfx folder, so
// headless Chrome can fetch and decode recordings: node serve.mjs [port] (default 8099).
// Sources default to %LOCALAPPDATA%\Temprena-sfx\{src,out}; SFX_SRC / SFX_OUT override.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { join, extname } from "node:path";
import { fileURLToPath } from "node:url";
const roots = {
  "/src/": (process.env.SFX_SRC ?? "C:/Users/PONTUS~1/AppData/Local/Temp/arena-sfx/src") + "/",
  "/out/": (process.env.SFX_OUT ?? "C:/Users/PONTUS~1/AppData/Local/Temp/arena-sfx/out") + "/",
  "/game/": fileURLToPath(new URL("../../client/web/sfx/", import.meta.url)),
};
const types = { ".wav": "audio/wav", ".ogg": "audio/ogg", ".flac": "audio/flac", ".mp3": "audio/mpeg", ".html": "text/html" };
createServer(async (req, res) => {
  const url = decodeURIComponent(req.url.split("?")[0]);
  if (url === "/") { res.writeHead(200, { "content-type": "text/html" }); return res.end("<!doctype html><title>sfx</title>probe"); }
  const prefix = Object.keys(roots).find((p) => url.startsWith(p));
  if (!prefix) { res.writeHead(404); return res.end(); }
  const path = join(roots[prefix], url.slice(prefix.length));
  try {
    await stat(path);
    res.writeHead(200, { "content-type": types[extname(path).toLowerCase()] ?? "application/octet-stream" });
    res.end(await readFile(path));
  } catch { res.writeHead(404); res.end(); }
}).listen(Number(process.argv[2] ?? 8099), "127.0.0.1");
