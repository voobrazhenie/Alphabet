# Working notes

## How to answer

- **Short and plain.** Keep replies brief and non-technical unless a longer or
  deeper answer is asked for. Simplify: say what it means and what to do, not
  how it works underneath.
- Skip jargon, error codes, and internals unless they change what the user
  should do. If something is only interesting mechanically, leave it out.
- End every activity sequence with ✅✅✅.

## What is being worked on

**The desktop app (`pc/`) is the active surface.** The web page is not a priority:
leave `neurons.html` alone unless asked for it directly. Native-only work — MIDI, the
camera group, how a frame is sized — does not need a matching page change, and the
"two copies in sync" rule below is about the page, not about `pc/`.

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
  Say so rather than quoting numbers. The native app *can* be run headless on
  software Vulkan to check the console renders — `pc/NOTES.md` has the recipe — but
  only in a debug build, and never for speed.
- **No MIDI hardware here** — the mapping arithmetic is unit tested, the device
  layer cannot be. It is Windows-only code and the container has no controller.

## Verifying changes

Headless Chromium with SwiftShader, driven by Playwright:

```
NODE_PATH=/opt/node22/lib/node_modules node tools/shadows.mjs check
```

It draws all three objects, checks there are no page errors, and — the part that
matters — checks that the sun switched off is **byte-identical** to a stored
baseline. `... shadows.mjs base` writes that baseline, and it has to be written from
a build without the change being tested.

Three traps it already works around, worth knowing before writing another one of
these: a WebGL canvas is blank to `drawImage` once composited, so pixels have to
come from the screenshot; the frame-time controller keeps moving the render
scale, so the pose pins it to 0.30 — the controller's floor, and the only value it
will sit still at under software rendering; and Chromium hands back an opaque
canvas as **RGB, not RGBA**, so a decoder that assumes four bytes per pixel turns
the picture into noise that still looks like a picture. Every number it reports
was wrong for months because of that one. Read the colour type from the header.

A screenshot is also of the canvas at its CSS size, not its backing store, so
counting individual pixels means making the two match first — otherwise the
browser's resampling is what gets counted.

Always check: all three objects still compile and render, and no page errors.

## The native build

`pc/` is a separate program: the same effect as a Windows app (Rust + wgpu), with
Vulkan / DirectX 12 / OpenGL as a live switch, the same keys and the same benchmark.
It is not part of the page and is not published anywhere — a GitHub Actions run
builds the .exe. `pc/README.md` says how it differs.

**Read `pc/NOTES.md` before changing anything in `pc/`.** It carries what is not
obvious from the code: how to check a change without a GPU, the decisions that look
arbitrary and are not (the y flip, the non-sRGB surface, the loop that must not be
unrollable), the pinned crate versions and their traps, and what has never been run
on real hardware yet.

The shader there is a port of the page's; `pc/tools/parity.mjs` renders both in
headless Chromium and compares them pixel for pixel. Keep it passing when either
shader changes.
