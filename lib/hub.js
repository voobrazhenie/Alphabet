import { WebSocketServer } from 'ws';

// Short id for this server instance. On Vercel each function instance gets its
// own, which is how the pages can tell whether they landed on the same one.
const INSTANCE = Math.random().toString(16).slice(2, 6);

export function attachHub(server, { path } = {}) {
  const wss = new WebSocketServer(path ? { server, path } : { server });

  const send = (ws, msg) => {
    if (ws.readyState === ws.OPEN) ws.send(JSON.stringify(msg));
  };

  const counts = () => {
    let screens = 0;
    let controllers = 0;
    for (const client of wss.clients) {
      if (client.role === 'screen') screens++;
      else if (client.role === 'controller') controllers++;
    }
    return { screens, controllers };
  };

  const announce = () => {
    const msg = { type: 'peers', ...counts(), instance: INSTANCE };
    for (const client of wss.clients) send(client, msg);
  };

  const toScreens = (msg) => {
    for (const client of wss.clients) {
      if (client.role === 'screen') send(client, msg);
    }
  };

  wss.on('connection', (ws) => {
    ws.role = 'unknown';
    console.log(`[hub ${INSTANCE}] connect — clients: ${wss.clients.size}`);
    send(ws, { type: 'peers', ...counts(), instance: INSTANCE });

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
        announce();
        return;
      }

      if (msg.type === 'pulse') {
        toScreens({ type: 'pulse' });
        return;
      }

      if (msg.type === 'slider') {
        const value = Number(msg.value);
        if (!Number.isFinite(value)) return;
        toScreens({ type: 'slider', value: Math.min(1, Math.max(0, value)) });
      }
    });

    ws.on('error', (err) => {
      console.log(`[hub ${INSTANCE}] socket error: ${err.message}`);
    });

    ws.on('close', () => {
      console.log(`[hub ${INSTANCE}] disconnect role=${ws.role} — clients: ${wss.clients.size}`);
      announce();
    });
  });

  wss.on('error', (err) => {
    console.log(`[hub ${INSTANCE}] server error: ${err.message}`);
  });

  return wss;
}
