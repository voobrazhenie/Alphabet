import http from 'node:http';
import { attachHub } from '../lib/hub.js';

// Vercel routes /api/ws to this file and hands the raw HTTP server the upgrade
// request, so the ws server accepts any path it receives here.
const server = http.createServer((req, res) => {
  res.writeHead(426, { 'content-type': 'text/plain' });
  res.end('Expected a WebSocket upgrade request');
});

attachHub(server);

export default server;
