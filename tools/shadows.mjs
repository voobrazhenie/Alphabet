// Shadows on the page, checked in a browser because there is no GPU here.
//
// Three things have to hold, and only the first two can be checked without eyes:
//   * every object still compiles and draws, with no page errors
//   * with shadows OFF the image is byte-identical to the baseline — the whole
//     no-op discipline, and what keeps parity.mjs green
//   * with shadows ON at full darkness the image changes, reaches real black, and
//     still matches its own stored shot — the march is easy to make subtly wrong
//     and the difference is a shape, not a number
//   * the left drag aims the sun, the right drag always turns the camera, and
//     L swaps the left one back and forth between them
//
// Usage: node shadows.mjs base   -> write baseline shots
//        node shadows.mjs check  -> compare against them
import { createRequire } from "node:module";
import { inflateSync } from "node:zlib";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
// an ESM import cannot see NODE_PATH, so playwright comes in the old way
const require = createRequire(import.meta.url);
const { chromium } = require("playwright");

const here = dirname(fileURLToPath(import.meta.url));
const page_url = "file://" + join(here, "..", "neurons.html");
const OUT = join(here, "..", "pc", "target", "shadows");  // gitignored already
const mode = process.argv[2] || "check";
const W = 360, H = 240;

const OBJECTS = [
  { name: "brain", scene: 0 },
  { name: "neuron", scene: 1 },
  { name: "chrome", scene: 2 },
];

// The page animates, so every shot has to be taken at the same pose. Freezing the
// clocks is not enough: the frame-time controller moves the render scale and the
// march budget on its own, and a software rasteriser is slow enough to trip it. Pin
// both, or two runs of the same frame are not the same frame.
async function pose(page, over) {
  await page.evaluate((o) => {
    const s = window.__state;
    s.running = false; s.morph = false;
    s.clock = 12.0; s.mClock = 40.0; s.pClock = 6.0;
    s.fly = false; s.mode = 0; s.dragAz = 0.6; s.dragEl = 0.25; s.zoom = 1.0;
    // A software rasteriser is slow enough that even a pinned resolution gives way
    // at 250 ms, and the ramp down takes different numbers of frames for two
    // shaders of different cost. 0.30 is the controller's floor and so its one
    // fixed point: it will not go lower, and it can only climb at 13 ms a frame,
    // which will not happen here. Start there and nothing moves.
    s.resPin = 1; s.scaleQ = 0.30; s.stepsPin = true; s.steps = 64;
    s.aa = 0;
    s.velAz = 0; s.velEl = 0;        // a drag leaves inertia behind; the next one starts still
    Object.assign(s, o);
  }, over);
  // Software rendering runs at a few frames a second, so waiting on the clock is
  // waiting on almost nothing. Wait on drawn frames instead.
  await page.evaluate(() => new Promise((done) => {
    let n = 0;
    const tick = () => (++n < 8 ? requestAnimationFrame(tick) : done());
    requestAnimationFrame(tick);
  }));
}

// A WebGL canvas is cleared once it has been composited, so drawImage gives back
// nothing. The screenshot is the only honest copy of the pixels — decode that.
function decode(png) {
  let i = 8, w = 0, h = 0, idat = [];
  while (i < png.length) {
    const len = png.readUInt32BE(i);
    const type = png.toString("ascii", i + 4, i + 8);
    if (type === "IHDR") { w = png.readUInt32BE(i + 8); h = png.readUInt32BE(i + 12); }
    if (type === "IDAT") idat.push(png.subarray(i + 8, i + 8 + len));
    i += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const bpp = 4, stride = w * bpp;
  const out = Buffer.alloc(h * stride);
  let pos = 0;
  for (let y = 0; y < h; y++) {
    const f = raw[pos++];
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? out[y * stride + x - bpp] : 0;
      const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= bpp && y > 0 ? out[(y - 1) * stride + x - bpp] : 0;
      let v = raw[pos++];
      if (f === 1) v += a;
      else if (f === 2) v += b;
      else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) {
        const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[y * stride + x] = v & 255;
    }
  }
  return { w, h, data: out };
}

function darkFraction(png) {
  const { data } = decode(png);
  let n = 0, total = 0;
  for (let i = 0; i < data.length; i += 4) {
    total++;
    if (data[i] < 12 && data[i + 1] < 12 && data[i + 2] < 12) n++;
  }
  return n / total;
}

// Switching object or resolution costs a few warm-up frames at a different size,
// and a screenshot taken during them is not the picture. Shoot until two in a row
// agree, and that one is settled.
async function shot(page, name) {
  let prev = null;
  for (let i = 0; i < 12; i++) {
    const buf = await page.locator("canvas").first().screenshot();
    if (prev && prev.equals(buf)) { prev = buf; break; }
    prev = buf;
    await page.evaluate(() => new Promise((done) => {
      let n = 0;
      const tick = () => (++n < 4 ? requestAnimationFrame(tick) : done());
      requestAnimationFrame(tick);
    }));
  }
  const buf = prev;
  writeFileSync(join(OUT, name + ".png"), buf);
  return buf;
}

const run = async () => {
  mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch({
    args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
  });
  const page = await browser.newPage({ viewport: { width: W, height: H } });
  const errors = [];
  // the page looks for its settings in the cloud and fails soft when there is no
  // network, which is the normal state of this container — not a fault
  const ours = (t) => !/ERR_|net::|Failed to load resource|firestore|googleapis/i.test(t);
  page.on("pageerror", (e) => { if (ours(String(e))) errors.push(String(e)); });
  page.on("console", (m) => { if (m.type() === "error" && ours(m.text())) errors.push(m.text()); });

  await page.goto(page_url);
  await page.waitForTimeout(1200);
  // the console is drawn over the canvas and an element screenshot picks it up —
  // live frame rates and all, which would never compare equal
  await page.keyboard.press("h");
  await page.waitForTimeout(300);

  let bad = 0;
  const say = (ok, msg) => { if (!ok) bad++; console.log((ok ? "ok  " : "FAIL") + " " + msg); };

  const hasState = await page.evaluate(() => !!window.__state);
  if (!hasState) {
    console.log("FAIL the page does not expose __state — the harness needs it");
    await browser.close();
    process.exit(1);
  }

  for (const o of OBJECTS) {
    await pose(page, { scene: o.scene, shadowOn: 0 });
    const off = await shot(page, o.name + "-off");
    const size = await page.evaluate(() => {
      const c = document.querySelector("canvas");
      return c.width + "x" + c.height;
    });
    say(errors.length === 0, `${o.name}: draws with no page errors (canvas ${size})`);

    const compare = (buf, file, what) => {
      const path = join(OUT, file);
      if (mode !== "check") {
        writeFileSync(path, buf);
        console.log(`--   ${o.name}: wrote ${file}`);
      } else if (existsSync(path)) {
        say(readFileSync(path).equals(buf), `${o.name}: ${what}`);
      } else {
        console.log(`--   ${o.name}: no ${what} to compare against`);
      }
    };
    compare(off, o.name + "-base.png", "shadows off is byte-identical to the baseline");

    // Hard, full strength, and deliberately low: a sun near the horizon is what
    // makes shadow rays leave at a shallow angle, which is the whole difficulty.
    await pose(page, {
      scene: o.scene, shadowOn: 1, shadowSoft: 0.0, shadowDark: 1.0,
      shadowReach: 4.0, sunAz: 2.2, sunEl: 0.35,
    });
    const on = await shot(page, o.name + "-on");
    say(!on.equals(off), `${o.name}: shadows on changes the image`);
    compare(on, o.name + "-on-base.png", "the shadows themselves are the stored ones");

    const dark = darkFraction(on);
    say(dark > 0.02, `${o.name}: reaches real black (${(dark * 100).toFixed(1)}% of pixels)`);
  }

  if (mode === "check") {
    // Who owns which button. The sun has the left drag from the start, the camera
    // always has the right one, and L swaps the left one back and forth.
    await pose(page, { scene: 2, shadowOn: 1 });
    const box = await page.locator("canvas").first().boundingBox();
    const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
    const look = () => page.evaluate(() => {
      const s = window.__state;
      return { az: s.sunAz, el: s.sunEl, dAz: s.dragAz, dEl: s.dragEl };
    });
    const drag = async (button) => {
      await page.mouse.move(cx, cy);
      await page.mouse.down({ button });
      await page.mouse.move(cx + 90, cy + 40, { steps: 6 });
      await page.mouse.up({ button });
    };
    const inMode = () => page.evaluate(() => !!window.__sunMode);

    say(await inMode(), "sun mode is on out of the box");

    let a = await look(); await drag("left"); let b = await look();
    say(Math.abs(b.az - a.az) > 1e-3 && Math.abs(b.el - a.el) > 1e-3,
        "left-drag swings the sun");
    say(b.dAz === a.dAz && b.dEl === a.dEl, "and the camera is left exactly where it was");

    await pose(page, { scene: 2, shadowOn: 1 });
    a = await look(); await drag("right"); b = await look();
    say(b.dAz !== a.dAz && b.dEl !== a.dEl, "right-drag turns the camera even so");
    say(b.az === a.az && b.el === a.el, "and the sun stays where it was put");

    await page.keyboard.press("l");
    say(!(await inMode()), "L hands the left button back to the camera");
    await pose(page, { scene: 2, shadowOn: 1 });
    a = await look(); await drag("left"); b = await look();
    say(b.dAz !== a.dAz, "left-drag turns the camera once the sun has let go");
    say(b.az === a.az && b.el === a.el, "and the sun does not move with it");
    await page.keyboard.press("l");
    say(await inMode(), "L takes it back again");
  }

  if (errors.length) {
    console.log("\npage errors:");
    for (const e of errors.slice(0, 6)) console.log("  " + e);
    bad++;
  }
  await browser.close();
  console.log(bad ? `\n${bad} failed` : "\nall good");
  process.exit(bad ? 1 : 0);
};
run();
