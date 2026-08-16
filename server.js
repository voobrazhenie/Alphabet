// Local dev only: serves the static pages and runs the same hub at /api/ws.
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, resolve } from 'node:path';
import { attachHub } from './lib/hub.js';

const ROOT = resolve(import.meta.dirname);
const PORT = Number(process.env.PORT ?? 3000);
const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
};

const server = http.createServer(async (req, res) => {
  const { pathname } = new URL(req.url, `http://localhost:${PORT}`);
  const file = resolve(ROOT, '.' + (pathname === '/' ? '/index.html' : pathname));

  if (!file.startsWith(ROOT)) {
    res.writeHead(403).end('Forbidden');
    return;
  }

  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404).end('Not found');
  }
});

attachHub(server, { path: '/api/ws' });

server.listen(PORT, () => {
  console.log(`http://localhost:${PORT} — screen: /screen.html, controller: /controller.html`);
});
