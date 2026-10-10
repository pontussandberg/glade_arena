// Minimal no-cache static server for client/web (no dependencies).
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../client/web/', import.meta.url));
const port = Number(process.env.PORT ?? 8080);
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.txt': 'text/plain' };

createServer(async (req, res) => {
  let path;
  try {
    path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^[\\/]+/, '');
  } catch {
    // A malformed URL (e.g. a lone %) would otherwise throw out of here and stop the server.
    return res.writeHead(400).end('bad request');
  }
  if (path.startsWith('..')) return res.writeHead(403).end();
  try {
    const body = await readFile(join(root, path || 'index.html'));
    res.writeHead(200, {
      'content-type': types[extname(path || 'index.html')] ?? 'application/octet-stream',
      'cache-control': 'no-store',
      // Lets the page's loader show how far along the (large, in dev) wasm download is.
      'content-length': body.length,
    });
    res.end(body);
  } catch {
    res.writeHead(404).end('not found');
  }
}).listen(port, () => console.log(`serving client/web on http://localhost:${port}`));
