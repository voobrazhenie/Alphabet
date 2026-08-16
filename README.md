# Alphabet — realtime proof of concept

Phone → cloud WebSocket server → screen. A phone opens `/controller.html`, a laptop opens
`/screen.html`, and the phone drives a white circle on the black screen page in realtime.

This is a disposable technical test, not production architecture. No database, no accounts,
nothing stored.

## What's here

| File | What it is |
| --- | --- |
| `screen.html` | Fullscreen black page with one white circle |
| `controller.html` | Mobile page: connection status, PULSE button, 0–1 slider |
| `api/ws.js` | The WebSocket server, deployed as a Vercel Function at `/api/ws` |
| `lib/hub.js` | The ~70 lines that actually do the work: track clients, broadcast to screens |
| `server.js` | Local dev server (static files + the same hub), not used in production |

Messages are plain JSON:

```json
{ "type": "hello", "role": "screen" }
{ "type": "pulse" }
{ "type": "slider", "value": 0.72 }
```

The server sends back `{ "type": "peers", "screens": 1, "controllers": 2, "instance": "a3f1" }`
so each page can show how many others are connected, and which server instance it landed on.

## Run locally

```bash
npm install
node server.js
```

Then open http://localhost:3000/screen.html and http://localhost:3000/controller.html.
To use a phone on the same wifi, open `http://<your-laptop-ip>:3000/controller.html`.

## Test the deployed version

1. Open `/screen.html` on the computer.
2. Open `/controller.html` on the phone (mobile data is fine — it goes through the cloud).
3. Press PULSE: the circle flashes.
4. Drag the slider: the circle resizes continuously.
5. Open more controllers — they all drive the same screen.

The controller shows `screens: N`. If it says `screens: 0` while the screen page is open, see
the second caveat below.

## Two caveats (Vercel-specific, not bugs in this code)

**Connections drop every 5 minutes.** Vercel Functions have a 300-second maximum duration on
the free plan, so the WebSocket closes when it's reached. Both pages reconnect automatically
with backoff, so you'll see a brief "Disconnected" blink and it carries on.

**Clients can land on different server instances.** Vercel pins a WebSocket connection to one
function instance, and a new connection isn't guaranteed to reach the same one
(https://vercel.com/docs/functions/websockets — "Manage persistent state"). If the phone and the
screen end up on different instances, they can't see each other. The `screens: N` counter on the
controller makes this visible: if it reads 0 while the screen is open, reload both pages.

With a handful of connections this is rare — Fluid compute puts many connections on one
instance. Making it impossible requires an external store (Vercel's own realtime guides use
Redis for cross-instance fan-out), which is deliberately out of scope here.

## Requirements on the Vercel side

WebSockets need **Fluid compute** enabled (default for projects created after April 2025) and
the WebSocket public beta available on the account. `vercel.json` sets the function's max
duration to 300s.
