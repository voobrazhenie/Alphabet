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
    s.matcapOn = 0; s.matcap = 0;    // every pose starts from the lit material
    s.shadowSrc = 0; s.shadowLevel = 0.5; s.shadowContrast = 0; s.shadowMc = 0;
    // A pose that leaves the geometry as the last one left it is not a pose. The
    // cube rig turns build steps off and the warp down, and the next case along
    // then quietly compares a different object with the baseline.
    s.lcOn = [1, 1, 1, 1, 1, 0]; s.warpOn = [1, 0, 0]; s.warpAmt = 0.74;
    // The film grain has a control now and it is off by default. The stored
    // baselines were taken before it existed, so the pose turns it back on:
    // that keeps "the sun switched off is the page as it was" a live claim
    // about the shading rather than a claim about the grain.
    s.noise = 1.0; s.shadowOnly = 0;
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
  let i = 8, w = 0, h = 0, colour = 6, idat = [];
  while (i < png.length) {
    const len = png.readUInt32BE(i);
    const type = png.toString("ascii", i + 4, i + 8);
    // Chromium hands back an opaque canvas as RGB, not RGBA, so the stride is
    // three bytes and not four. Reading it as four decodes to noise — and noise
    // that still looks like a picture, which is how it went unnoticed.
    if (type === "IHDR") { w = png.readUInt32BE(i + 8); h = png.readUInt32BE(i + 12); colour = png[i + 17]; }
    if (type === "IDAT") idat.push(png.subarray(i + 8, i + 8 + len));
    i += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const bpp = { 0: 1, 2: 3, 4: 2, 6: 4 }[colour];
  if (!bpp) throw new Error("unsupported PNG colour type " + colour);
  const stride = w * bpp;
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
  return { w, h, bpp, data: out };
}

// How much of the frame the second shot took away from the first. Asking instead
// whether anything reaches pure black is asking the wrong question: film grain,
// the vignette and the glow are all added after the shading, so nothing in this
// picture is ever near zero, and a test for it passes or fails on the decoder
// rather than on the shadows.
function darkened(a, b, drop) {
  const A = decode(a), B = decode(b);
  const lum = (d, i) => 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
  let n = 0, total = 0;
  for (let i = 0; i + 2 < A.data.length; i += A.bpp) {
    total++;
    if (lum(A.data, i) - lum(B.data, i) > drop) n++;
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
  }

  // The grain, the lit/unlit view and standing where the sun does. None of the
  // three is subtle, so the useful thing to hold them to is that each does
  // something and that the last one gives the camera back untouched.
  if (mode === "check") {
    await pose(page, { scene: 2, shadowOn: 0 });
    const grainy = await shot(page, "noise-on");
    await pose(page, { scene: 2, shadowOn: 0, noise: 0.0 });
    say(!(await shot(page, "noise-off")).equals(grainy), "the noise slider changes the frame");

    await pose(page, { scene: 2, shadowOn: 1 });
    const normal = await shot(page, "sunview-off");
    const cam = () => page.evaluate(() => {
      const s = window.__state;
      return JSON.stringify([s.dragAz, s.dragEl, s.zoom, s.fly, s.mode]);
    });
    const before = await cam();
    await page.keyboard.press("NumpadMultiply");
    say(await page.evaluate(() => !!window.__sunView), "* stands the camera where the sun is");
    say(!(await shot(page, "sunview-on")).equals(normal), "and the view is not the one it left");
    await page.keyboard.press("NumpadMultiply");
    say(!(await page.evaluate(() => !!window.__sunView)), "* gives the camera back");
    say((await cam()) === before, "with nothing about it changed");

    await pose(page, { scene: 2, shadowOn: 1, shadowOnly: 1, shadowSrc: 2,
                       shadowContrast: 1.0, shadowLevel: 0.60 });
    say(!(await shot(page, "lit-unlit")).equals(normal), "the lit / unlit view draws the shadow itself");
  }

  // Three ways to arrive at a shadow. Only the first spends a ray; all three go
  // through the same shaping and the same application, so the useful thing to
  // hold them to is that they are genuinely three answers and not one answer
  // wearing three hats — and that the hard end of the shaping really is hard.
  if (mode === "check") {
    // relief so the two textureless sources have normals to work with, and the
    // cube so the marched one has something to actually cast
    const rig = {
      scene: 2, shadowOn: 1, shadowContrast: 1.0, sunAz: 0.9, sunEl: 0.90,
      warpOn: [0, 0, 0], warpAmt: 0, lcOn: [1, 1, 0, 0, 0, 1],
      fly: 0, mode: 0, dragAz: 0.0, dragEl: 0.50, zoom: 0.46,
    };
    const shots = [];
    for (const [name, over] of [
      ["ray march", { shadowSrc: 0, shadowLevel: 0.50 }],
      ["sphere",    { shadowSrc: 1, shadowLevel: 0.50, shadowMc: 0 }],
      ["sun angle", { shadowSrc: 2, shadowLevel: 0.76 }],
    ]) {
      const file = "shadow-" + name.replace(" ", "-");
      // the same source with the shadow worked out and then not applied, so what
      // is being measured is the shadow and not the lighting changing underneath
      await pose(page, { ...rig, ...over, shadowDark: 0.0 });
      const none = await shot(page, file + "-none");
      await pose(page, { ...rig, ...over, shadowDark: 1.0 });
      const full = await shot(page, file);
      shots.push(full);
      const d = darkened(none, full, 40);
      say(d > 0.01, `${name}, full contrast and amount, really darkens (${(d * 100).toFixed(1)}%)`);
    }
    say(!shots[0].equals(shots[1]) && !shots[1].equals(shots[2]) && !shots[0].equals(shots[2]),
        "and the three are three answers, not one in three hats");
  }

  // A matcap replaces the material outright, so there are only two things worth
  // asking of it: that it does something, that the two built-in spheres do
  // different things, and that switching it off puts the page back exactly as it
  // was — the same no-op discipline the sun is held to.
  if (mode === "check") {
    const base = join(OUT, "chrome-base.png");
    await pose(page, { scene: 2, shadowOn: 0, matcapOn: 1, matcap: 0 });
    const mcA = await shot(page, "matcap-chrome");
    await pose(page, { scene: 2, shadowOn: 0, matcapOn: 1, matcap: 1 });
    const mcB = await shot(page, "matcap-normals");
    say(!mcA.equals(mcB), "the two built-in matcaps give different pictures");
    if (existsSync(base)) {
      const off = readFileSync(base);
      say(!mcA.equals(off), "a matcap replaces the lit material");
      await pose(page, { scene: 2, shadowOn: 0, matcapOn: 0 });
      say((await shot(page, "matcap-off")).equals(off),
          "and switching it off is byte-identical to the baseline");
    }

    // and one loaded from disk, which is the half of this that has no built-in
    // to fall back on: eight pixels of flat magenta, so the object drawn with it
    // could not be mistaken for anything else.
    const loaded = join(OUT, "loaded.png");
    writeFileSync(loaded, Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR42mP4r3ECK2IYWhIAaFh7wVa+/gkAAAAASUVORK5CYII=",
      "base64"));
    await page.setInputFiles("#mcFile", loaded);
    await page.waitForTimeout(500);
    const list = await page.evaluate(() =>
      Array.prototype.map.call(document.querySelectorAll("#mcSeg button"), b => b.textContent));
    say(list.length === 3 && list[2] === "loaded", `a loaded sphere joins the list (${list.join(", ")})`);
    say(await page.evaluate(() => window.__state.matcapOn === 1 && window.__state.matcap === 2),
        "and is switched on and selected");
    const drawn = await shot(page, "matcap-loaded");
    say(!drawn.equals(mcA) && !drawn.equals(mcB), "and the object is drawn with it");
  }

  // The cube is the one shadow anybody can check by eye, and the only test here
  // that asks whether the feature WORKS rather than whether it changed. A box
  // over flat ground with the sun well up: take the caster away and nothing else,
  // and the pixels that got lighter are its shadow and nothing else.
  {
    const rig = {
      scene: 2, warpOn: [0, 0, 0], warpAmt: 0, shadowOn: 1,
      shadowSoft: 0.0, shadowDark: 1.0, shadowReach: 4.0, sunAz: 0.9, sunEl: 0.9,
      fly: 0, mode: 0, dragAz: 0.0, dragEl: 0.50, zoom: 0.46,
    };
    await pose(page, { ...rig, lcOn: [1, 0, 0, 0, 0, 0] });
    const noCube = await shot(page, "cube-absent");
    await pose(page, { ...rig, lcOn: [1, 0, 0, 0, 0, 1] });
    const withCube = await shot(page, "cube-present");
    const sh = darkened(noCube, withCube, 40);
    say(sh > 0.01, `the test cube casts a shadow on the slab (${(sh * 100).toFixed(1)}% of the frame)`);

    // and it is the sun that does it, not the cube merely being in the way
    await pose(page, { ...rig, lcOn: [1, 0, 0, 0, 0, 1], shadowOn: 0 });
    const cubeUnlit = await shot(page, "cube-nosun");
    const gone = darkened(cubeUnlit, withCube, 40);
    say(gone > 0.01, `and the shadow is gone with the sun switched off (${(gone * 100).toFixed(1)}%)`);
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
