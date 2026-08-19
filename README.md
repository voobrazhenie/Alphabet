# Alphabet — collaborative letter sequencer

Eight cells hold one letter each (A–Z). The same eight cells are the eight steps of a sequencer:
`1 → 2 → … → 8 → repeat`. When a step becomes active, the letter in that cell decides the pitch.

Participants open the controller on their phones, claim a cell, and change its letter live. The
display plays the loop and shows the active step.

Vanilla HTML/CSS/JS, one Node WebSocket server, deployed on Vercel from GitHub. No framework,
no database, no accounts.

## Files

| File | Responsibility |
| --- | --- |
| `lib/hub.js` | Backend. Authoritative grid: 8 cells, their letters, and who owns each. Validation, locking, broadcasting |
| `api/ws.js` | Vercel Function entrypoint — the hub as a WebSocket endpoint at `/api/ws` |
| `server.js` | Local dev server: static files + the same hub |
| `screen.html` | Display. Grid rendering, sequencer clock, play/pause, tempo, octave, audio |
| `controller.html` | Phone. Claim/release a cell, pick a letter, see ownership |
| `letters.js` | Letter → note mapping, isolated so it can be swapped |

## Socket protocol

Client → server:

```json
{ "type": "hello", "role": "screen" }
{ "type": "cell:claim",   "index": 2 }
{ "type": "cell:release", "index": 2 }
{ "type": "cell:update",  "index": 2, "letter": "X" }
```

Server → client:

```json
{ "type": "state:init", "cells": [ … ], "screens": 1, "controllers": 2, "instance": "a3f1" }
{ "type": "state",      "cells": [ … ], … }
{ "type": "cell:denied", "index": 2, "reason": "taken" }
```

Each cell arrives as `{ index, letter, owner }` where `owner` is `null`, `"me"`, or `"other"` —
resolved per recipient, so internal client ids never leave the server.

Messages are sent only when shared state actually changes. Sequencer ticks, tempo and play/pause
never touch the socket: the clock runs entirely in the display page.

## Ownership rules (enforced on the server)

- A claim succeeds only if the cell is free; otherwise the requester gets `cell:denied`.
- One cell per participant — claiming a new cell releases the previous one.
- `cell:update` is applied only if the requesting socket owns that cell, and the letter matches
  `/^[A-Z]$/`.
- A cell is released when the participant releases it explicitly or when the socket closes.
  There is no inactivity timeout; disconnect handling covers it.

## Letter → note

`letters.js`. Chromatic from C3: A = C3, B = C#3, C = D3 … Z = 25 semitones above C3
(`BASE_MIDI = 48`). Change that file to retune the piece — nothing else depends on the mapping.

The display has an octave switch (– / + buttons, or the arrow up/down keys) covering -2 to +3
octaves, shown next to the tempo. It transposes playback and the note labels only; like tempo and
play/pause it is display-local and never crosses the socket. The status line also names the
server worker the page is connected to.

The brief specified A = C1; that lands around 33 Hz, inaudible on laptop and phone speakers, so
the base was moved up two octaves.

## Run locally

```bash
npm install
node server.js
```

Display: http://localhost:3000/screen.html — Controller: http://localhost:3000/controller.html
From a phone on the same wifi: `http://<your-laptop-ip>:3000/controller.html`.

Audio starts on the first Play click (browsers block sound before a user gesture).

## Test the deployed version

1. Open `/screen.html` on the computer, press PLAY — the highlight walks 1…8 and loops, one note
   per step.
2. Open `/controller.html` on a phone. Tap a free cell, pick a letter — the display changes
   immediately and the new pitch is heard on the next pass.
3. Open a second controller: the first phone's cell shows as taken and cannot be tapped.
4. Close one controller — its cell becomes free for everyone else.

## Limitations

**Instance pinning.** Vercel pins each WebSocket connection to one function instance, and a new
connection is not guaranteed to reach the same one
(https://vercel.com/docs/functions/websockets — "Manage persistent state"). Clients on different
instances would see different grids. Both pages print a short server id in their status line: if
the display and a phone show different ids, reload. A proper fix needs a shared store (Redis),
deliberately out of scope here.

**5-minute connections.** Function max duration on the free plan is 300 s, so sockets close and
reconnect. A participant's cell is freed on that drop and can be claimed by someone else.

**Memory-only state.** A redeploy or cold start reseeds the grid to A B C D E F G H.
