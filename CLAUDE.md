# Working notes

## How to answer

- **Short and plain.** Keep replies brief and non-technical unless a longer or
  deeper answer is asked for. Simplify: say what it means and what to do, not
  how it works underneath.
- Skip jargon, error codes, and internals unless they change what the user
  should do. If something is only interesting mechanically, leave it out.
- End every activity sequence with ✅✅✅.

## What this project is

`neurons.html` is a single self-contained page: a ray-marched WebGL shader with
three objects (Brain, Neuron, LiquidChrome) and a control console. No build
step, no dependencies — everything is inline in the one file.

Two copies are kept in sync and both are pushed on every change:

| Where | File | Branch |
| --- | --- | --- |
| `voobrazhenie/Alphabet` | `neurons.html` | `claude/ray-marching-neurons-ggq1wg` |
| `voobrazhenie/liquidchrome` | `index.html` | `main` (published at https://voobrazhenie.github.io/liquidchrome/) |

The page is also published as a Claude artifact at the same URL each time.

`REQUIREMENTS.md` in this repository is the written spec: what every control,
key, mode and the benchmark actually do. Read it before changing behaviour, and
update it in the same commit when behaviour changes.

## Known, not a bug

- **Artifact "403 / not watching"** — after each publish a notice says the
  artifact watch was not registered. It only means this chat will not be woken
  automatically if the page is edited somewhere else. The page itself is fine
  and nothing needs fixing. No need to mention it again unless asked.
- **No browser control** — this session runs in an isolated cloud container
  with its own headless browser. It cannot see or drive the user's real
  browser, so anything behind a login (Firebase console, dashboards) has to be
  done by the user, who then pastes the result here.
- **No GPU here** — the container renders in software, so frame rates measured
  in this session are meaningless. Correctness can be verified; speed cannot.
  Say so rather than quoting numbers.

## Verifying changes

Headless Chromium with SwiftShader, driven by Playwright:

```
node <script>.mjs   # args: --use-gl=angle --use-angle=swiftshader --enable-unsafe-swiftshader
```

Always check: all three objects still compile and render, and no page errors.
