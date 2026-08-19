import { randomUUID } from 'node:crypto';
import { WebSocketServer } from 'ws';

// Short id for this server instance. On Vercel each function instance gets its
// own, which is how the pages can tell whether they landed on the same one.
const INSTANCE = Math.random().toString(16).slice(2, 6);

const CELL_COUNT = 8;
const SEED = 'ABCDEFGH';
const LETTER = /^[A-Z]$/;

export function attachHub(server, { path } = {}) {
  const wss = new WebSocketServer(path ? { server, path } : { server });

  // Authoritative shared state. Memory only: a restart reseeds it.
  const cells = Array.from({ length: CELL_COUNT }, (_, index) => ({
    index,
    letter: SEED[index],
    owner: null,
  }));

  const send = (ws, msg) => {
    if (ws.readyState === ws.OPEN) ws.send(JSON.stringify(msg));
  };

  // Ownership is sent as a relationship, never as an id.
  const viewFor = (ws) =>
    cells.map(({ index, letter, owner }) => ({
      index,
      letter,
      owner: owner === null ? null : owner === ws.id ? 'me' : 'other',
    }));

  const counts = () => {
    let screens = 0;
    let controllers = 0;
    for (const client of wss.clients) {
      if (client.role === 'screen') screens++;
      else if (client.role === 'controller') controllers++;
    }
    return { screens, controllers };
  };

  const broadcast = (type) => {
    const peers = counts();
    for (const client of wss.clients) {
      send(client, { type, cells: viewFor(client), ...peers, instance: INSTANCE });
    }
  };

  // One cell per participant: claiming a new one gives up the old one.
  const releaseOwnedBy = (id) => {
    let changed = false;
    for (const cell of cells) {
      if (cell.owner === id) {
        cell.owner = null;
        changed = true;
      }
    }
    return changed;
  };

  wss.on('connection', (ws) => {
    ws.id = randomUUID();
    ws.role = 'unknown';
    console.log(`[hub ${INSTANCE}] connect — clients: ${wss.clients.size}`);
    send(ws, { type: 'state:init', cells: viewFor(ws), ...counts(), instance: INSTANCE });

    ws.on('message', (data) => {
      let msg;
      try {
        msg = JSON.parse(data.toString());
      } catch {
        return;
      }
      if (!msg || typeof msg.type !== 'string') return;

      if (msg.type === 'hello') {
        ws.role = msg.role === 'screen' ? 'screen' : 'controller';
        console.log(`[hub ${INSTANCE}] hello role=${ws.role}`);
        broadcast('state');
        return;
      }

      const index = Number(msg.index);
      const cell = Number.isInteger(index) ? cells[index] : undefined;
      if (!cell) return;

      if (msg.type === 'cell:claim') {
        if (cell.owner !== null && cell.owner !== ws.id) {
          send(ws, { type: 'cell:denied', index, reason: 'taken' });
          return;
        }
        releaseOwnedBy(ws.id);
        cell.owner = ws.id;
        console.log(`[hub ${INSTANCE}] claim cell ${index}`);
        broadcast('state');
        return;
      }

      if (msg.type === 'cell:release') {
        if (cell.owner !== ws.id) return;
        cell.owner = null;
        console.log(`[hub ${INSTANCE}] release cell ${index}`);
        broadcast('state');
        return;
      }

      if (msg.type === 'cell:update') {
        if (cell.owner !== ws.id) {
          send(ws, { type: 'cell:denied', index, reason: 'not-owner' });
          return;
        }
        const letter = String(msg.letter ?? '').toUpperCase();
        if (!LETTER.test(letter)) return;
        cell.letter = letter;
        console.log(`[hub ${INSTANCE}] cell ${index} = ${letter}`);
        broadcast('state');
      }
    });

    ws.on('error', (err) => {
      console.log(`[hub ${INSTANCE}] socket error: ${err.message}`);
    });

    ws.on('close', () => {
      const freed = releaseOwnedBy(ws.id);
      console.log(
        `[hub ${INSTANCE}] disconnect role=${ws.role} freed=${freed} — clients: ${wss.clients.size}`,
      );
      broadcast('state');
    });
  });

  wss.on('error', (err) => {
    console.log(`[hub ${INSTANCE}] server error: ${err.message}`);
  });

  return wss;
}
